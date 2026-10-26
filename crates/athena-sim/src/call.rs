//! Catalog-call values: the `Value::Call` arms — arithmetic, arrays,
//! player-state reads, strings and the `Random *` draws a scenario's
//! `random` source feeds.

use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::ambiguous;
use crate::eval::{Env, arguments, button_slot, eval, number, player_id, values_equal};
use crate::scenario::{Player, ServerLoad, Team};
use crate::state::WorldSlot;
use crate::{
    Change, MissingInput, MissingInputKind, NotModeled, PlayerWorld, PoolState, Stop, Val,
};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotModeled(NotModeled::new(what)))
}

fn missing<T>(
    input: MissingInputKind,
    player: Option<String>,
    name: Option<String>,
    construct: impl Into<String>,
) -> Result<T, Stop> {
    Err(Stop::MissingInput(MissingInput::new(
        input, player, name, construct,
    )))
}

/// Event-scoped values the simulator resolves from an injected event's
/// `values` map. Player-, enum- and vector-typed event values (`attacker`,
/// `victim`, `hostPlayer`, `eventAbility`, `eventDirection`) are not modeled.
const EVENT_VALUES: &[&str] = &[
    "eventDamage",
    "eventHealing",
    "eventWasCriticalHit",
    "eventWasEnvironment",
    "eventWasHealthPack",
];

/// The resolved world state of a scenario player, as the player-state
/// reading values (`Health`, `Max Health`, `Ability Cooldown`) see it. A
/// player the scenario did not declare, or whose state failed to resolve at
/// run start, is reported, never defaulted — reading world state is a touch,
/// like `Damage`/`Heal`.
fn player_world<'a>(
    players: &'a BTreeMap<String, Player>,
    id: &str,
) -> Result<&'a PlayerWorld, Stop> {
    let Some(player) = players.get(id) else {
        return unsupported(format!("`{id}` is not a scenario player"));
    };
    match &player.world {
        WorldSlot::Resolved(state) => Ok(&state.world),
        WorldSlot::Failed(error) => Err(Stop::NotModeled(error.clone())),
        WorldSlot::Unresolved => unreachable!("player world state is resolved at run start"),
    }
}

/// The truthiness a condition-family value (`and`, `or`, `not`,
/// `ifThenElse`) reads: a boolean as itself, a number or `Null` (as `0`)
/// by nonzero-ness. Anything else is not a condition value.
fn truthy(value: &Val) -> Result<bool, Stop> {
    match value {
        Val::Bool(b) => Ok(*b),
        Val::Number(n) => Ok(*n != 0.0),
        Val::Null => Ok(false),
        other => unsupported(format!("condition value {other:?}")),
    }
}

/// The array an array-reading value sees: an array as itself, `Null` as
/// the empty array. Anything else is not an array.
fn items(value: Val, construct: &str) -> Result<Vec<Val>, Stop> {
    match value {
        Val::Array(items) => Ok(items),
        Val::Null => Ok(Vec::new()),
        other => unsupported(format!("{construct} on {other:?}")),
    }
}

fn render(value: &Val) -> Result<String, Stop> {
    match value {
        Val::Number(n) => Ok(format!("{n:.2}")),
        Val::Null => Ok("0".into()),
        Val::Bool(b) => Ok(b.to_string()),
        Val::Text(text) => Ok(text.clone()),
        Val::Array(_) => unsupported("array inside a custom string"),
        Val::Player(_) => unsupported("player inside a custom string"),
        // The client's render form of a vector is unmeasured.
        Val::Vector { .. } => unsupported("vector inside a custom string"),
    }
}

/// The produced string a value may return. Whether the client still
/// caps strings at 511 bytes is unmeasured — the sources conflict on
/// whether the limit was removed or covers every produced string — so
/// a produced result past the boundary stops the run rather than pick
/// a side.
fn produced(text: String, construct: &str) -> Result<Val, Stop> {
    if text.len() > 511 {
        unsupported(format!("{construct} produced a string over 511 bytes"))
    } else {
        Ok(Val::Text(text))
    }
}

