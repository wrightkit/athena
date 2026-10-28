//! Player-event dispatch: the injected events a scenario schedules
//! and the events a modeled state change (`Damage`, `Heal`) raises.
//! Both reach an `Event::Player` rule only when the kind matches and
//! the event player satisfies the rule's team/player filters; an
//! instance that is not free queues the event and fires it when its
//! run is idle.

use std::cell::RefCell;
use std::collections::BTreeMap;

use workshop_rs::{Event, EventTarget, EventTeam, Rule};

use crate::blocks::Blocks;
use crate::eval::Env;
use crate::load::{Frame, Instance, player_matches};
use crate::scenario::EventCtx;
use crate::sched::{event_player, record_stop, run_and_record};
use crate::step::{World, conditions_hold};
use crate::{Finite, MissingInput, MissingInputKind, RuleRun, RunEnd, Stop};

/// Fire the events `world.pending_events` holds — raised by a modeled
/// state change like `Damage`/`Heal` — in the order they occurred: each
/// event runs the `Event::Player` rules whose kind and team/player
/// filters match, in rule-file order. An instance that is not free —
/// suspended on `Wait`, mid-run, or still holding earlier events —
/// queues the event instead; its own visit's pending loop fires it
/// when the run is idle, like an injected event's.
pub(crate) fn dispatch(
    instances: &mut [Instance<'_>],
    rule_list: &[&Rule],
    blocks: &[Blocks],
    subroutines: &BTreeMap<String, usize>,
    world: &mut World,
    tick: u64,
    now: Finite,
) -> Result<(), Stop> {
    for ctx in std::mem::take(&mut world.pending_events) {
        for instance in instances.iter_mut() {
            if !eligible(instance, world, &ctx, tick)?.unwrap_or(false) {
                continue;
            }
            if instance.wake.is_none() && instance.frames.is_empty() && instance.pending.is_empty()
            {
                instance.event = Some(ctx.clone());
                fire(instance, rule_list, blocks, subroutines, world, tick, now)?;
            } else {
                instance.pending.push_back(ctx.clone());
            }
        }
    }
    Ok(())
}

/// Queue `ctx` on every `Event::Player` instance whose event kind is
/// `ctx.kind` and whose team/player filters the event player satisfies:
/// an injected event's dispatch.
pub(crate) fn queue(
    instances: &mut [Instance<'_>],
    world: &mut World,
    ctx: &EventCtx,
    tick: u64,
) -> Result<(), Stop> {
    for instance in instances.iter_mut() {
        if eligible(instance, world, ctx, tick)?.unwrap_or(false) {
            instance.pending.push_back(ctx.clone());
        }
    }
    Ok(())
}

/// Check the conditions of `instance`'s `Event::Player` rule against
/// the event bound on it, and run when they hold: the sequence an
/// event-triggered run follows, shared by the injected events' pending
/// loop and the immediate dispatch of the events a modeled state
/// change raises.
pub(crate) fn fire(
    instance: &mut Instance<'_>,
    rule_list: &[&Rule],
    blocks: &[Blocks],
    subroutines: &BTreeMap<String, usize>,
    world: &mut World,
    tick: u64,
    now: Finite,
) -> Result<(), Stop> {
    // Each event's condition check is its own `Evaluate Once` pass.
    let pass = RefCell::new(BTreeMap::new());
    let env = Env {
        globals: &world.globals,
        players: &world.players,
        event: instance.event.as_ref(),
        each_player: None,
        hud: &world.hud,
        element: None,
        once: &pass,
        effects: &world.effects,
        random: world.random.as_ref(),
        server_load: world.server_load,
        map: world.map.as_deref(),
        match_phase: world.match_phase,
        game_mode: world.game_mode.as_deref(),
        objectives: world.objectives.as_deref(),
        rule: instance.rule_idx,
        changes: &world.changes,
        now,
        match_time: world.clock.match_time,
    };
    if conditions_hold(instance.rule, &env).map_err(|stop| {
        record_stop(
            &mut world.runs,
            RuleRun {
                rule: instance.rule_idx,
                name: instance.rule.name.clone(),
                event_player: event_player(instance),
                ended: RunEnd::Stopped,
            },
            None,
            stop.at(tick),
        )
    })? {
        instance.once.get_mut().clear();
        instance.frames.push(Frame {
            rule_idx: instance.rule_idx,
            pc: 0,
        });
        run_and_record(instance, rule_list, blocks, subroutines, world, tick, now)
            .map_err(|e| e.at(tick))?;
    } else {
        instance.event = None;
    }
    Ok(())
}

/// Whether `instance`'s `Event::Player` rule fires for `ctx`: `None`
/// when its event kind does not match, otherwise the team/player
/// filters' verdict on the event player — `missing_input` when a
/// filtered rule cannot read one.
fn eligible(
    instance: &Instance<'_>,
    world: &mut World,
    ctx: &EventCtx,
    tick: u64,
) -> Result<Option<bool>, Stop> {
    let Event::Player { kind, team, target } = &instance.rule.event else {
        return Ok(None);
    };
    if kind.catalog_id() != ctx.kind {
        return Ok(None);
    }
    // The team/player filters read the event player: an event that
    // does not name one can only satisfy `All`.
    let eligible = match &ctx.player {
        Some(id) => player_matches(
            *team,
            target,
            world
                .players
                .get(id)
                .expect("an event player is a declared scenario player"),
        )
        .map_err(|stop| {
            record_stop(
                &mut world.runs,
                RuleRun {
                    rule: instance.rule_idx,
                    name: instance.rule.name.clone(),
                    event_player: ctx.player.clone(),
                    ended: RunEnd::Stopped,
                },
                None,
                stop.at(tick),
            )
        })?,
        None if *team == EventTeam::All && *target == EventTarget::All => true,
        None => {
            return Err(record_stop(
                &mut world.runs,
                RuleRun {
                    rule: instance.rule_idx,
                    name: instance.rule.name.clone(),
                    event_player: None,
                    ended: RunEnd::Stopped,
                },
                None,
                MissingInput::new(
                    MissingInputKind::EventPlayer,
                    None,
                    None,
                    format!("rule `{}`: event team or player filter", instance.rule.name),
                )
                .at(tick)
                .into(),
            ));
        }
    };
    Ok(Some(eligible))
}
