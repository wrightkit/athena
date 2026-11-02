//! The match clock: `Pause`/`Unpause Match Time`, `Set Match Time` and
//! `Set Slow Motion` write it, `Match Time` and `Total Time Elapsed`
//! read it (athena#120). An unpaused clock advances each tick by the
//! tick length: a `Pause Match Time` stops `Total Time Elapsed` and
//! the match countdown alike. `Set Slow Motion` changes no clock a
//! rule can read: game time per tick and the seconds-to-ticks mapping
//! of a `Wait` stay as they are (P8, measured at 50% and 90%); the
//! write is recorded only. `Match Time` before any `Set Match Time` is
//! unanswerable — the lobby's phase limit is no input the scenario
//! format carries — and a countdown reaching zero is the phase
//! boundary the simulator does not run. The countdown and pause
//! are modeled, not measured (`docs/findings.md`).

use workshop_rs::Value;

use crate::eval::{eval, number};
use crate::load::Instance;
use crate::step::World;
use crate::unsupported;
use crate::{Change, Finite, Stop, Val};

/// The match clock: how far the run's game time has come, whether
/// `Pause Match Time` holds it, the countdown `Match Time` reports
/// and the last `Set Slow Motion` percent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Clock {
    /// `Pause Match Time` holds the clock; it advances only while
    /// this is false.
    pub paused: bool,
    /// The seconds `Match Time` reports; `None` until a `Set Match
    /// Time` writes it — the lobby's phase limit is not an input the
    /// scenario format carries, so the read has no start value.
    pub match_time: Option<Finite>,
    /// The last `Set Slow Motion` percent; `100` is the client's
    /// normal rate. Recorded only: no clock a rule can read moves (P8).
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
    /// `tick_s` on `elapsed` and loses the same on `match_time`. A
    /// paused clock stands. A countdown that reaches zero crosses the phase boundary the simulator does not
    /// run — game-mode advancement is not modeled — so the run stops.
    pub(crate) fn advance(&mut self, tick_s: f64) -> Result<(), Stop> {
        if self.paused {
            return Ok(());
        }
        self.elapsed = Finite::produced(self.elapsed.get() + tick_s, "match clock")?;
        if let Some(remaining) = self.match_time {
            let remaining = remaining.get() - tick_s;
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
