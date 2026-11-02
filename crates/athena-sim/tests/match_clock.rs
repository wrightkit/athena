//! The match clock (athena#120): `Pause`/`Unpause Match Time`,
//! `Set Match Time` and `Set Slow Motion` write it, `Match Time` and
//! `Total Time Elapsed` read it, and every write records a
//! `matchClock` change. `Set Slow Motion` changes nothing a rule can
//! read: game time per tick and `Wait` rounding stay as at 100% (P8).

use athena_sim::{Change, Outcome, Val, run_text};

/// A global rule `clock` whose actions `body` spells, with `g`/`t0`/
/// `t1`/`m` declared for reads to land in.
fn program(body: &str) -> String {
    format!(
        r#"
variables {{
    global:
        0: g
        1: t0
        2: t1
        3: m
}}

rule ("clock") {{
    event {{
        Ongoing - Global;
    }}
    actions {{
{body}
    }}
}}
"#
    )
}

fn run(body: &str, seconds: f64) -> athena_sim::Trace {
    run_text(&program(body), athena_sim::Options { seconds })
        .unwrap_or_else(|error| panic!("{error}"))
}

/// The `matchClock` changes one timeline entry recorded.
fn clock_changes(trace: &athena_sim::Trace, tick: u64) -> Vec<Change> {
    trace
        .timeline
        .entries
        .iter()
        .find(|entry| entry.tick == tick)
        .unwrap_or_else(|| panic!("no timeline entry for tick {tick}"))
        .changes
        .iter()
        .filter(|change| matches!(change, Change::MatchClock { .. }))
        .cloned()
        .collect()
}

#[test]
fn pause_freezes_total_time_elapsed_and_records_the_write() {
    let trace = run(
        "        Pause Match Time;
        Set Global Variable(t0, Total Time Elapsed);
        Wait(0.048, Ignore Condition);
        Set Global Variable(t1, Total Time Elapsed);",
        1.0,
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    // The clock stands while paused: both reads see the run's start,
    // and the wake tick's `time` repeats the first tick's.
    assert_eq!(trace.globals["t0"], Val::number(0.015));
    assert_eq!(trace.globals["t1"], Val::number(0.015));
    let [start, wake] = trace.timeline.entries.as_slice() else {
        panic!("expected two entries, got {:?}", trace.timeline.entries);
    };
    assert_eq!((start.tick, start.time.get()), (0, 15.0));
    assert_eq!((wake.tick, wake.time.get()), (3, 15.0));
    assert_eq!(
        clock_changes(&trace, 0),
        [Change::MatchClock {
            action: "pauseMatchTime".into(),
            field: "paused".into(),
            old: Val::Bool(false),
            new: Val::Bool(true),
        }]
    );
}

#[test]
fn unpause_resumes_the_clock() {
    let trace = run(
        "        Pause Match Time;
        Wait(0.016, Ignore Condition);
        Unpause Match Time;
        Set Global Variable(t0, Total Time Elapsed);
        Wait(0.016, Ignore Condition);
        Set Global Variable(t1, Total Time Elapsed);",
        1.0,
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["t0"], Val::number(0.015));
    assert_eq!(trace.globals["t1"], Val::number(0.031));
    assert_eq!(
        clock_changes(&trace, 1),
        [Change::MatchClock {
            action: "unpauseMatchTime".into(),
            field: "paused".into(),
            old: Val::Bool(true),
            new: Val::Bool(false),
        }]
    );
}

#[test]
fn set_match_time_writes_the_countdown_match_time_reads() {
    let trace = run(
        "        Set Match Time(60);
        Set Global Variable(m, Match Time);
        Wait(0.016, Ignore Condition);
        Set Global Variable(t1, Match Time);",
        1.0,
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["m"], Val::number(60.0));
    // One unpaused tick counted the countdown down by 16 ms.
    assert_eq!(trace.globals["t1"], Val::number(59.984));
    assert_eq!(
        clock_changes(&trace, 0),
        [Change::MatchClock {
            action: "setMatchTime".into(),
            field: "matchTime".into(),
            old: Val::Null,
            new: Val::number(60.0),
        }]
    );
}

#[test]
fn a_paused_countdown_holds_its_value() {
    // Bastion's freeze: pause, then `Set Match Time(False)` — the zero
    // stays written and read; the crossing check never runs paused.
    let trace = run(
        "        Pause Match Time;
        Set Match Time(False);
        Wait(0.032, Ignore Condition);
        Set Global Variable(m, Match Time);",
        1.0,
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["m"], Val::number(0.0));
}

#[test]
fn match_time_before_any_set_is_not_modeled() {
    let trace = run("        Set Global Variable(m, Match Time);", 1.0);
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "value getMatchTime before a Set Match Time"
    );
    assert_eq!(not_modeled.tick, Some(0));
}

#[test]
fn a_countdown_reaching_zero_ends_the_run() {
    // The phase boundary a zeroed countdown crosses is not modeled.
    let trace = run(
        "        Set Match Time(0.01);
        Wait(0.016, Ignore Condition);",
        1.0,
    );
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(not_modeled.construct, "matchTime reaching zero");
    assert_eq!(not_modeled.tick, Some(1));
    assert_eq!(not_modeled.location, None);
}

#[test]
fn set_slow_motion_records_the_factor_and_leaves_game_time_unscaled() {
    // P8 (measured): under 50% and 90% the Inspector clock still reads
    // 16 ms per tick, so a rule sees no change in `Total Time Elapsed`.
    let trace = run(
        "        Set Slow Motion(50);
        Set Global Variable(t0, Total Time Elapsed);
        Wait(0.016, Ignore Condition);
        Set Global Variable(t1, Total Time Elapsed);",
        1.0,
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["t0"], Val::number(0.015));
    assert_eq!(trace.globals["t1"], Val::number(0.031));
    assert_eq!(
        clock_changes(&trace, 0),
        [Change::MatchClock {
            action: "setSlowMotion".into(),
            field: "slowMotion".into(),
            old: Val::number(100.0),
            new: Val::number(50.0),
        }]
    );
}

#[test]
fn waits_under_a_non_100_factor_round_up_to_whole_ticks() {
    // P8w (measured): at 50% a `Wait(0.1)` still takes 112 ms.
    let trace = run(
        "        Set Slow Motion(50);
        Wait(0.1, Ignore Condition);
        Set Global Variable(t0, Total Time Elapsed);",
        1.0,
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["t0"], Val::number(0.127));
}

#[test]
fn a_non_finite_clock_argument_is_not_modeled() {
    for action in ["Set Match Time", "Set Slow Motion"] {
        let trace = run(&format!("        {action}(1 / 0);"), 1.0);
        let Outcome::NotModeled(not_modeled) = &trace.outcome else {
            panic!("{action}: expected NotModeled, got {:?}", trace.outcome);
        };
        assert!(
            not_modeled.construct.contains("non-finite"),
            "{action}: {}",
            not_modeled.construct
        );
    }
}
