//! `Skip`/`Skip If`, `Abort`/`Abort If`, the `Loop` restarts and
//! `Break`/`Continue` inside `While` (athena#58).

use athena_sim::{Options, Outcome, RunEnd, Val, run_text};

fn run(text: &str) -> athena_sim::Trace {
    run_text(text, Options { seconds: 2.0 }).unwrap_or_else(|error| panic!("{error}"))
}

fn ended(trace: &athena_sim::Trace) -> &RunEnd {
    &trace
        .timeline
        .entries
        .last()
        .expect("a tick ran")
        .rules
        .last()
        .expect("a rule ran")
        .ended
}

/// `Skip` steps over exactly its count.
const SKIP: &str = r#"
variables {
    global:
        0: a
        1: b
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(a, 1);
        Skip(2);
        Set Global Variable(a, 98);
        Set Global Variable(a, 99);
        Set Global Variable(b, 7);
    }
}
"#;

#[test]
fn skip_steps_over_exactly_its_count() {
    let trace = run(SKIP);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["a"], Val::Number(1.0));
    assert_eq!(trace.globals["b"], Val::Number(7.0));
}

/// `Skip(0)` skips nothing; `Skip` past the list ends the run.
#[test]
fn a_zero_skip_skips_nothing_and_skipping_past_the_end_finishes() {
    let text = r#"
variables {
    global:
        0: a
        1: b
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Skip(0);
        Set Global Variable(a, 1);
        Skip(10);
        Set Global Variable(b, 99);
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["a"], Val::Number(1.0));
    assert_eq!(trace.globals["b"], Val::Null);
    assert_eq!(*ended(&trace), RunEnd::Finished);
}

/// `Skip If` skips only when its condition holds; a `True` count is one.
#[test]
fn skip_if_skips_on_true_and_a_boolean_count_is_one() {
    let text = r#"
variables {
    global:
        0: a
        1: b
        2: c
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Skip If(False, 5);
        Set Global Variable(a, 1);
        Skip If(True, True);
        Set Global Variable(b, 99);
        Set Global Variable(c, 3);
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["a"], Val::Number(1.0));
    assert_eq!(trace.globals["b"], Val::Null);
    assert_eq!(trace.globals["c"], Val::Number(3.0));
}

/// A skip that lands inside an `If` body the jump never entered runs the
/// actions there raw: the skip is positional, not structural.
#[test]
fn a_skip_into_an_if_body_runs_it() {
    let text = r#"
variables {
    global:
        0: a
        1: b
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Skip(2);
        Set Global Variable(b, 0);
        If(False);
            Set Global Variable(a, 99);
        End;
        Set Global Variable(b, 2);
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["a"], Val::Number(99.0));
    assert_eq!(trace.globals["b"], Val::Number(2.0));
}

/// `Abort` ends the action list and records `aborted`.
#[test]
fn abort_ends_the_run_aborted() {
    let text = r#"
variables {
    global:
        0: a
        1: b
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(a, 1);
        Abort;
        Set Global Variable(b, 99);
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["a"], Val::Number(1.0));
    assert_eq!(trace.globals["b"], Val::Null);
    assert_eq!(*ended(&trace), RunEnd::Aborted);
}

/// `Abort If` ends the run only when its condition holds.
#[test]
fn abort_if_ends_the_run_only_on_true() {
    let text = r#"
variables {
    global:
        0: a
        1: b
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Abort If(False);
        Set Global Variable(a, 1);
        Abort If(True);
        Set Global Variable(b, 99);
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.globals["a"], Val::Number(1.0));
    assert_eq!(trace.globals["b"], Val::Null);
    assert_eq!(*ended(&trace), RunEnd::Aborted);
}

/// An `Abort` inside a subroutine kills the caller's run too.
#[test]
fn an_abort_inside_a_subroutine_kills_the_caller() {
    let text = r#"
variables {
    global:
        0: a
        1: b
}

subroutines {
    0: die
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(a, 1);
        Call Subroutine(die);
        Set Global Variable(b, 99);
    }
}

rule ("Subroutine die") {
    event {
        Subroutine;
        die;
    }
    actions {
        Abort;
        Set Global Variable(b, 98);
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.globals["a"], Val::Number(1.0));
    assert_eq!(trace.globals["b"], Val::Null);
    assert_eq!(*ended(&trace), RunEnd::Aborted);
}

/// A bare `Loop` never ends; the step limit stops the run `not_modeled`.
#[test]
fn a_loop_that_never_ends_stops_at_the_step_limit() {
    let text = r#"
rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        True;
    }
    actions {
        Loop;
    }
}
"#;
    let trace = run(text);
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("an endless `Loop` must stop not_modeled");
    };
    assert!(stop.construct.contains("steps"), "{stop:?}");
}

/// `Loop If` re-checks its condition on every pass.
#[test]
fn loop_if_rechecks_its_condition_each_pass() {
    let text = r#"
variables {
    global:
        0: i
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        True;
    }
    actions {
        Modify Global Variable(i, Add, 1);
        Loop If(Global.i < 3);
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["i"], Val::Number(3.0));
    assert_eq!(*ended(&trace), RunEnd::Finished);
}

