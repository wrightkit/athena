//! General and array value constructs plus the `At Index` variable
//! actions (athena#56), and the element-context array values and
//! `Evaluate Once` (athena#60).

use athena_sim::{ExpectationStatus, Options, Outcome, Val, run_scenario, run_text};

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
fn if_then_else_reads_only_the_taken_branch() {
    let trace = run(&set_global(
        "If-Then-Else(Compare(1, <, 2), 10, Position Of(Null))",
    ));
    assert_eq!(global(&trace), &Val::Number(10.0));

    // A `Null` condition reads as `0`, so the else branch wins.
    let trace = run(&set_global("If-Then-Else(Null, 10, 20)"));
    assert_eq!(global(&trace), &Val::Number(20.0));
}

#[test]
fn compare_covers_all_operators_arrays_and_null() {
    for (value, expected) in [
        ("Compare(2, !=, 3)", true),
        ("Compare(2, <=, 2)", true),
        ("Compare(3, >, 2)", true),
        ("Compare(Array(1, 2), ==, Array(1, 2))", true),
        ("Compare(Array(1, 2), ==, Array(1, 3))", false),
        ("Compare(Array(1), !=, Array(1, 2))", true),
        ("Compare(Null, ==, 0)", true),
        ("Compare(Null, ==, Null)", true),
        ("Compare(1, ==, \"1\")", false),
        ("Compare(Array(1), ==, 1)", false),
        ("Compare(False, ==, 0)", true),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Bool(expected), "{value}");
    }
}

#[test]
fn not_and_or_read_truthiness_and_short_circuit() {
    let trace = run(&set_global("Not(Or(False, True))"));
    assert_eq!(global(&trace), &Val::Bool(false));

    // `Or` on numbers reads nonzero-ness.
    let trace = run(&set_global("Or(0, 3)"));
    assert_eq!(global(&trace), &Val::Bool(true));

    // A decided `Or` never reads its right side.
    let trace = run(&set_global("Or(True, Position Of(Null))"));
    assert_eq!(global(&trace), &Val::Bool(true));
}

#[test]
fn divide_modulo_min_max_and_raise_to_power_do_arithmetic() {
    for (value, expected) in [
        ("Divide(7, 2)", 3.5),
        ("Divide(Null, 5)", 0.0),
        ("Modulo(7, 3)", 1.0),
        ("Min(2, 5)", 2.0),
        ("Max(2, 5)", 5.0),
        ("Raise To Power(2, 10)", 1024.0),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Number(expected), "{value}");
    }
}

#[test]
fn round_to_integer_follows_the_rounding_member() {
    for (value, expected) in [
        ("Round To Integer(2.1, Up)", 3.0),
        ("Round To Integer(2.9, Down)", 2.0),
        ("Round To Integer(2.5, Nearest)", 3.0),
        ("Round To Integer(-2.5, Nearest)", -3.0),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Number(expected), "{value}");
    }
}

#[test]
fn first_of_and_count_of_see_null_as_the_empty_array() {
    let trace = run(&set_global("First Of(Array(5, 6))"));
    assert_eq!(global(&trace), &Val::Number(5.0));

    let trace = run(&set_global("First Of(Empty Array)"));
    assert_eq!(global(&trace), &Val::Null);

    let trace = run(&set_global("First Of(Null)"));
    assert_eq!(global(&trace), &Val::Null);

    let trace = run(&set_global("Count Of(Array(1, 2, 3))"));
    assert_eq!(global(&trace), &Val::Number(3.0));

    let trace = run(&set_global("Count Of(Null)"));
    assert_eq!(global(&trace), &Val::Number(0.0));
}

#[test]
fn array_contains_and_index_of_array_value_search_by_compare_equality() {
    let trace = run(&set_global("Array Contains(Array(1, 2), Null)"));
    assert_eq!(global(&trace), &Val::Bool(false));

    // `Null` equals `0` under the `Compare` coercion.
    let trace = run(&set_global("Array Contains(Array(Null, 2), 0)"));
    assert_eq!(global(&trace), &Val::Bool(true));

    let trace = run(&set_global("Index Of Array Value(Array(4, 5), 5)"));
    assert_eq!(global(&trace), &Val::Number(1.0));

    let trace = run(&set_global("Index Of Array Value(Array(4, 5), 9)"));
    assert_eq!(global(&trace), &Val::Number(-1.0));

    let trace = run(&set_global("Index Of Array Value(Null, 9)"));
    assert_eq!(global(&trace), &Val::Number(-1.0));
}

