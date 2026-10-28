//! `For`/`While`/`If` block pairing and `For` loop execution (athena#50).

use athena_sim::{ExpectationStatus, Options, Outcome, Val, run_scenario, run_text};

fn run(text: &str) -> athena_sim::Trace {
    run_text(text, Options { seconds: 2.0 }).unwrap_or_else(|error| panic!("{error}"))
}

/// `For Global Variable` with an `If` in its body.
const FOR_IF: &str = r#"
variables {
    global:
        0: i
        1: total
}

rule ("sum") {
    event {
        Ongoing - Global;
    }
    actions {
        For Global Variable(i, 0, 5, 1);
            If(Compare(Global.i, >=, 3));
                Modify Global Variable(total, Add, Global.i);
            End;
        End;
    }
}
"#;

#[test]
fn a_for_global_variable_containing_an_if_runs_to_the_expected_values() {
    let trace = run(FOR_IF);
    assert_eq!(trace.outcome, Outcome::Complete);
    // i runs 0..=4 (the exit check at `End` stops at 5); the `If` admits 3+4.
    assert_eq!(trace.globals["i"], Val::number(5.0));
    assert_eq!(trace.globals["total"], Val::number(7.0));
}

/// A `For` inside each branch of an `If`/`Else` chain.
const FOR_IN_IF: &str = r#"
variables {
    global:
        0: i
        1: j
        2: hits
        3: misses
}

rule ("taken") {
    event {
        Ongoing - Global;
    }
    actions {
        If(True);
            For Global Variable(i, 0, 3, 1);
                Modify Global Variable(hits, Add, 1);
            End;
        Else;
            Set Global Variable(hits, 99);
        End;
    }
}

rule ("skipped") {
    event {
        Ongoing - Global;
    }
    actions {
        If(False);
            For Global Variable(j, 0, 3, 1);
                Modify Global Variable(misses, Add, 1);
            End;
        Else;
            Set Global Variable(misses, 99);
        End;
    }
}
"#;

#[test]
fn a_for_inside_an_if_pairs_its_end_with_the_for_not_the_if() {
    let trace = run(FOR_IN_IF);
    assert_eq!(trace.outcome, Outcome::Complete);
    // The taken branch's `For` iterates; the skipped branch's `For` never
    // writes `j`, and the `Else` runs instead.
    assert_eq!(trace.globals["hits"], Val::number(3.0));
    assert_eq!(trace.globals["i"], Val::number(3.0));
    assert_eq!(trace.globals["misses"], Val::number(99.0));
    assert_eq!(trace.globals["j"], Val::Null);
}

/// A `Wait` inside a `For` body suspends and resumes the loop.
const FOR_WAIT: &str = r#"
variables {
    global:
        0: i
        1: hits
}

rule ("slow loop") {
    event {
        Ongoing - Global;
    }
    actions {
        For Global Variable(i, 0, 3, 1);
            Wait(0.016, Ignore Condition);
            Modify Global Variable(hits, Add, 1);
        End;
    }
}
"#;

#[test]
fn a_for_resumes_each_iteration_across_a_wait() {
    let trace = run(FOR_WAIT);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["hits"], Val::number(3.0));
    assert_eq!(trace.globals["i"], Val::number(3.0));
}

/// A descending `For` (negative step) and one whose start already passed the
/// stop.
const FOR_BOUNDS: &str = r#"
variables {
    global:
        0: i
        1: down
        2: never
}

rule ("bounds") {
    event {
        Ongoing - Global;
    }
    actions {
        For Global Variable(i, 5, 0, -2);
            Modify Global Variable(down, Add, 1);
        End;
        For Global Variable(never, 5, 0, 1);
            Modify Global Variable(down, Add, 10);
        End;
    }
}
"#;

#[test]
fn a_for_counts_down_and_skips_an_already_passed_range() {
    let trace = run(FOR_BOUNDS);
    assert_eq!(trace.outcome, Outcome::Complete);
    // 5, 3, 1 run (i lands on -1); `never` is written 5 and the body is
    // skipped, so `down` counts only the first loop.
    assert_eq!(trace.globals["down"], Val::number(3.0));
    assert_eq!(trace.globals["i"], Val::number(-1.0));
    assert_eq!(trace.globals["never"], Val::number(5.0));
}

/// `For Player Variable` on the `Event Player`.
const FOR_PLAYER: &str = r#"
variables {
    player:
        0: i
        1: hits
}

rule ("per player") {
    event {
        Ongoing - Each Player;
    }
    actions {
        For Player Variable(Event Player, i, 0, 3, 1);
            Modify Player Variable(Event Player, hits, Add, 1);
        End;
    }
}
"#;

#[test]
fn a_for_player_variable_counts_on_each_player() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1" }],
        "expect": [
            { "kind": "playerVariable", "player": "p1", "name": "hits", "value": 3 },
            { "kind": "playerVariable", "player": "p1", "name": "i", "value": 3 }
        ]
    }"#;
    let trace = run_scenario(FOR_PLAYER, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(
        trace
            .expectations
            .iter()
            .all(|result| result.status == ExpectationStatus::Pass),
        "{:?}",
        trace.expectations
    );
}

