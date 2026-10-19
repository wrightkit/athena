//! The tick loop and rule instances.

use std::collections::BTreeMap;

use workshop_rs::{Action, Event, ModifyOp, Program, Rule, Value};

use crate::eval::{Env, eval, number};
use crate::{LogEntry, Options, SimError, Trace, Val};

/// One server tick (P0, P1).
const TICK_S: f64 = 0.016;
/// `Total Time Elapsed` on the first tick. After `Wait(3)` (188 ticks) every
/// capture reads t0 = 3.023.
const START_OFFSET_S: f64 = 0.015;
const STEP_LIMIT: usize = 1_000_000;

fn unsupported<T>(what: impl Into<String>) -> Result<T, SimError> {
    Err(SimError::Unsupported(what.into()))
}

/// Ticks a `Wait(seconds)` lasts: rounded up, never less than one (P1, P4).
fn wait_ticks(seconds: f64) -> u64 {
    ((seconds * 1000.0 / (TICK_S * 1000.0) - 1e-9).ceil() as u64).max(1)
}

struct Instance<'a> {
    rule: &'a Rule,
    /// For each `While` its `End`, and the other way round.
    pairs: Vec<Option<usize>>,
    pc: usize,
    started: bool,
    wake: Option<u64>,
}

enum Flow {
    Suspended,
    Finished,
}

struct World {
    globals: BTreeMap<String, Val>,
    logs: Vec<LogEntry>,
}

fn pair_blocks(actions: &[Action]) -> Result<Vec<Option<usize>>, SimError> {
    let mut pairs = vec![None; actions.len()];
    let mut open = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        match action {
            Action::While { .. } => open.push(index),
            Action::End => {
                let Some(start) = open.pop() else {
                    return unsupported("End without an opening action");
                };
                pairs[start] = Some(index);
                pairs[index] = Some(start);
            }
            _ => {}
        }
    }
    Ok(pairs)
}

fn load(program: &Program) -> Result<Vec<Instance<'_>>, SimError> {
    let mut instances = Vec::new();
    for rule in program.rules.iter().filter(|rule| !rule.disabled) {
        if rule.event != Event::Global {
            return unsupported(format!("rule `{}`: event {:?}", rule.name, rule.event));
        }
        if rule.conditions.iter().any(|condition| !condition.disabled) {
            return unsupported(format!("rule `{}`: conditions", rule.name));
        }
        instances.push(Instance {
            rule,
            pairs: pair_blocks(&rule.actions)?,
            pc: 0,
            started: false,
            wake: None,
        });
    }
    Ok(instances)
}

fn flag(value: Val) -> Result<bool, SimError> {
    match value {
        Val::Bool(b) => Ok(b),
        other => unsupported(format!("condition value {other:?}")),
    }
}

fn run_instance(
    instance: &mut Instance<'_>,
    world: &mut World,
    tick: u64,
    now: f64,
) -> Result<Flow, SimError> {
    let actions = &instance.rule.actions;
    for _ in 0..STEP_LIMIT {
        let Some(action) = actions.get(instance.pc) else {
            return Ok(Flow::Finished);
        };
        let env = Env {
            globals: &world.globals,
            now,
        };
        match action {
            Action::Disabled { .. } => instance.pc += 1,
            Action::SetGlobalVariable { variable, value } => {
                let value = eval(value, &env)?;
                world.globals.insert(variable.clone(), value);
                instance.pc += 1;
            }
            Action::ModifyGlobalVariable {
                variable,
                op,
                value,
            } => {
                let operand = number(&eval(value, &env)?)?;
                let current = number(world.globals.get(variable).unwrap_or(&Val::Null))?;
                let updated = match op {
                    ModifyOp::Add => current + operand,
                    ModifyOp::Subtract => current - operand,
                    ModifyOp::Multiply => current * operand,
                    other => return unsupported(format!("modify operation {other:?}")),
                };
                world.globals.insert(variable.clone(), Val::Number(updated));
                instance.pc += 1;
            }
            Action::While { condition } => {
                instance.pc = if flag(eval(condition, &env)?)? {
                    instance.pc + 1
                } else {
                    instance.pairs[instance.pc].expect("paired at load") + 1
                };
            }
            Action::End => instance.pc = instance.pairs[instance.pc].expect("paired at load"),
            Action::Call { name, args } => match name.as_str() {
                "logToInspector" => {
                    let [message] = args.as_slice() else {
                        return unsupported("logToInspector arguments");
                    };
                    let Val::Text(text) = eval(message, &env)? else {
                        return unsupported("logToInspector of a non-string");
                    };
                    world.logs.push(LogEntry { tick, text });
                    instance.pc += 1;
                }
                "wait" => {
                    let [seconds, behavior] = args.as_slice() else {
                        return unsupported("wait arguments");
                    };
                    match behavior {
                        Value::Enum { value, .. } if value == "IGNORE_CONDITION" => {}
                        other => return unsupported(format!("wait behavior {other:?}")),
                    }
                    let ticks = wait_ticks(number(&eval(seconds, &env)?)?);
                    instance.wake = Some(tick + ticks);
                    instance.pc += 1;
                    return Ok(Flow::Suspended);
                }
                other => return unsupported(format!("action {other}")),
            },
            other => return unsupported(format!("action {other:?}")),
        }
    }
    unsupported(format!(
        "rule `{}` ran {STEP_LIMIT} steps without a Wait",
        instance.rule.name
    ))
}

pub(crate) fn run(program: &Program, options: Options) -> Result<Trace, SimError> {
    let mut instances = load(program)?;
    let mut world = World {
        globals: program
            .global_variables
            .iter()
            .map(|variable| (variable.name.clone(), Val::Null))
            .collect(),
        logs: Vec::new(),
    };
    let ticks = (options.seconds / TICK_S).floor() as u64 + 1;
    for tick in 0..ticks {
        let now = START_OFFSET_S + tick as f64 * TICK_S;
        for instance in &mut instances {
            let due = match instance.wake {
                Some(wake) => wake <= tick,
                None => !instance.started,
            };
            if !due {
                continue;
            }
            instance.started = true;
            instance.wake = None;
            if let Flow::Finished = run_instance(instance, &mut world, tick, now)? {
                instance.wake = Some(u64::MAX);
            }
        }
    }
    Ok(Trace {
        logs: world.logs,
        globals: world.globals,
    })
}