/// The string a string-parameter argument reads as: text as itself, an
/// empty array as the empty string (the Workshop spells an empty string
/// `Empty Array` in these parameters), and a number, boolean or `Null`
/// by the `render` coercion. A non-empty array is not a string.
fn string(value: &Val) -> Result<String, Stop> {
    match value {
        Val::Array(items) if items.is_empty() => Ok(String::new()),
        Val::Array(_) => unsupported("non-empty array where a string is needed"),
        other => render(other),
    }
}

/// The `Env` an element-context array value's per-element operand sees:
/// the caller's `env` with `element` bound to the element and its index.
fn element_env<'a>(env: &Env<'a>, element: &'a Val, index: usize) -> Env<'a> {
    Env {
        element: Some((element, index as u64)),
        ..*env
    }
}

/// The deterministic random source a scenario's `random.seed` supplies:
/// a splitmix64 generator. A `Random *` value consumes draws in
/// evaluation order, each draw yielding one fraction in `[0, 1)`. The
/// client's generator is unmeasured; the stream shape is a modeled
/// choice, not a measured one.
pub(crate) struct Random(u64);

impl Random {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// One draw as a fraction in `[0, 1)`, from the draw's top 53 bits.
    fn fraction(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9007199254740992.0)
    }
}

/// Consume one draw of the scenario's `random` source, or stop the run
/// when the scenario supplies none.
fn draw(env: &Env, construct: &str) -> Result<f64, Stop> {
    match env.random {
        Some(random) => Ok(random.borrow_mut().fraction()),
        None => missing(
            MissingInputKind::Random,
            None,
            Some("random".to_string()),
            format!("value {construct}"),
        ),
    }
}

/// Record a `Random *` value's draw on the tick's changes and yield its
/// produced value.
fn drawn(env: &Env, construct: &str, value: Val) -> Val {
    env.changes.borrow_mut().push(Change::RandomDraw {
        rule: env.rule,
        construct: construct.to_string(),
        value: value.clone(),
    });
    value
}