/// A `For Player Variable` `End` step routes the written increment
/// through the finite check: `old + step` overflowing to `inf` stops
/// `not_modeled` instead of storing a value that serializes `null`
/// (athena#133).
#[test]
fn a_for_player_variable_end_stops_on_a_non_finite_increment() {
    let text = r#"
variables {
    player:
        0: i
}

rule ("per player") {
    event {
        Ongoing - Each Player;
    }
    actions {
        For Player Variable(Event Player, i, Raise To Power(10, 308), Multiply(Raise To Power(10, 308), 1.5), Raise To Power(10, 308));
        End;
    }
}
"#;
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1, "players": [{ "id": "p1" }] }"#;
    let trace = run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        stop.construct,
        "forPlayerVariable produced a non-finite number"
    );
}

/// Two `If`/`Else If` chains in a row: the first ends on a false clause.
const CONSECUTIVE_CHAINS: &str = r#"
variables {
    global:
        0: hits
}

rule ("chains") {
    event {
        Ongoing - Global;
    }
    actions {
        If(False);
            Set Global Variable(hits, -1);
        End;
        If(True);
            Set Global Variable(hits, 1);
        Else If(True);
            Set Global Variable(hits, 2);
        End;
    }
}
"#;

#[test]
fn a_fresh_chain_does_not_inherit_a_previous_chains_clause_jump() {
    let trace = run(CONSECUTIVE_CHAINS);
    assert_eq!(trace.outcome, Outcome::Complete);
    // Only the true clause of the second chain runs.
    assert_eq!(trace.globals["hits"], Val::number(1.0));
}

/// An `If`/`End` nested in a matched clause body exhausts its chain on a
/// false condition; the pending jump must not read as the enclosing `Else`'s.
const NESTED_EXHAUSTED_IF: &str = r#"
variables {
    global:
        0: hits
}

rule ("nested else") {
    event {
        Ongoing - Global;
    }
    actions {
        If(True);
            Set Global Variable(hits, 1);
            If(False);
                Set Global Variable(hits, -1);
            End;
        Else;
            Set Global Variable(hits, 99);
        End;
    }
}

rule ("nested else-if") {
    event {
        Ongoing - Global;
    }
    actions {
        If(True);
            Set Global Variable(hits, 2);
            If(False);
                Set Global Variable(hits, -2);
            End;
        Else If(True);
            Set Global Variable(hits, 99);
        End;
    }
}
"#;

#[test]
fn an_exhausted_if_inside_a_clause_does_not_trigger_the_outer_else() {
    let trace = run(NESTED_EXHAUSTED_IF);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["hits"], Val::number(2.0));
}

/// The nested `If` must not deliver the run to the `Else If` head either: a
/// condition that is not a boolean stops `not_modeled` if it is evaluated.
const NESTED_EXHAUSTED_IF_UNEVALUATED: &str = r#"
variables {
    global:
        0: hits
}

rule ("nested else-if") {
    event {
        Ongoing - Global;
    }
    actions {
        If(True);
            Set Global Variable(hits, 1);
            If(False);
                Set Global Variable(hits, -1);
            End;
        Else If(Global.hits);
            Set Global Variable(hits, 99);
        End;
    }
}
"#;

#[test]
fn an_exhausted_if_inside_a_clause_does_not_evaluate_the_outer_else_if() {
    let trace = run(NESTED_EXHAUSTED_IF_UNEVALUATED);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["hits"], Val::number(1.0));
}

/// A subroutine ending in an exhausted `If`/`End` returns with a pending
/// jump in its own rule; the caller's `Else` head must not match it.
const SUBROUTINE_EXHAUSTED_IF: &str = r#"
variables {
    global:
        0: hits
}

subroutines {
    0: inner
}

rule ("Subroutine inner") {
    event {
        Subroutine;
        inner;
    }
    actions {
        If(False);
            Set Global Variable(hits, -1);
        End;
    }
}

rule ("caller") {
    event {
        Ongoing - Global;
    }
    actions {
        If(True);
            Set Global Variable(hits, 1);
            Call Subroutine(inner);
        Else;
            Set Global Variable(hits, 99);
        End;
    }
}
"#;

#[test]
fn an_if_exhausted_inside_a_subroutine_does_not_trigger_the_callers_else() {
    let trace = run(SUBROUTINE_EXHAUSTED_IF);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["hits"], Val::number(1.0));
}

/// A stray `End` line: the parser keeps it as a raw action, and reaching it
/// still stops the run instead of pairing with nothing.
#[test]
fn an_end_without_an_opener_still_stops_not_modeled() {
    let trace = run(r#"
rule ("stray") {
    event {
        Ongoing - Global;
    }
    actions {
        End;
    }
}
"#);
    let Outcome::NotModeled(_) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
}
