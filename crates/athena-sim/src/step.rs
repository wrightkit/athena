//! One step of a rule instance: the action interpreter `run_instance`
//! runs a frame stack until a `Wait` suspends it or it finishes, plus
//! the `World` it reads and writes, recording the state changes in
//! `world.changes` for the tick's timeline entry.

use std::collections::BTreeMap;

use workshop_rs::{Action, ModifyOp, Rule, Value};

use crate::blocks::Blocks;
use crate::eval::{Env, button_slot, eval, number, player_id, player_ids};
use crate::hud::Hud;
use crate::load::{Frame, Instance, WaitMode};
use crate::scenario::Player;
use crate::state::{self, WorldSlot};
use crate::{Change, LogEntry, NotModeled, PlayerWorld, RuleRun, Stop, Timeline, Val};

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
    /// The rule executions of the tick in progress, in execution order;
    /// `tick_loop` drains them into the tick's timeline entry.
    pub runs: Vec<RuleRun>,
    /// The state changes of the tick in progress, in execution order;
    /// `tick_loop` drains them into the tick's timeline entry.
    pub changes: Vec<Change>,
    /// The recorded timeline.
    pub timeline: Timeline,
}

/// The resolved state of a scenario player. A player the scenario did
/// not declare, or whose state failed to resolve at run start, is
/// reported, never defaulted: touching that player's world state —
/// `Damage`/`Heal` dealt or received, `Set Ability Cooldown` — stops the
/// run as `not_modeled`.
fn world_state<'w>(world: &'w mut World, id: &str) -> Result<&'w mut state::Resolved, Stop> {
    let Some(player) = world.players.get_mut(id) else {
        return unsupported(format!("`{id}` is not a scenario player"));
    };
    match &mut player.world {
        WorldSlot::Resolved(state) => Ok(state),
        WorldSlot::Failed(error) => Err(Stop::NotModeled(error.clone())),
        WorldSlot::Unresolved => unreachable!("player world state is resolved at run start"),
    }
}