pub(crate) fn call(name: &str, args: &[Value], env: &Env) -> Result<Val, Stop> {
    // `__ambiguous_enum` arguments resolve to their `Vector` member in a
    // vector or number operand position; elsewhere they stay sentinels
    // and evaluation stops at them naming the spelling.
    let resolved = ambiguous::resolve_args(name, args);
    let args: &[Value] = &resolved;
    let binary = |op: fn(f64, f64) -> f64| -> Result<Val, Stop> {
        let [a, b] = arguments::<2>(name, args)?;
        Ok(Val::Number(op(
            number(&eval(a, env)?)?,
            number(&eval(b, env)?)?,
        )))
    };
    match name {
        // Evaluation reached a sentinel no operand position resolved —
        // a rule condition, an action argument or a non-operand call
        // argument — so the run stops naming the spelling.
        ambiguous::CALL => ambiguous::unresolved(args),
        "getTotalTimeElapsed" => Ok(Val::Number(env.now)),
        // Modeled, not measured: the client measures load per tick; the
        // simulator reads the scenario's `serverLoad` constants. The
        // field is optional in the format but there is no default —
        // without it the read is a missing input.
        "getServerLoad" | "getAverageServerLoad" | "getPeakServerLoad" => {
            let [] = arguments::<0>(name, args)?;
            match env.server_load {
                Some(ServerLoad {
                    current,
                    average,
                    peak,
                }) => Ok(Val::Number(match name {
                    "getServerLoad" => current,
                    "getAverageServerLoad" => average,
                    _ => peak,
                })),
                None => missing(
                    MissingInputKind::ServerLoad,
                    None,
                    Some("serverLoad".to_string()),
                    format!("value {name}"),
                ),
            }
        }
        // Modeled, not measured: the lobby chooses the map outside the
        // script, so the scenario's `map` supplies it as a constant — the
        // catalog `Map` member id as text, the same form a `Map` literal
        // yields. The field is optional but there is no default; a read
        // without it is a missing input, and a mid-run change is not
        // modeled.
        "currentMap" => {
            let [] = arguments::<0>(name, args)?;
            match env.map {
                Some(map) => Ok(Val::Text(map.to_string())),
                None => missing(
                    MissingInputKind::Map,
                    None,
                    Some("map".to_string()),
                    format!("value {name}"),
                ),
            }
        }
        "lastTextId" => {
            let [] = arguments::<0>(name, args)?;
            Ok(env
                .last_text_id
                .map_or(Val::Null, |id| Val::Number(id as f64)))
        }
        // Modeled, not measured: `Last Created Entity` is the id of the
        // newest `createEffect` entity; other entity-creating actions are
        // not modeled.
        "lastCreatedEntity" => {
            let [] = arguments::<0>(name, args)?;
            Ok(env
                .last_entity
                .map_or(Val::Null, |id| Val::Number(id as f64)))
        }
        "allPlayers" => {
            let [team] = arguments::<1>(name, args)?;
            // The `Team` argument is an enum literal in practice; an
            // evaluated member-id text (as `Team Of` yields) names the
            // same member.
            let filter = match team {
                Value::Enum { value, .. } => value.clone(),
                other => match eval(other, env)? {
                    Val::Text(text) => text,
                    other => return unsupported(format!("allPlayers on {other:?}")),
                },
            };
            let members = |team: Team| {
                env.players
                    .iter()
                    .filter(|(_, player)| player.team == Some(team))
                    .map(|(id, _)| Val::Player(id.clone()))
                    .collect()
            };
            Ok(Val::Array(match filter.as_str() {
                "ALL" => env
                    .players
                    .keys()
                    .map(|id| Val::Player(id.clone()))
                    .collect(),
                "TEAM_1" => members(Team::Team1),
                "TEAM_2" => members(Team::Team2),
                other => return unsupported(format!("allPlayers on {other}")),
            }))
        }
        // Modeled, not measured: the client's element order is
        // unmeasured, so the catalog's `Hero` member order is the
        // order. Elements are the member ids `Hero Of` yields, so
        // `Array Contains(All Heroes(), Hero Of(p))` holds.
        "allHeroes" => {
            let [] = arguments::<0>(name, args)?;
            let catalog = workshop_rs::catalog::Catalog::builtin_ref()
                .map_err(|error| NotModeled::new(format!("catalog unavailable: {error}")))?;
            let Some(domain) = catalog.enum_domain("Hero") else {
                return unsupported("catalog has no `Hero` domain");
            };
            Ok(Val::Array(
                domain
                    .members
                    .iter()
                    .map(|member| Val::Text(member.member.clone()))
                    .collect(),
            ))
        }
        "and" | "or" => {
            let [a, b] = arguments::<2>(name, args)?;
            // `&&`/`||` short-circuit: the right side is not read when the
            // left already decides the result.
            if truthy(&eval(a, env)?)? == (name == "or") {
                return Ok(Val::Bool(name == "or"));
            }
            Ok(Val::Bool(truthy(&eval(b, env)?)?))
        }
        "not" => {
            let [value] = arguments::<1>(name, args)?;
            Ok(Val::Bool(!truthy(&eval(value, env)?)?))
        }
        "ifThenElse" => {
            let [condition, when_true, when_false] = arguments::<3>(name, args)?;
            // Only the taken branch is read.
            if truthy(&eval(condition, env)?)? {
                eval(when_true, env)
            } else {
                eval(when_false, env)
            }
        }
        "hostPlayer" => {
            let [] = arguments::<0>(name, args)?;
            match env.players.iter().find(|(_, player)| player.host) {
                Some((id, _)) => Ok(Val::Player(id.clone())),
                None => unsupported("Host Player: the scenario marks no host player"),
            }
        }
        // Modeled, not measured: the simulator runs one lobby whose settings
        // nobody edits, so a Workshop Setting yields its declared `default`
        // argument. `min`/`max`, `options` and `sortOrder` are authoring
        // metadata the running rules never see; the scenario format has no
        // settings overrides.
        "createWorkshopSettingFloat"
        | "workshopSettingInteger"
        | "workshopSettingToggle"
        | "workshopSettingCombo"
        | "createWorkshopSettingHero" => {
            let [_, _, default, ..] = args else {
                return unsupported(format!("{name} with {} arguments", args.len()));
            };
            match default {
                // A hero setting's default is a `Hero` literal; it yields the
                // catalog member id as text, the same member id `Hero Of`
                // yields, so a hero setting and a played hero compare equal.
                Value::Enum { value, .. } => Ok(Val::Text(value.clone())),
                _ => eval(default, env),
            }
        }
        // Modeled, not measured: a player with a hero is spawned and alive.
        "hasSpawned" | "isAlive" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            match env.players.get(&id) {
                Some(player) if player.hero.is_some() => Ok(Val::Bool(true)),
                Some(_) => unsupported(format!("{name} of player `{id}` without a hero")),
                None => unsupported(format!("`{id}` is not a scenario player")),
            }
        }
        // `Number|Boolean|Vector` (and a `Player`, read as its
        // `position`): two numbers yield a number, anything vector-sided
        // combines component-wise with a scalar broadcast.
        "add" | "subtract" | "multiply" | "divide" => {
            let [a, b] = arguments::<2>(name, args)?;
            crate::vector::arithmetic(name, eval(a, env)?, eval(b, env)?, env.players)
        }
        "modulo" => binary(|a, b| a % b),
        "raiseToPower" => binary(f64::powf),
        "min" => binary(f64::min),
        "max" => binary(f64::max),
        "roundToInteger" => {
            let [value, rounding] = arguments::<2>(name, args)?;
            let n = number(&eval(value, env)?)?;
            let Value::Enum { value: mode, .. } = rounding else {
                return unsupported(format!("{name} rounding {rounding:?}"));
            };
            Ok(Val::Number(match mode.as_str() {
                "UP" => n.ceil(),
                "DOWN" => n.floor(),
                "NEAREST" => n.round(),
                other => return unsupported(format!("{name} rounding {other}")),
            }))
        }
        "<" | "<=" | ">" | ">=" => {
            let [a, b] = arguments::<2>(name, args)?;
            let (a, b) = (number(&eval(a, env)?)?, number(&eval(b, env)?)?);
            Ok(Val::Bool(match name {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            }))
        }
        "==" | "!=" => {
            let [a, b] = arguments::<2>(name, args)?;
            let (a, b) = (eval(a, env)?, eval(b, env)?);
            Ok(Val::Bool(values_equal(&a, &b)? != (name == "!=")))
        }
        "array" => args
            .iter()
            .map(|item| eval(item, env))
            .collect::<Result<_, _>>()
            .map(Val::Array),
        "emptyArray" => {
            let [] = arguments::<0>(name, args)?;
            Ok(Val::Array(Vec::new()))
        }
        "firstOf" => {
            let [array] = arguments::<1>(name, args)?;
            Ok(items(eval(array, env)?, name)?
                .into_iter()
                .next()
                .unwrap_or(Val::Null))
        }
        "countOf" => {
            let [array] = arguments::<1>(name, args)?;
            Ok(Val::Number(items(eval(array, env)?, name)?.len() as f64))
        }
        "appendToArray" => {
            let [array, value] = arguments::<2>(name, args)?;
            let (mut array, value) = (items(eval(array, env)?, name)?, eval(value, env)?);
            array.push(value);
            Ok(Val::Array(array))
        }
        "arrayContains" | "indexOfArrayValue" => {
            let [array, wanted] = arguments::<2>(name, args)?;
            let (array, wanted) = (eval(array, env)?, eval(wanted, env)?);
            let mut found = None;
            for (index, item) in items(array, name)?.iter().enumerate() {
                if values_equal(item, &wanted)? {
                    found = Some(index);
                    break;
                }
            }
            Ok(match name {
                "arrayContains" => Val::Bool(found.is_some()),
                _ => Val::Number(found.map_or(-1.0, |index| index as f64)),
            })
        }
        "slice" => {
            let [array, start, count] = arguments::<3>(name, args)?;
            let (array, start, count) = (
                items(eval(array, env)?, name)?,
                number(&eval(start, env)?)?,
                number(&eval(count, env)?)?,
            );
            let (start, end) = (
                (start.max(0.0) as usize).min(array.len()),
                (start.max(0.0) + count.max(0.0)) as usize,
            );
            Ok(Val::Array(array[start..end.min(array.len())].to_vec()))
        }
        "valueInArray" => {
            let [array, index] = arguments::<2>(name, args)?;
            let (array, index) = (eval(array, env)?, number(&eval(index, env)?)?);
            match array {
                Val::Array(items) if index >= 0.0 => {
                    Ok(items.get(index as usize).cloned().unwrap_or(Val::Null))
                }
                Val::Array(_) => Ok(Val::Null),
                other => unsupported(format!("valueInArray on {other:?}")),
            }
        }
        // Modeled, not measured: the client's generator is unmeasured.
        // The draws come from the scenario's `random` source in
        // evaluation order; without one the value is a missing input.
        "randomInteger" | "randomReal" => {
            let [min, max] = arguments::<2>(name, args)?;
            let (min, max) = (number(&eval(min, env)?)?, number(&eval(max, env)?)?);
            if !min.is_finite() || !max.is_finite() {
                return unsupported(format!("{name} on a non-finite bound"));
            }
            // A `Min` past `Max` swaps the bounds; unmeasured.
            let (min, max) = if min > max { (max, min) } else { (min, max) };
            // Finite bounds still overflow the range: `Max - Min` is
            // `inf` from `-1e308` to `1e308`, and a non-finite value
            // serializes as `null` in the trace.
            if !(max - min).is_finite() {
                return unsupported(format!("{name} on a non-finite range"));
            }
            let u = draw(env, name)?;
            let value = if name == "randomInteger" {
                // The bounds floor to integers; the range is inclusive.
                let (lo, hi) = (min.floor(), max.floor());
                Val::Number(lo + (u * (hi - lo + 1.0)).floor())
            } else {
                Val::Number(min + (max - min) * u)
            };
            Ok(drawn(env, name, value))
        }
        "randomValueInArray" => {
            let [array] = arguments::<1>(name, args)?;
            let array = items(eval(array, env)?, name)?;
            // An empty array yields `Null` without a draw.
            if array.is_empty() {
                return Ok(Val::Null);
            }
            let index = (draw(env, name)? * array.len() as f64) as usize;
            Ok(drawn(env, name, array[index].clone()))
        }
        "randomizedArray" => {
            let [array] = arguments::<1>(name, args)?;
            let mut array = items(eval(array, env)?, name)?;
            // A Fisher-Yates shuffle from the same stream, last index
            // down; an empty or one-element array draws nothing and
            // records nothing.
            if array.len() < 2 {
                return Ok(Val::Array(array));
            }
            for i in (1..array.len()).rev() {
                let j = (draw(env, name)? * (i + 1) as f64) as usize;
                array.swap(i, j);
            }
            Ok(drawn(env, name, Val::Array(array)))
        }
        "currentArrayElement" | "currentArrayIndex" => {
            let [] = arguments::<0>(name, args)?;
            match env.element {
                Some((element, index)) => Ok(if name == "currentArrayElement" {
                    element.clone()
                } else {
                    Val::Number(index as f64)
                }),
                None => unsupported(format!("{name} outside an element context")),
            }
        }
        // The element-context array values read the array once, then the
        // operand once per element with `Current Array Element`/`Current
        // Array Index` bound. `Null` reads as the empty array.
        "filteredArray" | "sortedArray" | "isTrueForAll" | "isTrueForAny" => {
            let [array, operand] = arguments::<2>(name, args)?;
            let items = items(eval(array, env)?, name)?;
            match name {
                "filteredArray" => {
                    let mut kept = Vec::new();
                    for (index, item) in items.iter().enumerate() {
                        if truthy(&eval(operand, &element_env(env, item, index))?)? {
                            kept.push(item.clone());
                        }
                    }
                    Ok(Val::Array(kept))
                }
                "sortedArray" => {
                    let mut keyed = Vec::with_capacity(items.len());
                    for (index, item) in items.iter().enumerate() {
                        let key = number(&eval(operand, &element_env(env, item, index))?)?;
                        keyed.push((key, item));
                    }
                    // Ascending by the key; an equal key keeps the
                    // element's place.
                    keyed.sort_by(|(a, _), (b, _)| a.total_cmp(b));
                    Ok(Val::Array(
                        keyed.into_iter().map(|(_, item)| item.clone()).collect(),
                    ))
                }
                // `Is True For All`/`Any` stop at the element that decides
                // them: the first false, the first true.
                _ => {
                    let want = name == "isTrueForAny";
                    for (index, item) in items.iter().enumerate() {
                        if truthy(&eval(operand, &element_env(env, item, index))?)? == want {
                            return Ok(Val::Bool(want));
                        }
                    }
                    Ok(Val::Bool(!want))
                }
            }
        }
        "getHero" | "teamOf" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            // `Hero Of` yields the catalog member id the declared `hero`
            // resolves to — hero text compares by member id, not by the
            // scenario's spelling; `Team Of` likewise yields the member id
            // of the declared `team`.
            let resolved = env.players.get(&id).and_then(|player| match name {
                "getHero" => player.hero_member.clone(),
                _ => player.team.map(|team| team.member_id().to_string()),
            });
            match resolved {
                Some(value) => Ok(Val::Text(value)),
                None => missing(
                    if name == "getHero" {
                        MissingInputKind::Hero
                    } else {
                        MissingInputKind::Team
                    },
                    Some(id),
                    None,
                    format!("value {name}"),
                ),
            }
        }
        name if EVENT_VALUES.contains(&name) => {
            let [] = arguments::<0>(name, args)?;
            match env.event.and_then(|event| event.values.get(name)).cloned() {
                Some(value) => Ok(value),
                None => missing(
                    MissingInputKind::EventValue,
                    None,
                    Some(name.to_string()),
                    format!("value {name}"),
                ),
            }
        }
        "getHealth" | "getMaxHealth" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            let world = player_world(env.players, &id)?;
            let pools = [
                Some(&world.health),
                world.shield.as_ref(),
                world.armor.as_ref(),
            ];
            let pick = if name == "getHealth" {
                |pool: &PoolState| pool.current
            } else {
                |pool: &PoolState| pool.max
            };
            Ok(Val::Number(pools.into_iter().flatten().map(pick).sum()))
        }
        // Modeled, not measured: a status is held while its `Set Status`
        // expiry is still ahead.
        "hasStatus" => {
            let [player, status] = arguments::<2>(name, args)?;
            let id = player_id(player, env)?;
            let Value::Enum { value: status, .. } = status else {
                return unsupported(format!("{name} status {status:?}"));
            };
            let world = player_world(env.players, &id)?;
            Ok(Val::Bool(
                world
                    .statuses
                    .get(status.as_str())
                    .is_some_and(|until| *until > env.now),
            ))
        }
        // A live input: the held state comes from the scenario player's
        // `buttons` map; a button not listed is a missing input.
        "isButtonHeld" => {
            let [player, button] = arguments::<2>(name, args)?;
            let id = player_id(player, env)?;
            let slot = button_slot(button)?;
            match env.players.get(&id) {
                Some(player) => match player.held.get(slot) {
                    Some(held) => Ok(Val::Bool(*held)),
                    None => missing(
                        MissingInputKind::Button,
                        Some(id),
                        Some(slot.to_string()),
                        "value isButtonHeld",
                    ),
                },
                None => unsupported(format!("`{id}` is not a scenario player")),
            }
        }
        "getAbilityCooldown" => {
            let [player, button] = arguments::<2>(name, args)?;
            let id = player_id(player, env)?;
            let slot = button_slot(button)?;
            let world = player_world(env.players, &id)?;
            if let Some(seconds) = world.cooldowns.get(slot) {
                return Ok(Val::Number(*seconds));
            }
            // A slot recorded only under a variant (`slot/variant`) has no
            // honest answer for the bare button; a slot with no recorded
            // cooldown at all reports 0 — the ability is never on cooldown.
            if world
                .cooldowns
                .keys()
                .any(|key| key.starts_with(&format!("{slot}/")))
            {
                return unsupported(format!("ability cooldown on variant-keyed slot {slot}"));
            }
            Ok(Val::Number(0.0))
        }
        "customString" => {
            let [template, rest @ ..] = args else {
                return unsupported("customString without a template");
            };
            let Val::Text(mut text) = eval(template, env)? else {
                return unsupported("customString template that is not a string");
            };
            let mut rendered = Vec::new();
            for arg in rest {
                rendered.push(render(&eval(arg, env)?)?);
            }
            for (index, value) in rendered.iter().enumerate() {
                text = text.replace(&format!("{{{index}}}"), value);
            }
            produced(text, name)
        }
        // `Char In String` reads the character at the index; an index
        // outside the string's bounds yields the empty string.
        "charAt" => {
            let [source, index] = arguments::<2>(name, args)?;
            let (source, index) = (string(&eval(source, env)?)?, number(&eval(index, env)?)?);
            Ok(source
                .chars()
                .nth(if index < 0.0 {
                    usize::MAX
                } else {
                    index as usize
                })
                .map_or_else(
                    || Val::Text(String::new()),
                    |char| Val::Text(char.to_string()),
                ))
        }
        // `String Split` splits around the separator; an empty separator
        // splits into characters and an empty source yields no parts.
        "stringSplit" => {
            let [source, separator] = arguments::<2>(name, args)?;
            let (source, separator) = (
                string(&eval(source, env)?)?,
                string(&eval(separator, env)?)?,
            );
            if source.is_empty() {
                return Ok(Val::Array(Vec::new()));
            }
            let parts: Vec<String> = if separator.is_empty() {
                source.chars().map(|char| char.to_string()).collect()
            } else {
                source
                    .split(&separator)
                    .map(|part| part.to_string())
                    .collect()
            };
            parts
                .into_iter()
                .map(|part| produced(part, name))
                .collect::<Result<_, _>>()
                .map(Val::Array)
        }
        // `Index Of String Char` reports the needle's first index as a
        // character count, or -1 when the needle is absent — and an
        // empty needle, which names no character, is always absent.
        "strIndex" => {
            let [source, character] = arguments::<2>(name, args)?;
            let (source, character) = (
                string(&eval(source, env)?)?,
                string(&eval(character, env)?)?,
            );
            Ok(Val::Number(if character.is_empty() {
                -1.0
            } else {
                source
                    .find(&character)
                    .map_or(-1.0, |byte| source[..byte].chars().count() as f64)
            }))
        }
        // `String Replace` replaces every occurrence of the pattern. An
        // empty pattern's occurrences are undefined, so the run stops.
        "stringReplace" => {
            let [source, pattern, replacement] = arguments::<3>(name, args)?;
            let (source, pattern, replacement) = (
                string(&eval(source, env)?)?,
                string(&eval(pattern, env)?)?,
                string(&eval(replacement, env)?)?,
            );
            if pattern.is_empty() {
                return unsupported("stringReplace with an empty pattern");
            }
            produced(source.replace(&pattern, &replacement), name)
        }
        // The vector family has its own dispatcher; it ends unknown
        // names in `not_modeled` the same way.
        other => crate::vector::call(other, args, env),
    }
}
