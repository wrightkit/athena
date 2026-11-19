use std::collections::BTreeMap;

use workshop_rs::{ModifyOp, Value};

use crate::eval::{number, values_equal};
use crate::produced_number;
use crate::scenario::Player;
use crate::unsupported;
use crate::vector::arithmetic;
use crate::{Stop, Val};

fn array_operand(value: Val) -> Result<Vec<Val>, Stop> {
    match value {
        Val::Array(items) => Ok(items),
        Val::Null => Ok(Vec::new()),
        other => unsupported(format!("array modify on {other:?}")),
    }
}

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
        ModifyOp::Modulo => produced_number(number(&old)? % number(&operand)?, op.catalog_id())?,
        ModifyOp::Min => produced_number(number(&old)?.min(number(&operand)?), op.catalog_id())?,
        ModifyOp::Max => produced_number(number(&old)?.max(number(&operand)?), op.catalog_id())?,
        ModifyOp::RaiseToPower => {
            produced_number(number(&old)?.powf(number(&operand)?), op.catalog_id())?
        }
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

pub(crate) fn modify_op(value: &Value) -> Result<ModifyOp, Stop> {
    if let Value::Call { name, args } = value
        && args.is_empty()
        && let Some(op) = ModifyOp::from_catalog_id(name)
    {
        return Ok(op);
    }
    unsupported(format!("modify operation {value:?}"))
}