#[test]
fn array_slice_clamps_its_bounds() {
    let trace = run(&set_global("Array Slice(Array(1, 2, 3, 4), 1, 2)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::Number(2.0), Val::Number(3.0)])
    );

    // A start past the end yields the empty array.
    let trace = run(&set_global("Array Slice(Array(1, 2), 9, 2)"));
    assert_eq!(global(&trace), &Val::Array(vec![]));

    // A count past the end stops at the end.
    let trace = run(&set_global("Array Slice(Array(1, 2, 3), 1, 99)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::Number(2.0), Val::Number(3.0)])
    );
}

#[test]
fn append_to_array_grows_arrays_and_null() {
    let trace = run(&set_global("Append To Array(Array(1), 2)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::Number(1.0), Val::Number(2.0)])
    );

    let trace = run(&set_global("Append To Array(Null, 2)"));
    assert_eq!(global(&trace), &Val::Array(vec![Val::Number(2.0)]));
}

#[test]
fn modify_global_variable_covers_the_operator_set() {
    for (op, initial, operand, expected) in [
        ("Divide", "10", "4", Val::Number(2.5)),
        ("Modulo", "10", "4", Val::Number(2.0)),
        ("Min", "10", "4", Val::Number(4.0)),
        ("Max", "10", "4", Val::Number(10.0)),
        ("Raise To Power", "2", "8", Val::Number(256.0)),
        (
            "Append To Array",
            "Null",
            "7",
            Val::Array(vec![Val::Number(7.0)]),
        ),
        (
            "Remove From Array By Value",
            "Array(1, 2, 1, 3)",
            "1",
            Val::Array(vec![Val::Number(2.0), Val::Number(3.0)]),
        ),
        (
            "Remove From Array By Index",
            "Array(1, 2, 3)",
            "9",
            Val::Array(vec![Val::Number(1.0), Val::Number(2.0), Val::Number(3.0)]),
        ),
    ] {
        let text = format!(
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
        Set Global Variable(g, {initial});
        Modify Global Variable(g, {op}, {operand});
    }}
}}
"#
        );
        let trace = run(&text);
        assert_eq!(global(&trace), &expected, "{op}");
    }
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

/// `Set`/`Modify Global Variable At Index` writes: past-end appends, a
/// negative index writes nothing, a modify outside the array skips.
const AT_INDEX: &str = r#"
variables {
    global:
        0: g
        1: m
        2: skipped
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable At Index(g, 1, 20);
        Set Global Variable At Index(g, 0, 10);
        Set Global Variable At Index(g, 9, 30);
        Set Global Variable At Index(g, 1, 15);
        Set Global Variable At Index(skipped, -1, 1);
        Set Global Variable(m, Array(1, 2));
        Modify Global Variable At Index(m, 1, Add, 10);
        Modify Global Variable At Index(m, 9, Add, 10);
    }
}
"#;

#[test]
fn at_index_writes_append_clamp_or_skip() {
    let trace = run(AT_INDEX);
    assert_eq!(trace.outcome, Outcome::Complete);
    // Index 1 on `Null` appends at 0, index 0 replaces it, index 9 past
    // the end appends, index 1 replaces that element.
    assert_eq!(
        trace.globals["g"],
        Val::Array(vec![Val::Number(10.0), Val::Number(15.0)])
    );
    assert_eq!(trace.globals["skipped"], Val::Null);
    // Index 1 is modified; index 9 writes nothing.
    assert_eq!(
        trace.globals["m"],
        Val::Array(vec![Val::Number(1.0), Val::Number(12.0)])
    );
    // The element writes record their effective index.
    let indices: Vec<_> = trace
        .timeline
        .entries
        .iter()
        .flat_map(|entry| entry.changes.iter())
        .filter_map(|change| match change {
            athena_sim::Change::GlobalVariable { index, .. } => Some(index.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        indices,
        vec![
            Some(vec![0]),
            Some(vec![0]),
            Some(vec![1]),
            Some(vec![1]),
            None,
            Some(vec![1])
        ]
    );
}

/// `Set`/`Modify Player Variable At Index` on the `Event Player`.
const PLAYER_AT_INDEX: &str = r#"
variables {
    player:
        0: arr
}

rule ("probe") {
    event {
        Ongoing - Each Player;
    }
    actions {
        Set Player Variable At Index(Event Player, arr, 0, Array(1));
        Set Player Variable At Index(Event Player, arr, 1, 2);
        Modify Player Variable At Index(Event Player, arr, 0, Append To Array, 9);
    }
}
"#;

#[test]
fn player_variable_at_index_writes_the_element() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1" }],
        "expect": [
            { "kind": "playerVariable", "player": "p1", "name": "arr",
              "value": [[1, 9], 2] }
        ]
    }"#;
    let trace = run_scenario(PLAYER_AT_INDEX, scenario).unwrap_or_else(|error| panic!("{error}"));
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

/// An enum literal in a value position yields its member id as text, so a
/// `Hero(...)` literal compares equal to `Hero Of`.
#[test]
fn an_enum_value_compares_by_its_member_id() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "hero": "Ana", "team": "team1", "host": true }],
        "expect": [
            { "kind": "global", "name": "g", "value": true }
        ]
    }"#;
    let text = r#"
variables {
    global:
        0: g
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(g, Compare(Hero Of(Host Player), ==, Hero(Ana)));
    }
}
"#;
    let trace = run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        trace
            .expectations
            .iter()
            .all(|result| result.status == ExpectationStatus::Pass),
        "{:?}",
        trace.expectations
    );
}

/// `Team Of` yields the catalog member id too, so it compares equal to a
/// `Team` literal — equality `Array Contains`/`Index Of Array Value`
/// inherit through `Compare` — and feeds `All Players`.
#[test]
fn team_of_compares_equal_to_a_team_literal() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "team": "team1", "host": true },
            { "id": "p2", "team": "team2" }
        ]
    }"#;
    let text = r#"
variables {
    global:
        0: same
        1: other
        2: inside
        3: members
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(same, Compare(Team Of(Host Player), ==, Team 1));
        Set Global Variable(other, Compare(Team Of(Host Player), !=, Team 1));
        Set Global Variable(inside, Array Contains(Array(Team 1), Team Of(Host Player)));
        Set Global Variable(members, Count Of(All Players(Team Of(Host Player))));
    }
}
"#;
    let trace = run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["same"], Val::Bool(true));
    assert_eq!(trace.globals["other"], Val::Bool(false));
    assert_eq!(trace.globals["inside"], Val::Bool(true));
    assert_eq!(trace.globals["members"], Val::Number(1.0));
}
