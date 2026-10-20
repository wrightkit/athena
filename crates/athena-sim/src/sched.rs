//! The tick loop and rule instances.

use std::collections::BTreeMap;

use workshop_rs::{Action, Event, ModifyOp, Program, Rule, Value};

use crate::eval::{Env, eval, number};
use crate::{LogEntry, NotModeled, Options, Outcome, Trace, Val};

/// One server tick (P0, P1).
const TICK_S: f64 = 0.016;
/// `Total Time Elapsed` on the first tick. After `Wait(3)` (188 ticks) every
/// capture reads t0 = 3.023.
const START_OFFSET_S: f64 = 0.015;
const STEP_LIMIT: usize = 1_000_000;

fn unsupported<T>(what: impl Into<String>) -> Result<T, NotModeled> {
    Err(NotModeled::new(what))
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
    /// The tick a suspended run resumes on; `None` while idle.
    wake: Option<u64>,
    /// Condition result on the previous tick. A rule starts on a false-to-true edge.
    prev: bool,
}

struct World {
    globals: BTreeMap<String, Val>,
    logs: Vec<LogEntry>,
}

fn pair_blocks(actions: &[Action]) -> Result<Vec<Option<usize>>, NotModeled> {
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

fn load(program: &Program) -> Result<Vec<Instance<'_>>, NotModeled> {
    let mut instances = Vec::new();
    for rule in program.rules.iter().filter(|rule| !rule.disabled) {
        if rule.event != Event::Global {
            return unsupported(format!("rule `{}`: event {:?}", rule.name, rule.event));
        }
        instances.push(Instance {
            rule,
            pairs: pair_blocks(&rule.actions)?,
            pc: 0,
            wake: None,
            prev: false,
        });
    }
    Ok(instances)
}

fn conditions_hold(rule: &Rule, env: &Env) -> Result<bool, NotModeled> {
    for condition in rule
        .conditions
        .iter()
        .filter(|condition| !condition.disabled)
    {
        if !flag(eval(&condition.value, env)?)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn flag(value: Val) -> Result<bool, NotModeled> {
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
) -> Result<(), NotModeled> {
    let actions = &instance.rule.actions;
    for _ in 0..STEP_LIMIT {
        let Some(action) = actions.get(instance.pc) else {
            return Ok(());
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
                    return Ok(());
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

pub(crate) fn run(program: &Program, options: Options) -> Trace {
    let mut world = World {
        globals: program
            .global_variables
            .iter()
            .map(|variable| (variable.name.clone(), Val::Null))
            .collect(),
        logs: Vec::new(),
    };
    let outcome =
        match load(program).and_then(|instances| tick_loop(instances, &mut world, options)) {
            Ok(()) => Outcome::Complete,
            Err(not_modeled) => Outcome::NotModeled(not_modeled),
        };
    Trace {
        outcome,
        logs: world.logs,
        globals: world.globals,
    }
}

fn tick_loop(
    mut instances: Vec<Instance<'_>>,
    world: &mut World,
    options: Options,
) -> Result<(), NotModeled> {
    let ticks = (options.seconds / TICK_S).floor() as u64 + 1;
    for tick in 0..ticks {
        let now = START_OFFSET_S + tick as f64 * TICK_S;
        for instance in &mut instances {
            // The condition is tracked every tick, also during a Wait: an edge
            // while a run is suspended is dropped, not queued (P2).
            let holds = conditions_hold(
                instance.rule,
                &Env {
                    globals: &world.globals,
                    now,
                },
            )
            .map_err(|e| e.at(tick))?;
            let edge = holds && !instance.prev;
            instance.prev = holds;
            let start = match instance.wake {
                Some(wake) if wake <= tick => true,
                Some(_) => false,
                None => {
                    instance.pc = 0;
                    edge
                }
            };
            if !start {
                continue;
            }
            instance.wake = None;
            run_instance(instance, world, tick, now).map_err(|e| e.at(tick))?;
        }
    }
    Ok(())
}
