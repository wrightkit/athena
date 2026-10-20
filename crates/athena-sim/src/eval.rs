//! Value expressions.

use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::{NotModeled, Val};

pub(crate) struct Env<'a> {
    pub globals: &'a BTreeMap<String, Val>,
    pub now: f64,
}

fn unsupported<T>(what: impl Into<String>) -> Result<T, NotModeled> {
    Err(NotModeled::new(what))
}

pub(crate) fn number(value: &Val) -> Result<f64, NotModeled> {
    match value {
        Val::Number(n) => Ok(*n),
        Val::Null => Ok(0.0),
        other => unsupported(format!("arithmetic on {other:?}")),
    }
}

fn render(value: &Val) -> Result<String, NotModeled> {
    match value {
        Val::Number(n) => Ok(format!("{n:.2}")),
        Val::Null => Ok("0".into()),
        Val::Bool(b) => Ok(b.to_string()),
        Val::Text(text) => Ok(text.clone()),
        Val::Array(_) => unsupported("array inside a custom string"),
    }
}

fn arguments<'a, const N: usize>(
    name: &str,
    args: &'a [Value],
) -> Result<&'a [Value; N], NotModeled> {
    args.try_into()
        .or_else(|_| unsupported(format!("{name} with {} arguments", args.len())))
}

pub(crate) fn eval(value: &Value, env: &Env) -> Result<Val, NotModeled> {
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
        Value::Call { name, args } => call(name, args, env),
        other => unsupported(format!("value {other:?}")),
    }
}

fn call(name: &str, args: &[Value], env: &Env) -> Result<Val, NotModeled> {
    let binary = |op: fn(f64, f64) -> f64| -> Result<Val, NotModeled> {
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
        "<" => {
            let [a, b] = arguments::<2>(name, args)?;
            Ok(Val::Bool(number(&eval(a, env)?)? < number(&eval(b, env)?)?))
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
