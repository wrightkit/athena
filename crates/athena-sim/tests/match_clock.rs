//! The match clock (athena#120): `Pause`/`Unpause Match Time`,
//! `Set Match Time` and `Set Slow Motion` write it, `Match Time` and
//! `Total Time Elapsed` read it, and every write records a
//! `matchClock` change. A `Wait` — and a suspending `Wait Until` —
//! under a non-100 factor stops `not_modeled`, the seconds-to-ticks
//! mapping there being unmeasured.

use athena_sim::{Change, Outcome, RunEnd, StopIndex, Val, run_scenario, run_text};

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
    assert_eq!(trace.globals["t0"], Val::Number(0.015));
    assert_eq!(trace.globals["t1"], Val::Number(0.015));
    let [start, wake] = trace.timeline.entries.as_slice() else {
        panic!("expected two entries, got {:?}", trace.timeline.entries);
    };
    assert_eq!((start.tick, start.time), (0, 15.0));
    assert_eq!((wake.tick, wake.time), (3, 15.0));
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
    assert_eq!(trace.globals["t0"], Val::Number(0.015));
    assert_eq!(trace.globals["t1"], Val::Number(0.031));
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
    assert_eq!(trace.globals["m"], Val::Number(60.0));
    // One unpaused tick counted the countdown down by 16 ms.
    assert_eq!(trace.globals["t1"], Val::Number(59.984));
    assert_eq!(
        clock_changes(&trace, 0),
        [Change::MatchClock {
            action: "setMatchTime".into(),
            field: "matchTime".into(),
            old: Val::Null,
            new: Val::Number(60.0),
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
    assert_eq!(trace.globals["m"], Val::Number(0.0));
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
fn set_slow_motion_scales_the_clock_and_records_the_factor() {
    // `reader` suspends at the 100% rate; `slow` then sets 50%, so the
    // in-flight `Wait Until` re-check sees one 8 ms tick, not 16.
    let text = r#"
variables {
    global:
        0: hit
}

rule ("reader") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait Until(Total Time Elapsed >= 0.023, 999);
        Set Global Variable(hit, Total Time Elapsed);
    }
}

rule ("slow") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Slow Motion(50);
    }
}
"#;
    let trace = run_text(text, athena_sim::Options { seconds: 1.0 }).unwrap();
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["hit"], Val::Number(0.023));
    let [start, wake] = trace.timeline.entries.as_slice() else {
        panic!("expected two entries, got {:?}", trace.timeline.entries);
    };
    assert_eq!((start.tick, start.time), (0, 15.0));
    assert_eq!((wake.tick, wake.time), (1, 23.0));
    assert_eq!(wake.rules[0].ended, RunEnd::Finished);
    assert_eq!(
        clock_changes(&trace, 0),
        [Change::MatchClock {
            action: "setSlowMotion".into(),
            field: "slowMotion".into(),
            old: Val::Number(100.0),
            new: Val::Number(50.0),
        }]
    );
}

#[test]
fn a_wait_under_a_non_100_factor_is_not_modeled() {
    let trace = run(
        "        Set Slow Motion(90);
        Wait(1, Ignore Condition);",
        1.0,
    );
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "action wait under a 90% simulation rate"
    );
    assert_eq!(not_modeled.tick, Some(0));
    let Some(location) = &not_modeled.location else {
        panic!("expected a stop location");
    };
    assert_eq!(location.at, Some(StopIndex::Action(1)));
}

#[test]
fn a_suspending_wait_until_under_a_non_100_factor_is_not_modeled() {
    let trace = run(
        "        Set Slow Motion(50);
        Wait Until(False, 1);",
        1.0,
    );
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "action waitUntil under a 50% simulation rate"
    );
}

#[test]
fn a_true_wait_until_under_a_non_100_factor_still_passes() {
    // The gate covers the suspend only — a condition already true
    // continues in the same tick, no timing is guessed.
    let trace = run(
        "        Set Slow Motion(50);
        Wait Until(True, 1);
        Set Global Variable(g, 1);",
        1.0,
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["g"], Val::Number(1.0));
}

#[test]
fn a_wait_suspended_before_the_factor_change_stops_at_its_wake() {
    // `A` suspends at the 100% rate; `slow` then sets 50%, so the wake
    // tick encodes the same unmeasured seconds-to-ticks mapping — the
    // run stops there rather than resume on it.
    let text = r#"
variables {
    global:
        0: g
}

rule ("A") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait(0.048, Ignore Condition);
        Set Global Variable(g, Total Time Elapsed);
    }
}

rule ("slow") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Slow Motion(50);
    }
}
"#;
    let trace = run_text(text, athena_sim::Options { seconds: 1.0 }).unwrap();
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "action wait under a 50% simulation rate"
    );
    assert_eq!(not_modeled.tick, Some(3));
    let Some(location) = &not_modeled.location else {
        panic!("expected a stop location");
    };
    assert_eq!(
        (location.rule, location.at),
        (0, Some(StopIndex::Action(0)))
    );
}

