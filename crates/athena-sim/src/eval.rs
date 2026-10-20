//! Value expressions.

use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::scenario::{EventCtx, Player};
use crate::{MissingInput, MissingInputKind, NotModeled, Stop, Val};

pub(crate) struct Env<'a> {
    pub globals: &'a BTreeMap<String, Val>,
    pub players: &'a BTreeMap<String, Player>,
    /// The injected event the running rule was triggered by, if any.
    pub event: Option<&'a EventCtx>,
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

pub(crate) fn number(value: &Val) -> Result<f64, Stop> {
    match value {
        Val::Number(n) => Ok(*n),
        Val::Null => Ok(0.0),
        other => unsupported(format!("arithmetic on {other:?}")),
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
        Value::EventPlayer => match env.event.and_then(|event| event.player.as_ref()) {
            Some(id) => Ok(Val::Player(id.clone())),
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
        "add" => binary(|a, b| a + b),
        "subtract" => binary(|a, b| a - b),
        "multiply" => binary(|a, b| a * b),
        "<" | ">=" => {
            let [a, b] = arguments::<2>(name, args)?;
            let (a, b) = (number(&eval(a, env)?)?, number(&eval(b, env)?)?);
            Ok(Val::Bool(if name == "<" { a < b } else { a >= b }))
        }
        "==" => {
            let [a, b] = arguments::<2>(name, args)?;
            let (a, b) = (eval(a, env)?, eval(b, env)?);
            Ok(Val::Bool(match (&a, &b) {
                (Val::Number(_) | Val::Null, Val::Number(_) | Val::Null) => {
                    number(&a)? == number(&b)?
                }
                (Val::Bool(a), Val::Bool(b)) => a == b,
                (Val::Text(a), Val::Text(b)) => a == b,
                (Val::Player(a), Val::Player(b)) => a == b,
                (Val::Array(_), _) | (_, Val::Array(_)) => {
                    return unsupported("== on an array");
                }
                _ => false,
            }))
        }
        "array" => args
            .iter()
            .map(|item| eval(item, env))
            .collect::<Result<_, _>>()
            .map(Val::Array),
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
        "getHero" | "teamOf" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            let declared = env.players.get(&id).and_then(|player| match name {
                "getHero" => player.hero.clone(),
                _ => player.team.map(|team| team.as_str().to_string()),
            });
            match declared {
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