/// A player's pool `current` points in absorb order, `(field, points)`;
/// `None` for a pool the player does not have.
fn pools(world: &PlayerWorld) -> [(&'static str, Option<f64>); 3] {
    [
        ("shield", world.shield.as_ref().map(|pool| pool.current)),
        ("armor", world.armor.as_ref().map(|pool| pool.current)),
        ("health", Some(world.health.current)),
    ]
}

pub(crate) fn flag(value: Val) -> Result<bool, Stop> {
    match value {
        Val::Bool(b) => Ok(b),
        other => unsupported(format!("condition value {other:?}")),
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
            now,
        };
        match action {
            Action::Disabled { .. } => advance(inst),
            Action::SetGlobalVariable { variable, value } => {
                let value = eval(value, &env)?;
                let old = world.globals.get(variable).cloned().unwrap_or(Val::Null);
                world.globals.insert(variable.clone(), value.clone());
                world.changes.push(Change::GlobalVariable {
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
                let operand = number(&eval(value, &env)?)?;
                let old = world.globals.get(variable).cloned().unwrap_or(Val::Null);
                let updated = match op {
                    ModifyOp::Add => number(&old)? + operand,
                    ModifyOp::Subtract => number(&old)? - operand,
                    ModifyOp::Multiply => number(&old)? * operand,
                    other => return unsupported(format!("modify operation {other:?}")),
                };
                world.globals.insert(variable.clone(), Val::Number(updated));
                world.changes.push(Change::GlobalVariable {
                    name: variable.clone(),
                    index: None,
                    old,
                    new: Val::Number(updated),
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
                world.changes.push(Change::PlayerVariable {
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
                let operand = number(&eval(value, &env)?)?;
                let Some(target) = world.players.get_mut(&id) else {
                    return unsupported(format!("player variable of undeclared player `{id}`"));
                };
                let old = target.vars.get(variable).cloned().unwrap_or(Val::Null);
                let updated = match op {
                    ModifyOp::Add => number(&old)? + operand,
                    ModifyOp::Subtract => number(&old)? - operand,
                    ModifyOp::Multiply => number(&old)? * operand,
                    other => return unsupported(format!("modify operation {other:?}")),
                };
                target.vars.insert(variable.clone(), Val::Number(updated));
                world.changes.push(Change::PlayerVariable {
                    player: id,
                    name: variable.clone(),
                    index: None,
                    old,
                    new: Val::Number(updated),
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
                world.changes.push(Change::GlobalVariable {
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
                world.changes.push(Change::PlayerVariable {
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
                        world.changes.push(Change::GlobalVariable {
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
                        world.changes.push(Change::PlayerVariable {
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
                    world.changes.push(Change::Log { text });
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
                    // A true condition continues in the same tick (P4).
                    if flag(eval(condition, &env)?)? {
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
                "damage" => {
                    let [player, damager, amount] = args.as_slice() else {
                        return unsupported("damage arguments");
                    };
                    // Arguments evaluate left to right. `Null` has no
                    // dealer scale; a player deals with its `damageDealt%`.
                    let ids = player_ids(player, &env)?;
                    let damager = eval(damager, &env)?;
                    let amount = number(&eval(amount, &env)?)?;
                    let dealt = match damager {
                        Val::Null => 1.0,
                        Val::Player(id) => world_state(world, &id)?.damage_dealt,
                        other => {
                            return unsupported(format!("{other:?} where a player is needed"));
                        }
                    };
                    for id in ids {
                        let target = world_state(world, &id)?;
                        let before = pools(&target.world);
                        state::damage(&mut target.world, amount * dealt * target.damage_received);
                        let after = pools(&target.world);
                        for ((field, old), (_, new)) in before.into_iter().zip(after) {
                            if old != new {
                                world.changes.push(Change::World {
                                    player: id.clone(),
                                    field: field.to_string(),
                                    old: old.map_or(Val::Null, Val::Number),
                                    new: new.map_or(Val::Null, Val::Number),
                                });
                            }
                        }
                    }
                    advance(inst);
                }
                "heal" => {
                    let [player, healer, amount] = args.as_slice() else {
                        return unsupported("heal arguments");
                    };
                    // Same shape as `damage`, with `healingDealt%` and
                    // `healingReceived%` scales.
                    let ids = player_ids(player, &env)?;
                    let healer = eval(healer, &env)?;
                    let amount = number(&eval(amount, &env)?)?;
                    let dealt = match healer {
                        Val::Null => 1.0,
                        Val::Player(id) => world_state(world, &id)?.healing_dealt,
                        other => {
                            return unsupported(format!("{other:?} where a player is needed"));
                        }
                    };
                    for id in ids {
                        let target = world_state(world, &id)?;
                        let old = target.world.health.current;
                        state::heal(&mut target.world, amount * dealt * target.healing_received);
                        let new = target.world.health.current;
                        if new != old {
                            world.changes.push(Change::World {
                                player: id,
                                field: "health".to_string(),
                                old: Val::Number(old),
                                new: Val::Number(new),
                            });
                        }
                    }
                    advance(inst);
                }
                "setAbilityCooldown" => {
                    let [player, button, cooldown] = args.as_slice() else {
                        return unsupported("setAbilityCooldown arguments");
                    };
                    // `Set Ability Cooldown` writes the reported cooldown for
                    // the slot literally; whether `abilityCooldown%` also
                    // scales it is what P7 is for.
                    let ids = player_ids(player, &env)?;
                    let slot = button_slot(button)?;
                    let seconds = number(&eval(cooldown, &env)?)?;
                    for id in ids {
                        let target = world_state(world, &id)?;
                        let old = target.world.cooldowns.get(slot).copied();
                        target.world.cooldowns.insert(slot.to_string(), seconds);
                        world.changes.push(Change::World {
                            player: id,
                            field: format!("cooldown.{slot}"),
                            old: old.map_or(Val::Null, Val::Number),
                            new: Val::Number(seconds),
                        });
                    }
                    advance(inst);
                }
                other => match crate::hud::action(
                    other,
                    args,
                    &env,
                    &mut world.hud,
                    tick,
                    &mut world.changes,
                ) {
                    Some(result) => {
                        result?;
                        advance(inst);
                    }
                    None => return unsupported(format!("action {other}")),
                },
            },
            other => return unsupported(format!("action {other:?}")),
        }
    }
    unsupported(format!(
        "rule `{}` ran {STEP_LIMIT} steps without a Wait",
        inst.rule.name
    ))
}