#[test]
fn a_wait_until_timeout_expiring_under_a_non_100_factor_is_not_modeled() {
    // The suspended `Wait Until`'s timeout is the same stale schedule:
    // the condition stays false and the expiry fires under 50%.
    let text = r#"
rule ("u") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait Until(False, 0.048);
    }
}

rule ("slow") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Slow Motion(50);
    }
}
"#;
    let trace = run_text(text, athena_sim::Options { seconds: 1.0 }).unwrap();
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "action waitUntil under a 50% simulation rate"
    );
    assert_eq!(not_modeled.tick, Some(3));
    let Some(location) = &not_modeled.location else {
        panic!("expected a stop location");
    };
    assert_eq!(
        (location.rule, location.at),
        (0, Some(StopIndex::Action(0)))
    );
}

#[test]
fn a_wait_until_condition_wake_under_a_non_100_factor_resumes() {
    // The one bypass of the stale-wake gate: a `Wait Until` wake its
    // own condition re-check produced this tick encodes no schedule,
    // so it resumes even under 50%. `waiter` suspends at 100%, `slow`
    // sets 50% at tick 1, `setter` writes `g` at tick 3 — the next
    // re-check wakes the run and it completes.
    let text = r#"
variables {
    global:
        0: g
        1: h
}

rule ("waiter") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait Until(g == 1, 999);
        Set Global Variable(h, 7);
    }
}

rule ("slow") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait(0.016, Ignore Condition);
        Set Slow Motion(50);
    }
}

rule ("setter") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Set Global Variable(g, 1);
    }
}
"#;
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "team": "team1" }],
        "events": [{ "kind": "playerDied", "time": 0.048, "eventPlayer": "p1" }]
    }"#;
    let trace = run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["h"], Val::Number(7.0));
}

#[test]
fn a_subroutine_wait_until_condition_wake_under_a_non_100_factor_resumes() {
    // The same carve-out on a `Start Rule`'d subroutine instance:
    // `sub` suspends on the `Wait Until` at 100% at tick 0, `slow`
    // sets 50% the same tick, and the re-check wake after `setter`
    // writes `g` resumes through to `h`.
    let text = r#"
variables {
    global:
        0: g
        1: h
}

subroutines {
    0: sub
}

rule ("start") {
    event {
        Ongoing - Global;
    }
    actions {
        Start Rule(sub, Do Nothing);
    }
}

rule ("Subroutine sub") {
    event {
        Subroutine;
        sub;
    }
    actions {
        Wait Until(g == 1, 999);
        Set Global Variable(h, 9);
    }
}

rule ("slow") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Slow Motion(50);
    }
}

rule ("setter") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Set Global Variable(g, 1);
    }
}
"#;
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "team": "team1" }],
        "events": [{ "kind": "playerDied", "time": 0.048, "eventPlayer": "p1" }]
    }"#;
    let trace = run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["h"], Val::Number(9.0));
}

#[test]
fn an_event_rule_wait_suspended_before_the_factor_change_stops() {
    // The event-rule resume path gates the same way: the `Player Died`
    // run suspends at 100%, `slow` sets 50%, the wake tick is stale.
    let text = r#"
rule ("died") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Wait(0.032, Ignore Condition);
    }
}

rule ("slow") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Slow Motion(50);
    }
}
"#;
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "team": "team1" }],
        "events": [{ "kind": "playerDied", "time": 0, "eventPlayer": "p1" }]
    }"#;
    let trace = run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "action wait under a 50% simulation rate"
    );
    assert_eq!(not_modeled.tick, Some(2));
    let Some(location) = &not_modeled.location else {
        panic!("expected a stop location");
    };
    assert_eq!(
        (location.rule, location.at, location.event_player.as_deref()),
        (0, Some(StopIndex::Action(0)), Some("p1"))
    );
}

#[test]
fn a_subroutine_wait_suspended_before_the_factor_change_stops() {
    // `Start Rule` puts the subroutine's run on its own instance; a
    // `Wait` it suspends on gates the same at its wake.
    let text = r#"
variables {
    global:
        0: g
}

subroutines {
    0: sub
}

rule ("start") {
    event {
        Ongoing - Global;
    }
    actions {
        Start Rule(sub, Do Nothing);
    }
}

rule ("Subroutine sub") {
    event {
        Subroutine;
        sub;
    }
    actions {
        Wait(0.032, Ignore Condition);
        Set Global Variable(g, 1);
    }
}

rule ("slow") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Slow Motion(50);
    }
}
"#;
    let trace = run_text(text, athena_sim::Options { seconds: 1.0 }).unwrap();
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "action wait under a 50% simulation rate"
    );
    assert_eq!(not_modeled.tick, Some(2));
    let Some(location) = &not_modeled.location else {
        panic!("expected a stop location");
    };
    assert_eq!(
        (location.rule, location.at),
        (1, Some(StopIndex::Action(0)))
    );
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