/// `Loop If Condition Is True` re-checks the rule's condition list on
/// every pass, not the run-start edge.
#[test]
fn loop_if_condition_is_true_rechecks_the_rule_conditions() {
    let text = r#"
variables {
    global:
        0: i
        1: passes
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        Global.i < 3;
    }
    actions {
        Modify Global Variable(i, Add, 1);
        Modify Global Variable(passes, Add, 1);
        Loop If Condition Is True;
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["i"], Val::Number(3.0));
    assert_eq!(trace.globals["passes"], Val::Number(3.0));
}

/// `Loop If Condition Is False` falls through while the conditions hold
/// and restarts once one fails — which here never stops.
#[test]
fn loop_if_condition_is_false_restarts_when_a_condition_fails() {
    let holds = r#"
variables {
    global:
        0: i
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        Global.i < 3;
    }
    actions {
        Modify Global Variable(i, Add, 1);
        Loop If Condition Is False;
        Modify Global Variable(i, Add, 10);
    }
}
"#;
    let trace = run(holds);
    assert_eq!(trace.outcome, Outcome::Complete);
    // The conditions held, so no restart and the action after ran once.
    assert_eq!(trace.globals["i"], Val::Number(11.0));

    let fails = r#"
variables {
    global:
        0: i
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        Global.i < 3;
    }
    actions {
        Set Global Variable(i, 10);
        Loop If Condition Is False;
    }
}
"#;
    let trace = run(fails);
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("a `Loop If Condition Is False` that fires must reach the step limit");
    };
    assert!(stop.construct.contains("steps"), "{stop:?}");
}

/// `Wait` + `Loop If Condition Is True` is the classic periodic pattern:
/// each wake re-checks the conditions and restarts the list.
#[test]
fn a_wait_inside_a_restarted_list_periodically_reruns() {
    let text = r#"
variables {
    global:
        0: i
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        True;
    }
    actions {
        Modify Global Variable(i, Add, 1);
        Wait(0.1, Ignore Condition);
        Loop If Condition Is True;
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    // 2 s of 112 ms iterations.
    let Val::Number(i) = trace.globals["i"] else {
        panic!("i must be a number");
    };
    assert!(
        i > 10.0,
        "the loop should have restarted many times, got {i}"
    );
}

/// `Break` leaves the innermost loop; `Continue` re-enters its head. Both
/// resolve through an intervening `If`.
#[test]
fn break_and_continue_inside_an_if_target_the_enclosing_while() {
    let text = r#"
variables {
    global:
        0: i
        1: x
        2: j
        3: y
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        True;
    }
    actions {
        While(Global.i < 10);
            Modify Global Variable(i, Add, 1);
            If(Global.i == 3);
                Break;
            End;
            Modify Global Variable(x, Add, 1);
        End;
        While(Global.j < 5);
            Modify Global Variable(j, Add, 1);
            If(Global.j < 3);
                Continue;
            End;
            Modify Global Variable(y, Add, 1);
        End;
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["i"], Val::Number(3.0));
    assert_eq!(trace.globals["x"], Val::Number(2.0));
    assert_eq!(trace.globals["j"], Val::Number(5.0));
    assert_eq!(trace.globals["y"], Val::Number(3.0));
}

/// `Continue` in a `For` lands on its `End`, which steps the variable —
/// it does not reset it.
#[test]
fn continue_in_a_for_steps_the_variable() {
    let text = r#"
variables {
    global:
        0: i
        1: x
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        True;
    }
    actions {
        For Global Variable(i, 0, 4, 1);
            If(Global.i < 2);
                Continue;
            End;
            Modify Global Variable(x, Add, 1);
        End;
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["i"], Val::Number(4.0));
    assert_eq!(trace.globals["x"], Val::Number(2.0));
}

/// `Break` outside any loop is not modeled.
#[test]
fn a_break_outside_a_loop_is_not_modeled() {
    let text = r#"
rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        True;
    }
    actions {
        If(True);
            Break;
        End;
    }
}
"#;
    let trace = run(text);
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("a `Break` outside a loop must stop not_modeled");
    };
    assert_eq!(stop.construct, "break outside a loop");
}

/// A `Loop` inside a `While` body restarts the whole action list, and the
/// re-entered `While` still exits at its bound.
#[test]
fn a_loop_inside_a_while_restarts_the_action_list() {
    let text = r#"
variables {
    global:
        0: a
        1: i
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        True;
    }
    actions {
        Set Global Variable(a, 1);
        While(Global.i < 10);
            Modify Global Variable(i, Add, 1);
            Loop;
        End;
        Set Global Variable(a, 2);
    }
}
"#;
    let trace = run(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["i"], Val::Number(10.0));
    assert_eq!(trace.globals["a"], Val::Number(2.0));
}
