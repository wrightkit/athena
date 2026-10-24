use std::collections::{BTreeMap, VecDeque};

use workshop_rs::hero_values::HeroValues;
use workshop_rs::{Action, Event, EventTarget, EventTeam, ModifyOp, Program, Rule, Value};

use crate::eval::{Env, button_slot, eval, number, player_id, player_ids};
use crate::hud::Hud;
use crate::scenario::{EventCtx, Expectation, Player, Scenario};
use crate::state::{self, WorldSlot};
use crate::{
    ExpectationResult, ExpectationStatus, LogEntry, NotModeled, Options, Outcome, PlayerState,
    Stop, Trace, Val,
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

/// Jump targets for the block actions of one rule.
struct Blocks {
    /// `While` → its `End`; `End` → its opener (`While` or `If`).
    pairs: Vec<Option<usize>>,
    /// `If`/`ElseIf` → the next clause head or `End`; `Else` → the `End`.
    next_clause: Vec<Option<usize>>,
    /// `ElseIf`/`Else` → the `End` of the chain, where fall-through exits.
    chain_end: Vec<Option<usize>>,
}

/// The position of a rule execution in progress: the rule whose actions the
/// program counter reads. A `Call Subroutine` action pushes a frame.
#[derive(Clone, Copy)]
struct Frame {
    rule_idx: usize,
    pc: usize,
}

struct Instance<'a> {
    rule: &'a Rule,
    rule_idx: usize,
    frames: Vec<Frame>,
    wake: Option<u64>,
    prev: bool,
    /// Injected events that triggered this rule and wait for it to be idle.
    pending: VecDeque<EventCtx>,
    /// The injected event the current run executes for, if event-triggered.
    event: Option<EventCtx>,
    /// Set when control jumped to a clause head on a false condition, so the
    /// head evaluates rather than exits the chain.
    clause_jump: bool,
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

/// A block that stays open until its `End` line.
enum Open {
    While(usize),
    If {
        clauses: Vec<usize>,
        else_seen: bool,
    },
}

fn block_maps(actions: &[Action]) -> Result<Blocks, Stop> {
    let mut pairs = vec![None; actions.len()];
    let mut next_clause = vec![None; actions.len()];
    let mut chain_end = vec![None; actions.len()];
    let mut stack: Vec<Open> = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        match action {
            Action::While { .. } => stack.push(Open::While(index)),
            Action::If { .. } => stack.push(Open::If {
                clauses: vec![index],
                else_seen: false,
            }),
            Action::ElseIf { .. } | Action::Else => match stack.last_mut() {
                Some(Open::If { clauses, else_seen }) if !*else_seen => {
                    next_clause[*clauses.last().expect("an If opens its chain")] = Some(index);
                    clauses.push(index);
                    if matches!(action, Action::Else) {
                        *else_seen = true;
                    }
                }
                Some(Open::If { .. }) => return unsupported("a clause action after the `Else`"),
                _ => return unsupported("a clause action without an `If`"),
            },
            Action::End => match stack.pop() {
                Some(Open::While(open)) => {
                    pairs[open] = Some(index);
                    pairs[index] = Some(open);
                }
                Some(Open::If { clauses, .. }) => {
                    let open = *clauses.first().expect("an If opens its chain");
                    next_clause[*clauses.last().expect("an If opens its chain")] = Some(index);
                    for clause in clauses {
                        chain_end[clause] = Some(index);
                    }
                    pairs[index] = Some(open);
                }
                None => return unsupported("End without an opening action"),
            },
            _ => {}
        }
    }
    match stack.last() {
        Some(Open::While(_)) => unsupported("`While` without an `End`"),
        Some(Open::If { .. }) => unsupported("`If` without an `End`"),
        None => Ok(Blocks {
            pairs,
            next_clause,
            chain_end,
        }),
    }
}

struct Loaded<'a> {
    rules: Vec<&'a Rule>,
    blocks: Vec<Blocks>,
    /// Subroutine name → rule index.
    subroutines: BTreeMap<String, usize>,
    instances: Vec<Instance<'a>>,
}

fn load(program: &Program) -> Result<Loaded<'_>, Stop> {
    let mut rules = Vec::new();
    let mut blocks = Vec::new();
    let mut subroutines = BTreeMap::new();
    let mut instances = Vec::new();
    for (rule_idx, rule) in program
        .rules
        .iter()
        .filter(|rule| !rule.disabled)
        .enumerate()
    {
        match &rule.event {
            Event::Global | Event::Subroutine(_) => {}
            Event::Player {
                team: EventTeam::All,
                target: EventTarget::All,
                ..
            } => {}
            Event::Player { .. } => {
                return unsupported(format!("rule `{}`: event team or player filter", rule.name));
            }
            other => {
                return unsupported(format!("rule `{}`: event {other:?}", rule.name));
            }
        }
        if let Event::Subroutine(name) = &rule.event {
            subroutines.insert(name.clone(), rule_idx);
        }
        rules.push(rule);
        blocks.push(block_maps(&rule.actions)?);
        instances.push(Instance {
            rule,
            rule_idx,
            frames: Vec::new(),
            wake: None,
            prev: false,
            pending: VecDeque::new(),
            event: None,
            clause_jump: false,
        });
    }
    Ok(Loaded {
        rules,
        blocks,
        subroutines,
        instances,
    })
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
                    match behavior {
                        Value::Enum { value, .. } if value == "IGNORE_CONDITION" => {}
                        other => return unsupported(format!("wait behavior {other:?}")),
                    }
                    let ticks = wait_ticks(number(&eval(seconds, &env)?)?);
                    inst.wake = Some(tick + ticks);
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
    let outcome =
        match load(program).and_then(|loaded| tick_loop(loaded, &mut world, options, &events)) {
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
                    if let Event::Player { kind, .. } = instance.rule.event
                        && kind.catalog_id() == injected.kind
                    {
                        instance.pending.push_back(injected.clone());
                    }
                }
            }
        }
        for instance in &mut loaded.instances {
            match &instance.rule.event {
                Event::Global => {
                    // The condition is tracked every tick, also during a Wait:
                    // an edge while a run is suspended is dropped, not queued
                    // (P2).
                    let holds = conditions_hold(
                        instance.rule,
                        &Env {
                            globals: &world.globals,
                            players: &world.players,
                            event: None,
                            last_text_id: world.hud.last_text_id,
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
                Event::Subroutine(_) | Event::EachPlayer | Event::EachPlayerWithFilters { .. } => {}
            }
        }
    }
    Ok(())
}
