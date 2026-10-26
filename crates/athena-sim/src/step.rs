//! One step of a rule instance: the action interpreter `run_instance`
//! runs a frame stack until a `Wait` suspends it or it finishes, plus
//! the `World` it reads and writes, recording the state changes in
//! `world.changes` for the tick's timeline entry.

use std::cell::RefCell;
use std::collections::BTreeMap;

use workshop_rs::{Action, Rule, Value};

use crate::blocks::Blocks;
use crate::call::Random;
use crate::effects::Effects;
use crate::eval::{Env, eval, number, player_id};
use crate::hud::Hud;
use crate::load::{Frame, Instance, WaitMode};
use crate::modify::{apply_modify, modify_at, modify_op, set_at};
use crate::scenario::Player;
use crate::state::{self, WorldSlot};
use crate::{Change, LogEntry, NotModeled, RuleRun, Stop, Timeline, Val};

/// One server tick (P0, P1).
const TICK_S: f64 = 0.016;
const STEP_LIMIT: usize = 1_000_000;
/// Nested subroutine calls a run may make; the limit exists so a recursive
/// program cannot grow the frame stack without bound.
const CALL_DEPTH_LIMIT: usize = 128;

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotModeled(NotModeled::new(what)))
}

/// Ticks a `Wait(seconds)` lasts: rounded up, never less than one (P1, P4).
fn wait_ticks(seconds: f64) -> u64 {
    ((seconds * 1000.0 / (TICK_S * 1000.0) - 1e-9).ceil() as u64).max(1)
}

/// Whether a `For` control variable at `value` reached or passed the range
/// stop. The direction of "passed" follows the sign of `step`.
fn for_done(value: f64, stop: f64, step: f64) -> bool {
    if step >= 0.0 {
        value >= stop
    } else {
        value <= stop
    }
}

pub(crate) struct World {
    pub globals: BTreeMap<String, Val>,
    pub players: BTreeMap<String, Player>,
    pub logs: Vec<LogEntry>,
    pub hud: Hud,
    /// The effect entities of the run so far.
    pub effects: Effects,
    /// `Start Rule` requests as `(rule index, restart)`; applied when the
    /// target's instance is next visited in the tick loop.
    pub pending_starts: Vec<(usize, bool)>,
    /// The rule executions of the tick in progress — started, resumed or
    /// ended — in execution order; `tick_loop` drains them into the
    /// tick's timeline entry.
    pub runs: Vec<RuleRun>,
    /// The state changes of the tick in progress, in execution order;
    /// `tick_loop` drains them into the tick's timeline entry. A
    /// `RefCell` so an `Env` — which borrows the world's other fields —
    /// can record a `randomDraw`.
    pub changes: RefCell<Vec<Change>>,
    /// The scenario's `random` source; `None` when it supplies none.
    pub random: Option<RefCell<Random>>,
    /// The recorded timeline.
    pub timeline: Timeline,
}

/// The resolved state of a scenario player. A player the scenario did
/// not declare, or whose state failed to resolve at run start, is
/// reported, never defaulted: touching that player's world state —
/// `Damage`/`Heal` dealt or received, `Set Ability Cooldown` — stops the
/// run as `not_modeled`.
pub(crate) fn world_state<'w>(
    world: &'w mut World,
    id: &str,
) -> Result<&'w mut state::Resolved, Stop> {
    let Some(player) = world.players.get_mut(id) else {
        return unsupported(format!("`{id}` is not a scenario player"));
    };
    match &mut player.world {
        WorldSlot::Resolved(state) => Ok(state),
        WorldSlot::Failed(error) => Err(Stop::NotModeled(error.clone())),
        WorldSlot::Unresolved => unreachable!("player world state is resolved at run start"),
    }
}

pub(crate) fn flag(value: Val) -> Result<bool, Stop> {
    match value {
        Val::Bool(b) => Ok(b),
        other => unsupported(format!("condition value {other:?}")),
    }
}

