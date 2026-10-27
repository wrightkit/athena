//! The `Modify` operator set and the array-element writes the
//! `Set`/`Modify * Variable At Index` actions make: `apply_modify` for
//! the whole-value operators, `set_at`/`modify_at` for an element, each
//! reading `Null` as the empty array.

use std::collections::BTreeMap;

use workshop_rs::{ModifyOp, Value};

use crate::eval::{number, values_equal};
use crate::scenario::Player;
use crate::unsupported;
use crate::vector::arithmetic;
use crate::{Stop, Val};

/// The array a `Modify` array operation reads: an array as itself, `Null`
/// as the empty array. Anything else is not an array.
fn array_operand(value: Val) -> Result<Vec<Val>, Stop> {
    match value {
        Val::Array(items) => Ok(items),
        Val::Null => Ok(Vec::new()),
        other => unsupported(format!("array modify on {other:?}")),
    }
}

/// The value a `Modify` operation writes from the variable's `old` value
/// and the evaluated `operand`: `Add`/`Subtract`/`Multiply`/`Divide` run
/// the vector-aware arithmetic of the value forms, reading a `Player`
/// operand's `position` from `players`; `Modulo`/`Min`/`Max`/`Raise To
/// Power` read both as numbers; the array operations keep `operand`
/// whole. `Remove From Array By Value` drops every element equal to
/// `operand`.
pub(crate) fn apply_modify(
    old: Val,
    op: &ModifyOp,
    operand: Val,
    players: &BTreeMap<String, Player>,
) -> Result<Val, Stop> {
    Ok(match op {
        ModifyOp::Add | ModifyOp::Subtract | ModifyOp::Multiply | ModifyOp::Divide => {
            arithmetic(op.catalog_id(), old, operand, players)?
        }
        ModifyOp::Modulo => Val::Number(number(&old)? % number(&operand)?),
        ModifyOp::Min => Val::Number(number(&old)?.min(number(&operand)?)),
        ModifyOp::Max => Val::Number(number(&old)?.max(number(&operand)?)),
        ModifyOp::RaiseToPower => Val::Number(number(&old)?.powf(number(&operand)?)),
        ModifyOp::AppendToArray => {
            let mut items = array_operand(old)?;
            items.push(operand);
            Val::Array(items)
        }
        ModifyOp::RemoveFromArrayByValue => {
            let mut kept = Vec::new();
            for item in array_operand(old)? {
                if !values_equal(&item, &operand)? {
                    kept.push(item);
                }
            }
            Val::Array(kept)
        }
        ModifyOp::RemoveFromArrayByIndex => {
            let mut items = array_operand(old)?;
            let index = number(&operand)?;
            if index >= 0.0 && (index as usize) < items.len() {
                items.remove(index as usize);
            }
            Val::Array(items)
        }
        other => return unsupported(format!("modify operation {other:?}")),
    })
}

/// Write `value` at `index` of the array `slot` holds — `Null` counts as an
/// empty array and an index past the end appends; a negative index writes
/// nothing. Returns the effective index, the element `old` and the stored
/// `new`, or `None` when nothing was written.
pub(crate) fn set_at(
    slot: &mut Val,
    index: f64,
    value: Val,
) -> Result<Option<(u64, Val, Val)>, Stop> {
    if index < 0.0 {
        return Ok(None);
    }
    let mut items = match slot {
        Val::Array(items) => std::mem::take(items),
        Val::Null => Vec::new(),
        other => return unsupported(format!("array-element write on {other:?}")),
    };
    let effective = (index as usize).min(items.len());
    let old = if effective == items.len() {
        items.push(value.clone());
        Val::Null
    } else {
        std::mem::replace(&mut items[effective], value.clone())
    };
    *slot = Val::Array(items);
    Ok(Some((effective as u64, old, value)))
}

/// Apply `op` with `operand` to the element at `index` of the array `slot`
/// holds; an index outside the array writes nothing. Returns like
/// [`set_at`].
pub(crate) fn modify_at(
    slot: &mut Val,
    index: f64,
    op: &ModifyOp,
    operand: Val,
    players: &BTreeMap<String, Player>,
) -> Result<Option<(u64, Val, Val)>, Stop> {
    let items = match slot {
        Val::Array(items) => items,
        Val::Null => return Ok(None),
        other => return unsupported(format!("array-element write on {other:?}")),
    };
    if index < 0.0 || index as usize >= items.len() {
        return Ok(None);
    }
    let index = index as usize;
    let old = std::mem::replace(&mut items[index], Val::Null);
    let new = apply_modify(old.clone(), op, operand, players)?;
    items[index] = new.clone();
    Ok(Some((index as u64, old, new)))
}

/// The `Modify` operation a `Modify * At Index` action's operator argument
/// names — a zero-argument `Value::Call` on an operator catalog id.
pub(crate) fn modify_op(value: &Value) -> Result<ModifyOp, Stop> {
    if let Value::Call { name, args } = value
        && args.is_empty()
        && let Some(op) = ModifyOp::from_catalog_id(name)
    {
        return Ok(op);
    }
    unsupported(format!("modify operation {value:?}"))
}
