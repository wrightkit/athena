//! The element-context array values and `Evaluate Once` (athena#60):
//! the per-element binding of `Current Array Element`/`Current Array
//! Index`, and the pin scope of an `Evaluate Once` site.

use athena_sim::{Options, Outcome, Val, run_text};

fn run(text: &str) -> athena_sim::Trace {
    run_text(text, Options { seconds: 2.0 }).unwrap_or_else(|error| panic!("{error}"))
}

/// One `Ongoing - Global` rule that writes `g` once at tick 0.
fn set_global(value: &str) -> String {
    format!(
        r#"
variables {{
    global:
        0: g
}}

rule ("probe") {{
    event {{
        Ongoing - Global;
    }}
    actions {{
        Set Global Variable(g, {value});
    }}
}}
"#
    )
}

fn global(trace: &athena_sim::Trace) -> &Val {
    assert_eq!(trace.outcome, Outcome::Complete);
    &trace.globals["g"]
}

#[test]
fn evaluate_once_evaluates_its_operand() {
    let trace = run(&set_global("Evaluate Once(First Of(Array(5, 6)))"));
    assert_eq!(global(&trace), &Val::Number(5.0));

    let trace = run(&set_global("Evaluate Once(Null)"));
    assert_eq!(global(&trace), &Val::Null);
}

/// Inside an element-context operand the operand site is re-read once
/// per element, so `Evaluate Once` does not pin there; the run stops.
#[test]
fn evaluate_once_inside_an_element_context_is_not_modeled() {
    for value in [
        "Filtered Array(Array(1, 2), Compare(Evaluate Once(Current Array Element), >, 1))",
        "Sorted Array(Array(1, 2), Evaluate Once(Current Array Index))",
        "Is True For All(Array(1, 2), Evaluate Once(True))",
        "Is True For Any(Array(1, 2, 3), Compare(Evaluate Once(Current Array Index), ==, 2))",
        "Mapped Array(Array(1, 2), Evaluate Once(Current Array Element))",
        // The element context reaches through an operand's own values.
        "Is True For Any(Array(1), Is True For Any(Array(2), Evaluate Once(True)))",
    ] {
        let trace = run(&set_global(value));
        let Outcome::NotModeled(stop) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
        assert!(
            stop.construct
                .contains("evaluateOnce inside an element context"),
            "{value}: {}",
            stop.construct
        );
    }
}

/// The pin scope is one rule-instance run: a `For`/`While` body
/// re-reads the site every iteration and gets the first read back.
#[test]
fn evaluate_once_pins_within_a_run_across_loop_iterations() {
    let trace = run(r#"
variables {
    global:
        0: g
        1: i
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        For Global Variable(i, 0, 3, 1);
            Set Global Variable(g, Evaluate Once(Global.i));
        End;
    }
}
"#);
    assert_eq!(global(&trace), &Val::Number(0.0));

    let trace = run(r#"
variables {
    global:
        0: g
        1: i
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(i, 0);
        While(Compare(Global.i, <, 3));
            Set Global Variable(g, Evaluate Once(Global.i));
            Modify Global Variable(i, Add, 1);
        End;
    }
}
"#);
    assert_eq!(global(&trace), &Val::Number(0.0));
}

/// The pin clears when the instance's next run starts, so a rule that
/// fires again re-reads the site.
#[test]
fn evaluate_once_repins_when_the_rule_fires_again() {
    let trace = run(r#"
variables {
    global:
        0: g
        1: c
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    conditions {
        Compare(Global.c, >, 0);
    }
    actions {
        Set Global Variable(g, Evaluate Once(Global.c));
        Set Global Variable(c, 0);
    }
}

rule ("bump") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait(0.2, Abort When False);
        Set Global Variable(c, 1);
        Wait(0.2, Abort When False);
        Set Global Variable(c, 2);
    }
}
"#);
    assert_eq!(global(&trace), &Val::Number(2.0));
}

/// A rule-condition check is its own scope: the site re-reads every
/// pass, so a condition can edge when the pinned variable changes.
#[test]
fn evaluate_once_in_a_rule_condition_repins_each_check() {
    let trace = run(r#"
variables {
    global:
        0: g
        1: n
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    conditions {
        Evaluate Once(Global.n) == 5;
    }
    actions {
        Set Global Variable(g, 1);
    }
}

rule ("bump") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait(0.2, Abort When False);
        Set Global Variable(n, 5);
    }
}
"#);
    assert_eq!(global(&trace), &Val::Number(1.0));
}

/// A `Wait Until` call is one scope: its per-tick re-checks read the
/// first check's pin, so the wait never releases here.
#[test]
fn evaluate_once_in_wait_until_pins_across_its_rechecks() {
    let trace = run(r#"
variables {
    global:
        0: g
        1: n
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait Until(Evaluate Once(Global.n) == 5, 999);
        Set Global Variable(g, 1);
    }
}

rule ("bump") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait(0.2, Abort When False);
        Set Global Variable(n, 5);
    }
}
"#);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["g"], Val::Null);
}