/// Whether a rule's enabled conditions all hold under `env`, evaluated in
/// order. `Loop If Condition Is True`/`False` re-check this mid-run, the
/// same way the scheduler checks it for the run start.
pub(crate) fn conditions_hold(rule: &Rule, env: &Env) -> Result<bool, Stop> {
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

/// The `Number|Boolean` count argument of `Skip`/`Skip If`: `True` counts
/// one action, `False` and `Null` count none, a negative count skips
/// nothing.
fn skip_count(value: &Val) -> Result<usize, Stop> {
    match value {
        Val::Number(n) => Ok(n.max(0.0) as usize),
        Val::Bool(b) => Ok(*b as usize),
        Val::Null => Ok(0),
        other => unsupported(format!("skip count {other:?}")),
    }
}

/// The step at which `pc` moves past the frame's last action.
fn advance(inst: &mut Instance<'_>) {
    inst.frames.last_mut().expect("a running frame").pc += 1;
}

pub(crate) fn run_instance(
    inst: &mut Instance<'_>,
    rules: &[&Rule],
    blocks: &[Blocks],
    subroutines: &BTreeMap<String, usize>,
    world: &mut World,
    tick: u64,
    now: f64,
) -> Result<(), Stop> {
    for _ in 0..STEP_LIMIT {
        let Some(Frame { rule_idx, pc }) = inst.frames.last().copied() else {
            return Ok(());
        };
        let rule = rules[rule_idx];
        let map = &blocks[rule_idx];
        let Some(action) = rule.actions.get(pc) else {
            inst.frames.pop();
            continue;
        };
        let env = Env {
            globals: &world.globals,
            players: &world.players,
            event: inst.event.as_ref(),
            each_player: inst.each_player.as_deref(),
            last_text_id: world.hud.last_text_id,
            element: None,
            once: &inst.once,
            last_entity: world.effects.last_created,
            random: world.random.as_ref(),
            rule: rule_idx,
            changes: &world.changes,
            now,
        };
        match action {
            Action::Disabled { .. } => advance(inst),
            Action::SetGlobalVariable { variable, value } => {
                let value = eval(value, &env)?;
                let old = world.globals.get(variable).cloned().unwrap_or(Val::Null);
                world.globals.insert(variable.clone(), value.clone());
                world.changes.borrow_mut().push(Change::GlobalVariable {
                    name: variable.clone(),
                    index: None,
                    old,
                    new: value,
                });
                advance(inst);
            }
            Action::ModifyGlobalVariable {
                variable,
                op,
                value,
            } => {
                let operand = eval(value, &env)?;
                let old = world.globals.get(variable).cloned().unwrap_or(Val::Null);
                let updated = apply_modify(old.clone(), op, operand)?;
                world.globals.insert(variable.clone(), updated.clone());
                world.changes.borrow_mut().push(Change::GlobalVariable {
                    name: variable.clone(),
                    index: None,
                    old,
                    new: updated,
                });
                advance(inst);
            }
            Action::SetPlayerVariable {
                player,
                variable,
                value,
            } => {
                let id = player_id(player, &env)?;
                let value = eval(value, &env)?;
                let Some(target) = world.players.get_mut(&id) else {
                    return unsupported(format!("player variable of undeclared player `{id}`"));
                };
                let old = target.vars.get(variable).cloned().unwrap_or(Val::Null);
                target.vars.insert(variable.clone(), value.clone());
                world.changes.borrow_mut().push(Change::PlayerVariable {
                    player: id,
                    name: variable.clone(),
                    index: None,
                    old,
                    new: value,
                });
                advance(inst);
            }
            Action::ModifyPlayerVariable {
                player,
                variable,
                op,
                value,
            } => {
                let id = player_id(player, &env)?;
                let operand = eval(value, &env)?;
                let Some(target) = world.players.get_mut(&id) else {
                    return unsupported(format!("player variable of undeclared player `{id}`"));
                };
                let old = target.vars.get(variable).cloned().unwrap_or(Val::Null);
                let updated = apply_modify(old.clone(), op, operand)?;
                target.vars.insert(variable.clone(), updated.clone());
                world.changes.borrow_mut().push(Change::PlayerVariable {
                    player: id,
                    name: variable.clone(),
                    index: None,
                    old,
                    new: updated,
                });
                advance(inst);
            }
            Action::If { condition } => {
                if flag(eval(condition, &env)?)? {
                    // A new chain starts clean; a stale jump would read the
                    // next `ElseIf`/`Else` as a clause head to evaluate.
                    inst.clause_jump = None;
                    advance(inst);
                } else {
                    let target = map.next_clause[pc].expect("paired at load");
                    inst.clause_jump = Some(Frame {
                        rule_idx,
                        pc: target,
                    });
                    inst.frames.last_mut().expect("a running frame").pc = target;
                }
            }
            Action::ElseIf { condition } => {
                if inst.clause_jump != Some(Frame { rule_idx, pc }) {
                    inst.frames.last_mut().expect("a running frame").pc =
                        map.chain_end[pc].expect("paired at load") + 1;
                } else {
                    inst.clause_jump = None;
                    if flag(eval(condition, &env)?)? {
                        advance(inst);
                    } else {
                        let target = map.next_clause[pc].expect("paired at load");
                        inst.clause_jump = Some(Frame {
                            rule_idx,
                            pc: target,
                        });
                        inst.frames.last_mut().expect("a running frame").pc = target;
                    }
                }
            }
            Action::Else => {
                inst.frames.last_mut().expect("a running frame").pc =
                    if inst.clause_jump == Some(Frame { rule_idx, pc }) {
                        inst.clause_jump = None;
                        pc + 1
                    } else {
                        map.chain_end[pc].expect("paired at load") + 1
                    };
            }
            Action::While { condition } => {
                inst.frames.last_mut().expect("a running frame").pc =
                    if flag(eval(condition, &env)?)? {
                        pc + 1
                    } else {
                        map.pairs[pc].expect("paired at load") + 1
                    };
            }
            Action::ForGlobalVariable {
                variable,
                start,
                stop,
                step,
            } => {
                let (start, stop, step) = (
                    number(&eval(start, &env)?)?,
                    number(&eval(stop, &env)?)?,
                    number(&eval(step, &env)?)?,
                );
                let old = world.globals.get(variable).cloned().unwrap_or(Val::Null);
                world.globals.insert(variable.clone(), Val::Number(start));
                world.changes.borrow_mut().push(Change::GlobalVariable {
                    name: variable.clone(),
                    index: None,
                    old,
                    new: Val::Number(start),
                });
                // The loop-exit check runs at the `For` line once, then at
                // each `End`; a value already past the stop never enters.
                inst.frames.last_mut().expect("a running frame").pc = if for_done(start, stop, step)
                {
                    map.pairs[pc].expect("paired at load") + 1
                } else {
                    pc + 1
                };
            }
            Action::ForPlayerVariable {
                player,
                variable,
                start,
                stop,
                step,
            } => {
                let id = player_id(player, &env)?;
                let (start, stop, step) = (
                    number(&eval(start, &env)?)?,
                    number(&eval(stop, &env)?)?,
                    number(&eval(step, &env)?)?,
                );
                let Some(target) = world.players.get_mut(&id) else {
                    return unsupported(format!("player variable of undeclared player `{id}`"));
                };
                let old = target.vars.get(variable).cloned().unwrap_or(Val::Null);
                target.vars.insert(variable.clone(), Val::Number(start));
                world.changes.borrow_mut().push(Change::PlayerVariable {
                    player: id,
                    name: variable.clone(),
                    index: None,
                    old,
                    new: Val::Number(start),
                });
                inst.frames.last_mut().expect("a running frame").pc = if for_done(start, stop, step)
                {
                    map.pairs[pc].expect("paired at load") + 1
                } else {
                    pc + 1
                };
            }
            Action::End => {
                let open = map.pairs[pc].expect("paired at load");
                // `While` ends re-enter the head to re-check the condition;
                // `For` ends step the variable and re-check the bound.
                let next = match &rule.actions[open] {
                    Action::While { .. } => open,
                    Action::ForGlobalVariable {
                        variable,
                        stop,
                        step,
                        ..
                    } => {
                        let (stop, step) =
                            (number(&eval(stop, &env)?)?, number(&eval(step, &env)?)?);
                        let old = world.globals.get(variable).cloned().unwrap_or(Val::Null);
                        let updated = number(&old)? + step;
                        world.globals.insert(variable.clone(), Val::Number(updated));
                        world.changes.borrow_mut().push(Change::GlobalVariable {
                            name: variable.clone(),
                            index: None,
                            old,
                            new: Val::Number(updated),
                        });
                        if for_done(updated, stop, step) {
                            pc + 1
                        } else {
                            open + 1
                        }
                    }
                    Action::ForPlayerVariable {
                        player,
                        variable,
                        stop,
                        step,
                        ..
                    } => {
                        let id = player_id(player, &env)?;
                        let (stop, step) =
                            (number(&eval(stop, &env)?)?, number(&eval(step, &env)?)?);
                        let Some(target) = world.players.get_mut(&id) else {
                            return unsupported(format!(
                                "player variable of undeclared player `{id}`"
                            ));
                        };
                        let old = target.vars.get(variable).cloned().unwrap_or(Val::Null);
                        let updated = number(&old)? + step;
                        target.vars.insert(variable.clone(), Val::Number(updated));
                        world.changes.borrow_mut().push(Change::PlayerVariable {
                            player: id,
                            name: variable.clone(),
                            index: None,
                            old,
                            new: Val::Number(updated),
                        });
                        if for_done(updated, stop, step) {
                            pc + 1
                        } else {
                            open + 1
                        }
                    }
                    _ => pc + 1,
                };
                inst.frames.last_mut().expect("a running frame").pc = next;
            }
            Action::CallSubroutine { subroutine } => {
                let Some(&target) = subroutines.get(subroutine) else {
                    return unsupported(format!("subroutine {subroutine}"));
                };
                if inst.frames.len() >= CALL_DEPTH_LIMIT {
                    return unsupported(format!("subroutine {subroutine} call depth"));
                }
                advance(inst);
                inst.frames.push(Frame {
                    rule_idx: target,
                    pc: 0,
                });
            }
            Action::Call { name, args } => match name.as_str() {
                "abort" | "abortIf" => {
                    let fire = match name.as_str() {
                        "abort" => {
                            let [] = args.as_slice() else {
                                return unsupported("abort arguments");
                            };
                            true
                        }
                        _ => {
                            let [condition] = args.as_slice() else {
                                return unsupported("abortIf arguments");
                            };
                            flag(eval(condition, &env)?)?
                        }
                    };
                    // `Abort` kills the whole run, subroutine frames
                    // included; the run records `aborted`, not `finished`.
                    if fire {
                        inst.frames.clear();
                        inst.clause_jump = None;
                        inst.aborted = true;
                    } else {
                        advance(inst);
                    }
                }
                "skip" | "skipIf" => {
                    let (fire, count) = match args.as_slice() {
                        [count] if name == "skip" => (true, count),
                        [condition, count] if name == "skipIf" => {
                            (flag(eval(condition, &env)?)?, count)
                        }
                        _ => return unsupported(format!("{name} arguments")),
                    };
                    // The client reads action arguments left to right, so a
                    // `Skip If` count that cannot be evaluated stops the run
                    // even when the condition is false.
                    let count = skip_count(&eval(count, &env)?)?;
                    // A skip is positional: it can land inside a block the
                    // jump never entered, and the actions there run raw.
                    let skip = if fire { count.saturating_add(1) } else { 1 };
                    let frame = inst.frames.last_mut().expect("a running frame");
                    frame.pc = frame.pc.saturating_add(skip);
                }
                "loop" | "loopIf" | "loopIfConditionIsTrue" | "__loopIfConditionIsFalse__" => {
                    let again = match name.as_str() {
                        "loop" => {
                            let [] = args.as_slice() else {
                                return unsupported("loop arguments");
                            };
                            true
                        }
                        "loopIf" => {
                            let [condition] = args.as_slice() else {
                                return unsupported("loopIf arguments");
                            };
                            flag(eval(condition, &env)?)?
                        }
                        "loopIfConditionIsTrue" => {
                            // A condition re-check is its own
                            // `Evaluate Once` scope, like the
                            // scheduler's per-tick check.
                            let pass = RefCell::new(BTreeMap::new());
                            conditions_hold(rule, &Env { once: &pass, ..env })?
                        }
                        _ => {
                            let pass = RefCell::new(BTreeMap::new());
                            !conditions_hold(rule, &Env { once: &pass, ..env })?
                        }
                    };
                    // The restarts jump to the head of the action list the
                    // frame executes — inside a `Call Subroutine`, the
                    // subroutine's own.
                    inst.frames.last_mut().expect("a running frame").pc =
                        if again { 0 } else { pc + 1 };
                }
                "break" | "continue" => {
                    let [] = args.as_slice() else {
                        return unsupported(format!("{name} arguments"));
                    };
                    let Some(open) = map.enclosing[pc] else {
                        return unsupported(format!("{name} outside a loop"));
                    };
                    let end = map.pairs[open].expect("paired at load");
                    // `Continue` lands on the `End`, which steps a `For` or
                    // re-enters a `While` head; `Break` lands past it.
                    inst.frames.last_mut().expect("a running frame").pc =
                        if name == "break" { end + 1 } else { end };
                }
                "logToInspector" => {
                    let [message] = args.as_slice() else {
                        return unsupported("logToInspector arguments");
                    };
                    let Val::Text(text) = eval(message, &env)? else {
                        return unsupported("logToInspector of a non-string");
                    };
                    world.logs.push(LogEntry {
                        tick,
                        text: text.clone(),
                    });
                    world.changes.borrow_mut().push(Change::Log { text });
                    advance(inst);
                }
                "wait" => {
                    let [seconds, behavior] = args.as_slice() else {
                        return unsupported("wait arguments");
                    };
                    let mode = match behavior {
                        Value::Enum { value, .. } => match value.as_str() {
                            "IGNORE_CONDITION" => WaitMode::Ignore,
                            "ABORT_WHEN_FALSE" => WaitMode::AbortWhenFalse,
                            "RESTART_WHEN_TRUE" => WaitMode::RestartWhenTrue,
                            other => return unsupported(format!("wait behavior {other}")),
                        },
                        other => return unsupported(format!("wait behavior {other:?}")),
                    };
                    // The rule's condition is tracked for global rules only.
                    if mode != WaitMode::Ignore && inst.event.is_some() {
                        return unsupported("wait behavior in an event-triggered rule");
                    }
                    let ticks = wait_ticks(number(&eval(seconds, &env)?)?);
                    inst.wake = Some(tick + ticks);
                    inst.wait_mode = mode;
                    advance(inst);
                    return Ok(());
                }
                "waitUntil" => {
                    let [condition, timeout] = args.as_slice() else {
                        return unsupported("waitUntil arguments");
                    };
                    if inst.event.is_some() {
                        return unsupported("Wait Until in an event-triggered rule");
                    }
                    // One `Wait Until` call is an `Evaluate Once`
                    // scope: the per-tick re-checks the scheduler runs
                    // while the run is suspended share this first
                    // check's pins.
                    inst.until_once.get_mut().clear();
                    let until_env = Env {
                        once: &inst.until_once,
                        ..env
                    };
                    // A true condition continues in the same tick (P4).
                    if flag(eval(condition, &until_env)?)? {
                        advance(inst);
                        continue;
                    }
                    let timeout = number(&eval(timeout, &env)?)?;
                    if timeout <= 0.0 {
                        return unsupported("Wait Until(false, 0)");
                    }
                    inst.wake = Some(tick + wait_ticks(timeout));
                    inst.wait_mode = WaitMode::Until(Frame { rule_idx, pc });
                    advance(inst);
                    return Ok(());
                }
                "setGlobalVariableAtIndex" | "modifyGlobalVariableAtIndex" => {
                    let modify = name.as_str() == "modifyGlobalVariableAtIndex";
                    let [variable, index, rest @ ..] = args.as_slice() else {
                        return unsupported(format!("{name} arguments"));
                    };
                    let (op, operand) = match (modify, rest) {
                        (false, [operand]) => (None, operand),
                        (true, [op, operand]) => (Some(modify_op(op)?), operand),
                        _ => return unsupported(format!("{name} arguments")),
                    };
                    let Value::GlobalVariable(variable) = variable else {
                        return unsupported(format!("{name} variable {variable:?}"));
                    };
                    let index = number(&eval(index, &env)?)?;
                    let operand = eval(operand, &env)?;
                    let slot = world.globals.entry(variable.clone()).or_insert(Val::Null);
                    let written = match op {
                        None => set_at(slot, index, operand)?,
                        Some(op) => modify_at(slot, index, &op, operand)?,
                    };
                    if let Some((effective, old, new)) = written {
                        world.changes.borrow_mut().push(Change::GlobalVariable {
                            name: variable.clone(),
                            index: Some(vec![effective]),
                            old,
                            new,
                        });
                    }
                    advance(inst);
                }
                "setPlayerVariableAtIndex" | "modifyPlayerVariableAtIndex" => {
                    let modify = name.as_str() == "modifyPlayerVariableAtIndex";
                    // The parser merges a declared variable into
                    // `Value::PlayerVariable { player, variable }`; an
                    // expression in the variable position keeps the
                    // catalog's player-then-variable argument order.
                    let (player, variable, rest) = match args.as_slice() {
                        [
                            Value::PlayerVariable {
                                player, variable, ..
                            },
                            rest @ ..,
                        ] => (player.as_ref(), variable.clone(), rest),
                        [player, variable, rest @ ..] => {
                            let variable = match variable {
                                Value::PlayerVariable { variable, .. } => variable.clone(),
                                other => match eval(other, &env)? {
                                    Val::Text(name) => name,
                                    other => {
                                        return unsupported(format!("{name} variable {other:?}"));
                                    }
                                },
                            };
                            (player, variable, rest)
                        }
                        _ => return unsupported(format!("{name} arguments")),
                    };
                    let (op, index, operand) = match (modify, rest) {
                        (false, [index, operand]) => (None, index, operand),
                        (true, [index, op, operand]) => (Some(modify_op(op)?), index, operand),
                        _ => return unsupported(format!("{name} arguments")),
                    };
                    let id = player_id(player, &env)?;
                    let index = number(&eval(index, &env)?)?;
                    let operand = eval(operand, &env)?;
                    let Some(target) = world.players.get_mut(&id) else {
                        return unsupported(format!("player variable of undeclared player `{id}`"));
                    };
                    let slot = target.vars.entry(variable.clone()).or_insert(Val::Null);
                    let written = match op {
                        None => set_at(slot, index, operand)?,
                        Some(op) => modify_at(slot, index, &op, operand)?,
                    };
                    if let Some((effective, old, new)) = written {
                        world.changes.borrow_mut().push(Change::PlayerVariable {
                            player: id,
                            name: variable.clone(),
                            index: Some(vec![effective]),
                            old,
                            new,
                        });
                    }
                    advance(inst);
                }
                other => {
                    let result =
                        crate::hud::action(other, args, &env, &mut world.hud, tick, &world.changes)
                            .or_else(|| {
                                crate::effects::action(
                                    other,
                                    args,
                                    &env,
                                    &mut world.effects,
                                    tick,
                                    &world.changes,
                                )
                            });
                    // `env` shares a borrow of `world.players`, so it ends
                    // here and the delegated actions take `world` itself.
                    let result = result.or_else(|| {
                        crate::actions::action(other, args, world, inst, rule_idx, subroutines, now)
                    });
                    match result {
                        Some(result) => {
                            result?;
                            advance(inst);
                        }
                        None => return unsupported(format!("action {other}")),
                    }
                }
            },
            other => return unsupported(format!("action {other:?}")),
        }
    }
    unsupported(format!(
        "rule `{}` ran {STEP_LIMIT} steps without a Wait",
        inst.rule.name
    ))
}
