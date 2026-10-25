use std::collections::BTreeMap;

use workshop_rs::hero_values::HeroValues;
use workshop_rs::{Action, Event, EventTarget, EventTeam, Program, Rule};

use crate::eval::{Env, eval};
use crate::hud::Hud;
use crate::load::{Frame, Loaded, WaitMode, load, player_matches};
use crate::scenario::{EventCtx, Expectation, Scenario};
use crate::state::{self, WorldSlot};
use crate::step::{World, flag, run_instance};
use crate::{
    ExpectationResult, ExpectationStatus, MissingInput, MissingInputKind, NotModeled, Options,
    Outcome, PlayerState, Stop, Trace, Val,
};

/// One server tick (P0, P1).
const TICK_S: f64 = 0.016;
/// `Total Time Elapsed` on the first tick. After `Wait(3)` (188 ticks) every
/// capture reads t0 = 3.023.
const START_OFFSET_S: f64 = 0.015;

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
