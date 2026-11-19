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

pub(crate) fn fire(
    instance: &mut Instance<'_>,
    rule_list: &[&Rule],
    blocks: &[Blocks],
    subroutines: &BTreeMap<String, usize>,
    world: &mut World,
    tick: u64,
    now: Finite,
) -> Result<(), Stop> {
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
