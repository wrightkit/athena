use std::collections::BTreeMap;

use workshop_rs::hero_values::HeroValues;
use workshop_rs::{Action, Event, EventTarget, EventTeam, ModifyOp, Program, Rule, Value};

use crate::blocks::Blocks;
use crate::eval::{Env, button_slot, eval, number, player_id, player_ids};
use crate::hud::Hud;
use crate::load::{Frame, Instance, Loaded, WaitMode, load, player_matches};
use crate::scenario::{EventCtx, Expectation, Player, Scenario};
use crate::state::{self, WorldSlot};
use crate::{
    ExpectationResult, ExpectationStatus, LogEntry, MissingInput, MissingInputKind, NotModeled,
    Options, Outcome, PlayerState, Stop, Trace, Val,
};

/// One server tick (P0, P1).
const TICK_S: f64 = 0.016;
/// `Total Time Elapsed` on the first tick. After `Wait(3)` (188 ticks) every
/// capture reads t0 = 3.023.
const START_OFFSET_S: f64 = 0.015;
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

struct World {
    globals: BTreeMap<String, Val>,
    players: BTreeMap<String, Player>,
    logs: Vec<LogEntry>,
    hud: Hud,
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

fn conditions_hold(rule: &Rule, env: &Env) -> Result<bool, Stop> {
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

/// The condition of the `Wait Until` action the frame points at.
fn until_holds(rules: &[&Rule], frame: Frame, env: &Env) -> Result<bool, Stop> {
    let Some(Action::Call { args, .. }) = rules[frame.rule_idx].actions.get(frame.pc) else {
        unreachable!("a Wait Until frame points at its action");
    };
    flag(eval(&args[0], env)?)
}

fn flag(value: Val) -> Result<bool, Stop> {
    match value {
        Val::Bool(b) => Ok(b),
        other => unsupported(format!("condition value {other:?}")),
    }
}

/// The step at which `pc` moves past the frame's last action.
fn advance(inst: &mut Instance<'_>) {
    inst.frames.last_mut().expect("a running frame").pc += 1;
}

fn run_instance(
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
                world.globals.insert(variable.clone(), value);
                advance(inst);
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
                target.vars.insert(variable.clone(), value);
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
                let current = number(target.vars.get(variable).unwrap_or(&Val::Null))?;
                let updated = match op {
                    ModifyOp::Add => current + operand,
                    ModifyOp::Subtract => current - operand,
                    ModifyOp::Multiply => current * operand,
                    other => return unsupported(format!("modify operation {other:?}")),
                };
                target.vars.insert(variable.clone(), Val::Number(updated));
                advance(inst);
            }
            Action::If { condition } => {
                if flag(eval(condition, &env)?)? {
                    advance(inst);
                } else {
                    inst.clause_jump = true;
                    inst.frames.last_mut().expect("a running frame").pc =
                        map.next_clause[pc].expect("paired at load");
                }
            }
            Action::ElseIf { condition } => {
                if !inst.clause_jump {
                    inst.frames.last_mut().expect("a running frame").pc =
                        map.chain_end[pc].expect("paired at load") + 1;
                } else {
                    inst.clause_jump = false;
                    if flag(eval(condition, &env)?)? {
                        advance(inst);
                    } else {
                        inst.clause_jump = true;
                        inst.frames.last_mut().expect("a running frame").pc =
                            map.next_clause[pc].expect("paired at load");
                    }
                }
            }
            Action::Else => {
                inst.frames.last_mut().expect("a running frame").pc = if inst.clause_jump {
                    inst.clause_jump = false;
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
            Action::End => {
                let open = map.pairs[pc].expect("paired at load");
                inst.frames.last_mut().expect("a running frame").pc =
                    if matches!(rule.actions[open], Action::While { .. }) {
                        open
                    } else {
                        pc + 1
                    };
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
                    world.logs.push(LogEntry { tick, text });
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
                        state::damage(&mut target.world, amount * dealt * target.damage_received);
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
                        state::heal(&mut target.world, amount * dealt * target.healing_received);
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
                        target.world.cooldowns.insert(slot.to_string(), seconds);
                    }
                    advance(inst);
                }
                other => match crate::hud::action(other, args, &env, &mut world.hud, tick) {
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

/// Group injected events by the tick they fire on: `time` counts seconds from
/// the first tick, so the tick is `floor(time / 16 ms)`. Events at the same
/// tick keep file order.
fn events_by_tick(events: &[crate::scenario::InjectedEvent]) -> BTreeMap<u64, Vec<EventCtx>> {
    let mut by_tick: BTreeMap<u64, Vec<EventCtx>> = BTreeMap::new();
    for event in events {
        let tick = (event.time / TICK_S).floor() as u64;
        by_tick.entry(tick).or_default().push(EventCtx {
            kind: event.kind.clone(),
            player: event.event_player.clone(),
            values: event.values.clone(),
        });
    }
    by_tick
}

pub(crate) fn run(
    program: &Program,
    options: Options,
    scenario: Option<Scenario>,
    hero_values: &HeroValues,
) -> Trace {
    let (mut players, events, expect) = scenario
        .map(|scenario| (scenario.players, scenario.events, scenario.expect))
        .unwrap_or_default();
    // Resolve every player's world state before the first tick. A failure is
    // stored, not reported yet: the trace row still says what the scenario
    // declared, and the run stops only if a `Damage`/`Heal`/`Set Ability
    // Cooldown` action or a world-state read touches the player.
    for (id, player) in &mut players {
        player.world = match &player.hero {
            Some(hero) => match state::resolve(
                id,
                hero,
                &player.hero_spellings,
                player.team,
                program.settings.as_ref(),
                hero_values,
            ) {
                Ok(resolved) => WorldSlot::Resolved(resolved),
                Err(error) => WorldSlot::Failed(error),
            },
            None => WorldSlot::Failed(NotModeled::new(format!("player `{id}` has no hero"))),
        };
    }
    let mut world = World {
        globals: program
            .global_variables
            .iter()
            .map(|variable| (variable.name.clone(), Val::Null))
            .collect(),
        players,
        logs: Vec::new(),
        hud: Hud::default(),
    };
    let events = events_by_tick(&events);
    let mut missing_inputs = Vec::new();
    let outcome = match load(program, &world.players)
        .and_then(|loaded| tick_loop(loaded, &mut world, options, &events))
    {
        Ok(()) => Outcome::Complete,
        Err(Stop::NotModeled(not_modeled)) => Outcome::NotModeled(not_modeled),
        Err(Stop::MissingInput(missing)) => {
            missing_inputs.push(missing);
            Outcome::MissingInput
        }
    };
    Trace {
        expectations: expectations(&expect, &world, &outcome),
        outcome,
        logs: world.logs,
        globals: world.globals,
        players: world
            .players
            .iter()
            .map(|(id, player)| {
                let state = PlayerState {
                    hero: player.hero.clone(),
                    team: player.team.map(|team| team.as_str().to_string()),
                    world: match &player.world {
                        WorldSlot::Resolved(state) => Some(state.world.clone()),
                        WorldSlot::Unresolved | WorldSlot::Failed(_) => None,
                    },
                };
                (id.clone(), state)
            })
            .collect(),
        hud: world.hud.elements,
        missing_inputs,
    }
}

/// Expectations are checked at the end of a complete run only; a partial run
/// reports `not_evaluated` for every one.
fn expectations(
    expect: &[Expectation],
    world: &World,
    outcome: &Outcome,
) -> Vec<ExpectationResult> {
    let complete = matches!(outcome, Outcome::Complete);
    expect
        .iter()
        .enumerate()
        .map(|(index, expectation)| {
            if !complete {
                return ExpectationResult {
                    index,
                    status: ExpectationStatus::NotEvaluated,
                    actual: Val::Null,
                };
            }
            let (actual, pass) = match expectation {
                Expectation::Global { name, value } => {
                    let actual = world.globals.get(name).cloned().unwrap_or(Val::Null);
                    (actual.clone(), actual == *value)
                }
                Expectation::PlayerVariable {
                    player,
                    name,
                    value,
                } => {
                    let actual = world
                        .players
                        .get(player)
                        .and_then(|player| player.vars.get(name))
                        .cloned()
                        .unwrap_or(Val::Null);
                    (actual.clone(), actual == *value)
                }
                Expectation::Log { tick, text } => {
                    let pass = world
                        .logs
                        .iter()
                        .any(|entry| entry.tick == *tick && entry.text == *text);
                    let actual = world
                        .logs
                        .iter()
                        .find(|entry| entry.tick == *tick)
                        .map(|entry| Val::Text(entry.text.clone()))
                        .unwrap_or(Val::Null);
                    (actual, pass)
                }
            };
            ExpectationResult {
                index,
                status: if pass {
                    ExpectationStatus::Pass
                } else {
                    ExpectationStatus::Fail
                },
                actual,
            }
        })
        .collect()
}

fn tick_loop(
    mut loaded: Loaded<'_>,
    world: &mut World,
    options: Options,
    events: &BTreeMap<u64, Vec<EventCtx>>,
) -> Result<(), Stop> {
    let ticks = (options.seconds / TICK_S).floor() as u64 + 1;
    for tick in 0..ticks {
        let now = START_OFFSET_S + tick as f64 * TICK_S;
        if let Some(due) = events.get(&tick) {
            for injected in due {
                for instance in &mut loaded.instances {
                    if let Event::Player { kind, team, target } = &instance.rule.event
                        && kind.catalog_id() == injected.kind
                    {
                        // The team/player filters read the event player: an
                        // event that does not name one can only satisfy
                        // `All`.
                        let eligible = match &injected.player {
                            Some(id) => player_matches(
                                instance.rule,
                                *team,
                                target,
                                world
                                    .players
                                    .get(id)
                                    .expect("the scenario loader checks `eventPlayer` is declared"),
                            )
                            .map_err(|stop| stop.at(tick))?,
                            None if *team == EventTeam::All && *target == EventTarget::All => true,
                            None => {
                                return Err(Stop::MissingInput(
                                    MissingInput::new(
                                        MissingInputKind::EventPlayer,
                                        None,
                                        None,
                                        format!(
                                            "rule `{}`: event team or player filter",
                                            instance.rule.name
                                        ),
                                    )
                                    .at(tick),
                                ));
                            }
                        };
                        if eligible {
                            instance.pending.push_back(injected.clone());
                        }
                    }
                }
            }
        }
        for instance in &mut loaded.instances {
            match &instance.rule.event {
                Event::Global | Event::EachPlayer | Event::EachPlayerWithFilters { .. } => {
                    // The condition is tracked every tick, also during a Wait:
                    // an edge while a run is suspended is dropped, not queued
                    // (P2). An `Ongoing - Each Player` instance runs this same
                    // ongoing logic with `Event Player` bound to its player.
                    let each_player = instance.each_player.clone();
                    let env = Env {
                        globals: &world.globals,
                        players: &world.players,
                        event: None,
                        each_player: each_player.as_deref(),
                        last_text_id: world.hud.last_text_id,
                        now,
                    };
                    let holds = conditions_hold(instance.rule, &env).map_err(|e| e.at(tick))?;
                    let edge = holds && !instance.prev;
                    instance.prev = holds;
                    // A wake due this tick wins over the wait behavior.
                    if instance.wake.is_some_and(|wake| wake > tick) {
                        match instance.wait_mode {
                            WaitMode::Ignore => {}
                            WaitMode::AbortWhenFalse => {
                                if !holds {
                                    instance.frames.clear();
                                    instance.wake = None;
                                    continue;
                                }
                            }
                            WaitMode::RestartWhenTrue => {
                                if edge {
                                    instance.frames.clear();
                                    instance.frames.push(Frame {
                                        rule_idx: instance.rule_idx,
                                        pc: 0,
                                    });
                                    instance.wake = None;
                                }
                            }
                            WaitMode::Until(frame) => {
                                if until_holds(&loaded.rules, frame, &env)
                                    .map_err(|e| e.at(tick))?
                                {
                                    instance.wake = Some(tick);
                                }
                            }
                        }
                    }
                    let start = match instance.wake {
                        Some(wake) if wake <= tick => true,
                        Some(_) => false,
                        None => {
                            if edge && instance.frames.is_empty() {
                                instance.frames.push(Frame {
                                    rule_idx: instance.rule_idx,
                                    pc: 0,
                                });
                            }
                            !instance.frames.is_empty()
                        }
                    };
                    if !start {
                        continue;
                    }
                    instance.wake = None;
                    run_instance(
                        instance,
                        &loaded.rules,
                        &loaded.blocks,
                        &loaded.subroutines,
                        world,
                        tick,
                        now,
                    )
                    .map_err(|e| e.at(tick))?;
                }
                Event::Player { .. } => {
                    if instance.wake.is_some_and(|wake| wake <= tick) {
                        instance.wake = None;
                        run_instance(
                            instance,
                            &loaded.rules,
                            &loaded.blocks,
                            &loaded.subroutines,
                            world,
                            tick,
                            now,
                        )
                        .map_err(|e| e.at(tick))?;
                    }
                    while instance.wake.is_none() && instance.frames.is_empty() {
                        let Some(ctx) = instance.pending.pop_front() else {
                            break;
                        };
                        instance.event = Some(ctx);
                        let env = Env {
                            globals: &world.globals,
                            players: &world.players,
                            event: instance.event.as_ref(),
                            each_player: None,
                            last_text_id: world.hud.last_text_id,
                            now,
                        };
                        if conditions_hold(instance.rule, &env).map_err(|e| e.at(tick))? {
                            instance.frames.push(Frame {
                                rule_idx: instance.rule_idx,
                                pc: 0,
                            });
                            run_instance(
                                instance,
                                &loaded.rules,
                                &loaded.blocks,
                                &loaded.subroutines,
                                world,
                                tick,
                                now,
                            )
                            .map_err(|e| e.at(tick))?;
                        } else {
                            instance.event = None;
                        }
                    }
                }
                Event::Subroutine(_) => {}
            }
        }
    }
    Ok(())
}
