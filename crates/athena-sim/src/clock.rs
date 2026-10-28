//! The match clock: `Pause`/`Unpause Match Time`, `Set Match Time` and
//! `Set Slow Motion` write it, `Match Time` and `Total Time Elapsed`
//! read it (athena#120). An unpaused clock advances each tick by the
//! tick length scaled by the slow-motion percent: a `Pause Match Time`
//! stops `Total Time Elapsed` and the match countdown alike, and
//! `Set Slow Motion` scales how fast both run. A `Wait` maps seconds
//! to ticks the clock does not describe, so a `Wait` under a non-100
//! factor — and a `Wait Until` whose timeout would suspend — stops
//! `not_modeled` rather than guess the mapping. `Match Time` before
//! any `Set Match Time` is unanswerable — the lobby's phase limit is
//! no input the scenario format carries — and a countdown reaching
//! zero is the phase boundary the simulator does not run. Modeled,
//! not measured (`docs/findings.md`).

use workshop_rs::Value;

use crate::eval::{eval, number};
use crate::load::Instance;
use crate::step::World;
use crate::unsupported;
use crate::{Change, Finite, Stop, Val};

/// The match clock: how far the run's game time has come, whether
/// `Pause Match Time` holds it, the countdown `Match Time` reports
/// and the `Set Slow Motion` percent that scales its rate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Clock {
    /// `Pause Match Time` holds the clock; it advances only while
    /// this is false.
    pub paused: bool,
    /// The seconds `Match Time` reports; `None` until a `Set Match
    /// Time` writes it — the lobby's phase limit is not an input the
    /// scenario format carries, so the read has no start value.
    pub match_time: Option<Finite>,
    /// The `Set Slow Motion` percent; `100` is the client's normal
    /// rate.
    pub slow_motion: Finite,
    /// The game-time seconds elapsed; `Total Time Elapsed` reads the
    /// start offset plus this.
    pub elapsed: Finite,
}

impl Clock {
    /// The start-of-run clock: unpaused, at the normal rate, with no
    /// match countdown set.
    pub(crate) fn new() -> Self {
        Self {
            paused: false,
            match_time: None,
            slow_motion: Finite::from(100),
            elapsed: Finite::from(0),
        }
    }

    /// Advance the clock into a tick: an unpaused clock gains
    /// `tick_s` scaled by `slow_motion` on `elapsed` and loses the
    /// same on `match_time`. A paused clock stands. A countdown that
    /// reaches zero crosses the phase boundary the simulator does not
    /// run — game-mode advancement is not modeled — so the run stops.
    pub(crate) fn advance(&mut self, tick_s: f64) -> Result<(), Stop> {
        if self.paused {
            return Ok(());
        }
        let step = tick_s * self.slow_motion.get() / 100.0;
        self.elapsed = Finite::produced(self.elapsed.get() + step, "match clock")?;
        if let Some(remaining) = self.match_time {
            let remaining = remaining.get() - step;
            if remaining <= 0.0 {
                return unsupported("matchTime reaching zero");
            }
            self.match_time = Some(Finite::produced(remaining, "match clock")?);
        }
        Ok(())
    }
}

/// Execute a match-clock action against `world`; `None` when `name`
/// is not one of them.
pub(crate) fn action(
    name: &str,
    args: &[Value],
    world: &mut World,
    inst: &Instance<'_>,
    rule_idx: usize,
    now: Finite,
) -> Option<Result<(), Stop>> {
    if !matches!(
        name,
        "pauseMatchTime" | "unpauseMatchTime" | "setMatchTime" | "setSlowMotion"
    ) {
        return None;
    }
    Some(run(name, args, world, inst, rule_idx, now))
}

fn run(
    name: &str,
    args: &[Value],
    world: &mut World,
    inst: &Instance<'_>,
    rule_idx: usize,
    now: Finite,
) -> Result<(), Stop> {
    // The caller's `Env` cannot cross this call — it shares a borrow of
    // `world.players` — so the `Env` is built here, as in `actions.rs`.
    let env = world.env(
        inst.event.as_ref(),
        inst.each_player.as_deref(),
        &inst.once,
        rule_idx,
        now,
    );
    // Each arm evaluates its arguments left to right first, then writes
    // the clock field and records the `matchClock` change; a write
    // records even when it changes nothing — the action ran.
    match name {
        "pauseMatchTime" | "unpauseMatchTime" => {
            let [] = args else {
                return unsupported(format!("{name} arguments"));
            };
            let paused = name == "pauseMatchTime";
            let old = world.clock.paused;
            world.clock.paused = paused;
            world.changes.borrow_mut().push(Change::MatchClock {
                action: name.to_string(),
                field: "paused".to_string(),
                old: Val::Bool(old),
                new: Val::Bool(paused),
            });
        }
        "setMatchTime" => {
            let [time] = args else {
                return unsupported(format!("{name} arguments"));
            };
            let time = Finite::produced(number(&eval(time, &env)?)?, "action setMatchTime")?;
            let old = world.clock.match_time;
            world.clock.match_time = Some(time);
            world.changes.borrow_mut().push(Change::MatchClock {
                action: name.to_string(),
                field: "matchTime".to_string(),
                old: old.map_or(Val::Null, Val::Number),
                new: Val::Number(time),
            });
        }
        _ => {
            let [percent] = args else {
                return unsupported(format!("{name} arguments"));
            };
            let percent = Finite::produced(number(&eval(percent, &env)?)?, "action setSlowMotion")?;
            let old = world.clock.slow_motion;
            world.clock.slow_motion = percent;
            world.changes.borrow_mut().push(Change::MatchClock {
                action: name.to_string(),
                field: "slowMotion".to_string(),
                old: Val::Number(old),
                new: Val::Number(percent),
            });
        }
    }
    Ok(())
}
