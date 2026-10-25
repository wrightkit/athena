use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::scenario::{EventCtx, Player, Team};
use crate::state::WorldSlot;
use crate::{MissingInput, MissingInputKind, NotModeled, PlayerWorld, PoolState, Stop, Val};

pub(crate) struct Env<'a> {
    pub globals: &'a BTreeMap<String, Val>,
    pub players: &'a BTreeMap<String, Player>,
    /// The injected event the running rule was triggered by, if any.
    pub event: Option<&'a EventCtx>,
    /// The player an `Ongoing - Each Player` instance runs for; `Event
    /// Player` reads it when no injected event supplies one.
    pub each_player: Option<&'a str>,
    /// The `id` of the newest `hudText`/`progressBarHud` element; what
    /// `Last Text ID` reads. `None` before the first one is created.
    pub last_text_id: Option<u64>,
    /// The element and place `Current Array Element`/`Current Array Index`
    /// read; an element-context array value binds it per element of its
    /// array argument. `None` outside one.
    pub element: Option<(&'a Val, u64)>,
    pub now: f64,
}

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

/// Resolve a Workshop player expression to a scenario player `id`.
pub(crate) fn player_id(value: &Value, env: &Env) -> Result<String, Stop> {
    match eval(value, env)? {
        Val::Player(id) => Ok(id),
        other => unsupported(format!("{other:?} where a player is needed")),
    }
}

/// Resolve a `Player|Array` argument, as `Damage` and `Heal` take, to the
/// `id`s of the players it names.
pub(crate) fn player_ids(value: &Value, env: &Env) -> Result<Vec<String>, Stop> {
    match eval(value, env)? {
        Val::Player(id) => Ok(vec![id]),
        Val::Array(items) => items
            .iter()
            .map(|item| match item {
                Val::Player(id) => Ok(id.clone()),
                other => unsupported(format!("{other:?} where a player is needed")),
            })
            .collect(),
        other => unsupported(format!("{other:?} where a player is needed")),
    }
}

/// Resolve a Workshop "players" expression — the `Visible To`/`player`
/// argument of a HUD action — to scenario player `id`s. A single player,
/// an array of players (what `allPlayers` produces), or `Null` (nobody).
pub(crate) fn viewers(value: &Value, env: &Env) -> Result<Vec<String>, Stop> {
    match eval(value, env)? {
        Val::Player(id) => Ok(vec![id]),
        Val::Array(items) => items
            .iter()
            .map(|item| match item {
                Val::Player(id) => Ok(id.clone()),
                other => unsupported(format!("{other:?} where a player is needed")),
            })
            .collect(),
        Val::Null => Ok(Vec::new()),
        other => unsupported(format!("{other:?} where players are needed")),
    }
}

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

/// The `Button` enum member to the cooldown slot name the player-state
/// table reports it under.
pub(crate) fn button_slot(button: &Value) -> Result<&'static str, Stop> {
    let Value::Enum { value, .. } = button else {
        return unsupported(format!("{button:?} where a button is needed"));
    };
    Ok(match value.as_str() {
        "ABILITY_1" => "ability1",
        "ABILITY_2" => "ability2",
        "SECONDARY_FIRE" => "secondaryFire",
        "PRIMARY_FIRE" => "primaryFire",
        "ULTIMATE" => "ultimate",
        "CROUCH" => "crouch",
        "INTERACT" => "interact",
        "JUMP" => "jump",
        "MELEE" => "melee",
        "RELOAD" => "reload",
        other => return unsupported(format!("button {other}")),
    })
}

pub(crate) fn number(value: &Val) -> Result<f64, Stop> {
    match value {
        Val::Number(n) => Ok(*n),
        Val::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Val::Null => Ok(0.0),
        other => unsupported(format!("arithmetic on {other:?}")),
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

/// The `Compare` equality of two evaluated values: numbers, booleans and
/// `Null` by number coercion (`Null` and `False` equal `0`), text by text,
/// players by scenario `id`, arrays by element-wise equality. Values of
/// different types are never equal.
pub(crate) fn values_equal(a: &Val, b: &Val) -> Result<bool, Stop> {
    Ok(match (a, b) {
        (Val::Array(a), Val::Array(b)) => {
            a.len() == b.len()
                && a.iter().zip(b.iter()).try_fold(true, |equal, (a, b)| {
                    Ok::<_, Stop>(equal && values_equal(a, b)?)
                })?
        }
        (Val::Number(_) | Val::Bool(_) | Val::Null, Val::Number(_) | Val::Bool(_) | Val::Null) => {
            number(a)? == number(b)?
        }
        (Val::Text(a), Val::Text(b)) => a == b,
        (Val::Player(a), Val::Player(b)) => a == b,
        _ => false,
    })
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

fn arguments<'a, const N: usize>(name: &str, args: &'a [Value]) -> Result<&'a [Value; N], Stop> {
    args.try_into()
        .or_else(|_| unsupported(format!("{name} with {} arguments", args.len())))
}

pub(crate) fn eval(value: &Value, env: &Env) -> Result<Val, Stop> {
    match value {
        Value::Number(n) => Ok(Val::Number(*n)),
        Value::Bool(b) => Ok(Val::Bool(*b)),
        Value::Null => Ok(Val::Null),
        Value::String(text) => Ok(Val::Text(text.clone())),
        Value::Array(items) => items
            .iter()
            .map(|item| eval(item, env))
            .collect::<Result<_, _>>()
            .map(Val::Array),
        // An enum literal in a value position yields its catalog member
        // id as text — the same form `Hero Of` and hero settings yield,
        // so `Compare(Hero Of(p), ==, Hero(Ana))` compares equal.
        Value::Enum { value, .. } => Ok(Val::Text(value.clone())),
        Value::GlobalVariable(name) => Ok(env.globals.get(name).cloned().unwrap_or(Val::Null)),
        Value::PlayerVariable { player, variable } => {
            let id = player_id(player, env)?;
            Ok(env
                .players
                .get(&id)
                .and_then(|player| player.vars.get(variable))
                .cloned()
                .unwrap_or(Val::Null))
        }
        Value::EventPlayer => match env
            .event
            .and_then(|event| event.player.as_deref())
            .or(env.each_player)
        {
            Some(id) => Ok(Val::Player(id.to_string())),
            None => missing(
                MissingInputKind::EventPlayer,
                None,
                None,
                "value eventPlayer",
            ),
        },
        Value::Call { name, args } => call(name, args, env),
        other => unsupported(format!("value {other:?}")),
    }
}

fn call(name: &str, args: &[Value], env: &Env) -> Result<Val, Stop> {
    let binary = |op: fn(f64, f64) -> f64| -> Result<Val, Stop> {
        let [a, b] = arguments::<2>(name, args)?;
        Ok(Val::Number(op(
            number(&eval(a, env)?)?,
            number(&eval(b, env)?)?,
        )))
    };
    match name {
        "getTotalTimeElapsed" => Ok(Val::Number(env.now)),
        "lastTextId" => {
            let [] = arguments::<0>(name, args)?;
            Ok(env
                .last_text_id
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
        "add" => binary(|a, b| a + b),
        "subtract" => binary(|a, b| a - b),
        "multiply" => binary(|a, b| a * b),
        "divide" => binary(|a, b| a / b),
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
        // `Evaluate Once` pins its operand against re-evaluation; the
        // simulator never re-reads a value site, so a plain read is the
        // whole model.
        "evaluateOnce" => {
            let [value] = arguments::<1>(name, args)?;
            eval(value, env)
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
            Ok(Val::Text(text))
        }
        other => unsupported(format!("value {other}")),
    }
}
