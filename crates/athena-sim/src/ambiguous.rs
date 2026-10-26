//! The `__ambiguous_enum` sentinel: `workshop-rs` gives a bare member
//! spelling it cannot pin to one enum domain the call
//! `__ambiguous_enum(spelling, [candidates])`, and leaves the
//! disambiguation by parameter type to the caller. Where the catalog
//! types the argument as a vector or number operand the sentinel
//! resolves to its `Vector` member's enum literal; elsewhere it stays a
//! sentinel and evaluation stops naming the spelling.

use std::borrow::Cow;

use workshop_rs::Value;

use crate::{NotModeled, Stop, Val};

/// The sentinel's call id.
pub(crate) const CALL: &str = "__ambiguous_enum";

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotModeled(NotModeled::new(what)))
}

/// The `Vector` member among a sentinel's candidates: the enum
/// candidate with `value_type` `Vector`, when there is one.
fn vector_member(args: &[Value]) -> Option<&str> {
    let Some(Value::Array(candidates)) = args.get(1) else {
        return None;
    };
    candidates.iter().find_map(|candidate| match candidate {
        Value::Enum { value_type, value } if value_type == "Vector" => Some(value.as_str()),
        _ => None,
    })
}

/// Rewrite a call's `__ambiguous_enum` arguments before dispatch: where
/// the catalog parameter type names a vector or number operand, the
/// argument becomes the `Vector` member it stands for. Everywhere else
/// the sentinel stays, annotated with the argument position for the
/// stop message.
pub(crate) fn resolve_operands(name: &str, args: &[Value]) -> Vec<Value> {
    let entry = workshop_rs::catalog::Catalog::builtin_ref()
        .ok()
        .and_then(|catalog| catalog.entry(workshop_rs::catalog::Kind::Value, name));
    let mut args = args.to_vec();
    for (index, arg) in args.iter_mut().enumerate() {
        let Value::Call {
            name: call,
            args: inner,
        } = arg
        else {
            continue;
        };
        if call.as_str() != CALL {
            continue;
        }
        let operand = entry
            .and_then(|entry| entry.param_type(index))
            .is_some_and(|param_type| {
                param_type
                    .split('|')
                    .any(|ty| matches!(ty, "Vector" | "Number"))
            });
        let member = if operand {
            vector_member(inner).map(str::to_string)
        } else {
            None
        };
        match member {
            Some(member) => {
                *arg = Value::Enum {
                    value_type: "Vector".to_string(),
                    value: member,
                };
            }
            None => inner.push(Value::String(
                entry.and_then(|entry| entry.param_name(index)).map_or_else(
                    || format!("argument {index} of `{name}`"),
                    |param| format!("`{param}` argument of `{name}`"),
                ),
            )),
        }
    }
    args
}

/// `resolve_operands` for a call whose arguments may hold no sentinel:
/// the borrowed arguments when no `__ambiguous_enum` is among them.
pub(crate) fn resolve_args<'a>(name: &str, args: &'a [Value]) -> Cow<'a, [Value]> {
    if args
        .iter()
        .any(|arg| matches!(arg, Value::Call { name, .. } if name == CALL))
    {
        resolve_operands(name, args).into()
    } else {
        args.into()
    }
}

/// A sentinel no operand position resolved: the run stops naming the
/// spelling, and the annotated position when there is one.
pub(crate) fn unresolved(args: &[Value]) -> Result<Val, Stop> {
    let spelling = match args.first() {
        Some(Value::String(spelling)) => spelling.as_str(),
        _ => "?",
    };
    match args.get(2) {
        Some(Value::String(position)) => {
            unsupported(format!("value {CALL} `{spelling}` in {position}"))
        }
        _ => unsupported(format!("value {CALL} `{spelling}`")),
    }
}