#[test]
fn filtered_array_keeps_the_elements_the_condition_holds_for() {
    let trace = run(&set_global(
        "Filtered Array(Array(1, 2, 3, 4), Compare(Current Array Element, >, 2))",
    ));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::Number(3.0), Val::Number(4.0)])
    );

    // `Null` reads as the empty array.
    let trace = run(&set_global("Filtered Array(Null, Current Array Element)"));
    assert_eq!(global(&trace), &Val::Array(vec![]));

    // The bound element is read inside a nested array value too.
    let trace = run(&set_global(
        "Filtered Array(Array(1, 2), Array Contains(Array(2, 3), Current Array Element))",
    ));
    assert_eq!(global(&trace), &Val::Array(vec![Val::Number(2.0)]));
}

#[test]
fn sorted_array_orders_by_the_per_element_key() {
    let trace = run(&set_global(
        "Sorted Array(Array(30, 10, 20), Current Array Element)",
    ));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![
            Val::Number(10.0),
            Val::Number(20.0),
            Val::Number(30.0)
        ])
    );

    // `Current Array Index` binds the element's place in the source
    // array, so a descending index key reverses it.
    let trace = run(&set_global(
        "Sorted Array(Array(30, 10, 20), Subtract(0, Current Array Index))",
    ));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![
            Val::Number(20.0),
            Val::Number(10.0),
            Val::Number(30.0)
        ])
    );

    let trace = run(&set_global("Sorted Array(Null, Current Array Element)"));
    assert_eq!(global(&trace), &Val::Array(vec![]));
}

#[test]
fn is_true_for_all_and_any_cover_the_whole_array() {
    for (value, expected) in [
        (
            "Is True For All(Array(2, 4), Compare(Current Array Element, >, 1))",
            true,
        ),
        (
            "Is True For All(Array(2, 4), Compare(Current Array Element, >, 3))",
            false,
        ),
        (
            "Is True For Any(Array(2, 4), Compare(Current Array Element, >, 3))",
            true,
        ),
        (
            "Is True For Any(Array(2, 4), Compare(Current Array Element, >, 9))",
            false,
        ),
        // The empty array: vacuously true for `All`, never for `Any`.
        ("Is True For All(Empty Array, False)", true),
        ("Is True For Any(Null, True)", false),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Bool(expected), "{value}");
    }
}

/// `Mapped Array` evaluates the operand once per element with `Current
/// Array Element`/`Current Array Index` bound and collects the results
/// — the element-context rule the other array values follow (athena#134,
/// modeled not measured).
#[test]
fn mapped_array_collects_the_per_element_result() {
    let trace = run(&set_global(
        "Mapped Array(Array(10, 20, 30), Add(Current Array Element, Current Array Index))",
    ));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![
            Val::Number(10.0),
            Val::Number(21.0),
            Val::Number(32.0)
        ])
    );

    // The empty array maps to nothing, and `Null` reads as it.
    for array in ["Empty Array", "Null"] {
        let trace = run(&set_global(&format!(
            "Mapped Array({array}, Current Array Element)"
        )));
        assert_eq!(global(&trace), &Val::Array(vec![]), "{array}");
    }

    // A non-array operand stops the same way the other array values do.
    let trace = run(&set_global("Mapped Array(5, Current Array Element)"));
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("{:?}", trace.outcome);
    };
    assert_eq!(stop.construct, "mappedArray on Number(5.0)");

    // A constant mapping — the Bastion form — needs no element read.
    let trace = run(&set_global("Mapped Array(Array(1, 2), -30)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::Number(-30.0), Val::Number(-30.0)])
    );

    // A nested element-context value rebinds the pair for its own
    // operand; the outer binding applies again outside it.
    let trace = run(&set_global(
        "Mapped Array(Array(1, 2), Array(Current Array Element, Count Of(Mapped Array(Array(9), Current Array Index))))",
    ));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![
            Val::Array(vec![Val::Number(1.0), Val::Number(1.0)]),
            Val::Array(vec![Val::Number(2.0), Val::Number(1.0)]),
        ])
    );
}

/// A mapped number is a produced number: one that is not finite — here
/// the variable a `For` `End` step left `inf` — stops `not_modeled`
/// instead of serializing `null` (athena#134).
#[test]
fn mapped_array_stops_on_a_non_finite_result() {
    let trace = run(r#"
variables {
    global:
        0: g
        1: r
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        For Global Variable(g, Raise To Power(10, 308), Multiply(Raise To Power(10, 308), 1.5), Raise To Power(10, 308));
        End;
        Set Global Variable(r, Mapped Array(Array(1), Global.g));
    }
}
"#);
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("{:?}", trace.outcome);
    };
    assert_eq!(stop.construct, "mappedArray produced a non-finite number");
}

#[test]
fn current_array_element_outside_an_element_context_is_not_modeled() {
    for value in ["Current Array Element", "Current Array Index"] {
        let trace = run(&set_global(value));
        let Outcome::NotModeled(stop) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
        assert!(stop.construct.contains("outside an element context"));
    }
}
