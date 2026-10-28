//! Catalog-call values: the `Value::Call` arms — arithmetic, arrays,
//! strings and the `Random *` draws a scenario's `random` source feeds.
//! The player-roster and player-state reads have their own dispatcher
//! in `player.rs`.

use workshop_rs::Value;

use crate::ambiguous;
use crate::eval::{Env, arguments, eval, number, values_equal};
use crate::scenario::{Phase, ServerLoad};
use crate::unsupported;
use crate::{Change, MissingInput, MissingInputKind, Stop, Val};

fn missing<T>(
    input: MissingInputKind,
    player: Option<String>,
    name: Option<String>,
    construct: impl Into<String>,
) -> Result<T, Stop> {
    Err(MissingInput::new(input, player, name, construct).into())
}

/// Event-scoped values the simulator resolves from an event's `values`
/// map — the scenario `values` of an injected event, or the bindings a
/// modeled cause (`Damage`/`Heal`) attaches to the events it raises.
/// Enum- and vector-typed event values (`eventAbility`, `eventDirection`)
/// are not modeled.
const EVENT_VALUES: &[&str] = &[
    "attacker",
    "eventDamage",
    "eventHealing",
    "eventWasCriticalHit",
    "eventWasEnvironment",
    "eventWasHealthPack",
    "victim",
];

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
        // A `Custom String` checks for the value before `render` runs;
        // a string-read reaching one through `string()` stops here.
        Val::Display { .. } => unsupported("a display value inside a string"),
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
        // The clock `Set Match Time` writes; the lobby's phase limit is
        // no input the scenario format carries, so a read before any
        // write is unanswerable — `not_modeled`, not a guessed default.
        "getMatchTime" => {
            let [] = arguments::<0>(name, args)?;
            match env.match_time {
                Some(remaining) => Ok(Val::Number(remaining)),
                None => unsupported(format!("value {name} before a Set Match Time")),
            }
        }
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
        // Modeled, not measured: the lobby chooses the game mode outside
        // the script, so the scenario's `gameMode` supplies it as a
        // constant — the catalog `Gamemode` member id as text, the same
        // form a `Game Mode` literal yields. The field is optional but
        // there is no default; a read without it is a missing input, and
        // a mid-run change is not modeled.
        "getCurrentGamemode" => {
            let [] = arguments::<0>(name, args)?;
            match env.game_mode {
                Some(mode) => Ok(Val::Text(mode.to_string())),
                None => missing(
                    MissingInputKind::GameMode,
                    None,
                    Some("gameMode".to_string()),
                    format!("value {name}"),
                ),
            }
        }
        // `Game Mode(...)`: the constant constructor — its argument is a
        // `Gamemode` enum literal, which yields the member id as text.
        "gameMode" => {
            let [mode] = arguments::<1>(name, args)?;
            eval(mode, env)
        }
        // Modeled, not measured: the match phase is a scenario fact —
        // the simulator does not run a match, so `matchPhase` supplies
        // it as a constant. The catalog's six phase reads each describe
        // exactly one phase, so a read answers true exactly for its own
        // phase; there is no default, so a read without `matchPhase` is
        // a missing input.
        "isWaitingForPlayers"
        | "isAssemblingHeroes"
        | "isInSetup"
        | "isGameInProgress"
        | "isMatchBetweenRounds"
        | "isMatchComplete" => {
            let [] = arguments::<0>(name, args)?;
            let own = match name {
                "isWaitingForPlayers" => Phase::WaitingForPlayers,
                "isAssemblingHeroes" => Phase::AssemblingHeroes,
                "isInSetup" => Phase::Setup,
                "isGameInProgress" => Phase::GameInProgress,
                "isMatchBetweenRounds" => Phase::BetweenRounds,
                _ => Phase::MatchComplete,
            };
            match env.match_phase {
                Some(phase) => Ok(Val::Bool(phase == own)),
                None => missing(
                    MissingInputKind::MatchPhase,
                    None,
                    Some("matchPhase".to_string()),
                    format!("value {name}"),
                ),
            }
        }
        "lastTextId" => {
            let [] = arguments::<0>(name, args)?;
            Ok(env
                .hud
                .borrow()
                .last_text_id
                .map_or(Val::Null, |id| Val::Number(id as f64)))
        }
        // `Last Created Entity` is the id of the newest durable entity;
        // a `playEffect` has no entity id to hand out.
        "lastCreatedEntity" => {
            let [] = arguments::<0>(name, args)?;
            Ok(env
                .effects
                .borrow()
                .last_created
                .map_or(Val::Null, |id| Val::Number(id as f64)))
        }
        // The catalog's `Player|EntityId` names "the specified player,
        // icon entity, or effect entity". A scenario player exists for
        // the whole run — no action changes the roster; a number
        // answers from the entity table and `Null` names nothing.
        "entityExists" => {
            let [entity] = arguments::<1>(name, args)?;
            match eval(entity, env)? {
                Val::Player(_) => Ok(Val::Bool(true)),
                Val::Number(id) => Ok(Val::Bool(env.effects.borrow().entity_exists(id))),
                Val::Null => Ok(Val::Bool(false)),
                other => unsupported(format!("{name} on {other:?}")),
            }
        }
        // "The current number" counts the live elements — a destroyed
        // one is no longer an instance.
        "getNumberOfEntityIds" => {
            let [] = arguments::<0>(name, args)?;
            Ok(Val::Number(env.effects.borrow().entity_count() as f64))
        }
        "getNumberOfTextIds" => {
            let [] = arguments::<0>(name, args)?;
            Ok(Val::Number(env.hud.borrow().text_count() as f64))
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
        // `Sine From Radians`: plain math on the evaluated argument; a
        // non-finite argument has no sine to report.
        "sin" => {
            let [value] = arguments::<1>(name, args)?;
            let sine = number(&eval(value, env)?)?.sin();
            if !sine.is_finite() {
                return unsupported(format!("{name} produced a non-finite number"));
            }
            Ok(Val::Number(sine))
        }
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
        "lastOf" => {
            let [array] = arguments::<1>(name, args)?;
            Ok(items(eval(array, env)?, name)?
                .into_iter()
                .last()
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
        // `Remove From Array` drops every element equal to the operand —
        // "one or more values removed (if found)", the same removal
        // `Remove From Array By Value` performs. An array operand drops
        // every element equal to any element it holds — "each matching
        // element is removed".
        "removeFromArray" => {
            let [array, wanted] = arguments::<2>(name, args)?;
            let (array, wanted) = (items(eval(array, env)?, name)?, eval(wanted, env)?);
            let mut kept = Vec::new();
            for item in array {
                let removed = match &wanted {
                    Val::Array(removals) => {
                        let mut removed = false;
                        for removal in removals {
                            if values_equal(&item, removal)? {
                                removed = true;
                                break;
                            }
                        }
                        removed
                    }
                    wanted => values_equal(&item, wanted)?,
                };
                if !removed {
                    kept.push(item);
                }
            }
            Ok(Val::Array(kept))
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
        // Modeled, not measured: a non-array operand has no element at
        // any index, so the catalog's "element does not exist" answer —
        // the same `Null` an out-of-range index yields — applies to it.
        "valueInArray" => {
            let [array, index] = arguments::<2>(name, args)?;
            let (array, index) = (eval(array, env)?, number(&eval(index, env)?)?);
            match array {
                Val::Array(items) if index >= 0.0 => {
                    Ok(items.get(index as usize).cloned().unwrap_or(Val::Null))
                }
                _ => Ok(Val::Null),
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
        // The display-only producers: the value is a client rendering
        // token — a localized icon glyph, a per-player key name or a
        // color — whose content nothing in the model can read. It
        // carries the producer and its evaluated arguments, never the
        // token (athena#113).
        "iconString" | "heroIconString" | "abilityIconString" | "inputBindingString"
        | "customColor" => {
            let arity = match name {
                "abilityIconString" => 2,
                "customColor" => 4,
                _ => 1,
            };
            if args.len() != arity {
                return unsupported(format!("{name} with {} arguments", args.len()));
            }
            let evaluated = args
                .iter()
                .map(|arg| eval(arg, env))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Val::Display {
                producer: name.to_string(),
                args: evaluated,
            })
        }
        "customString" => {
            let [template, rest @ ..] = args else {
                return unsupported("customString without a template");
            };
            let Val::Text(mut text) = eval(template, env)? else {
                return unsupported("customString template that is not a string");
            };
            let mut evaluated = Vec::with_capacity(rest.len());
            for arg in rest {
                evaluated.push(eval(arg, env)?);
            }
            // A `Custom String` holding a display value is itself
            // display: the rendered text would carry the token's
            // content. The record keeps the producer and the evaluated
            // arguments — template first — so the trace shows
            // `customString("{0}", iconString(Icon(WARNING)))`, not an
            // invented rendering (athena#113).
            if evaluated
                .iter()
                .any(|arg| matches!(arg, Val::Display { .. }))
            {
                evaluated.insert(0, Val::Text(text));
                return Ok(Val::Display {
                    producer: name.to_string(),
                    args: evaluated,
                });
            }
            let mut rendered = Vec::new();
            for arg in &evaluated {
                rendered.push(render(arg)?);
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
        // The player-roster and player-state family has its own
        // dispatcher, then the vector family; each ends the unknown
        // names it owns in `not_modeled` the same way.
        other => crate::player::call(other, args, env)
            .unwrap_or_else(|| crate::vector::call(other, args, env)),
    }
}
