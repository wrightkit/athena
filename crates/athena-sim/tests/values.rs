//! General and array value constructs plus the `At Index` variable
//! actions (athena#56).

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

#[track_caller]
fn assert_not_modeled(trace: &athena_sim::Trace, construct: &str) {
    match &trace.outcome {
        Outcome::NotModeled(stop) => assert_eq!(stop.construct, construct),
        other => panic!("expected not_modeled, got {other:?}"),
    }
}

#[test]
fn if_then_else_reads_only_the_taken_branch() {
    let trace = run(&set_global(
        "If-Then-Else(Compare(1, <, 2), 10, Position Of(Null))",
    ));
    assert_eq!(global(&trace), &Val::number(10.0));

    // A `Null` condition reads as `0`, so the else branch wins.
    let trace = run(&set_global("If-Then-Else(Null, 10, 20)"));
    assert_eq!(global(&trace), &Val::number(20.0));
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
        assert_eq!(global(&trace), &Val::number(expected), "{value}");
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
        assert_eq!(global(&trace), &Val::number(expected), "{value}");
    }
}

/// A `.ow` number literal the `f64` range cannot hold — the lexer
/// accepts any digit run and `f64::from_str` saturates it to infinity —
/// stops `not_modeled` at evaluation instead of reaching a store sink
/// and serializing `null` (athena#56).
#[test]
fn a_non_finite_number_literal_stops_at_evaluation() {
    let digits = "9".repeat(400);
    for value in [
        digits.clone(),
        format!("Append To Array(Empty Array, {digits})"),
    ] {
        let trace = run(&set_global(&value));
        assert_not_modeled(&trace, "number literal produced a non-finite number");
    }

    // The `For` start argument evaluates the same literal.
    let text = format!(
        r#"
variables {{
    global:
        0: i
}}

rule ("probe") {{
    event {{
        Ongoing - Global;
    }}
    actions {{
        For Global Variable(i, {digits}, 0, -1);
        End;
    }}
}}
"#
    );
    let trace = run(&text);
    assert_not_modeled(&trace, "number literal produced a non-finite number");
}

/// A `For` `End` step routes its written value through the finite check:
/// `old + step` overflowing to `inf` stops `not_modeled` at the `End`
/// instead of storing a value that would serialize `null` (athena#133).
#[test]
fn for_end_stops_on_a_non_finite_increment() {
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
        Set Global Variable(r, Round To Integer(Global.g, Up));
    }
}
"#);
    assert_not_modeled(&trace, "forGlobalVariable produced a non-finite number");
}

/// `Val::Number` can only hold a `Finite`: the serialized trace can
/// never carry `{"type":"number","value":null}` — every produced
/// number is checked before it becomes a value (athena#133).
#[test]
fn a_serialized_trace_never_carries_a_null_number() {
    // The scenario trace covers the structures a `number` can live in:
    // globals, change `old`/`new`s, clock and pool fields, event
    // values, the timeline `time`.
    let program = r#"
variables {
    global:
        0: g
}

rule ("probe") {
    event {
        Player Joined Match;
        All;
        All;
    }
    actions {
        Damage(Event Player, Null, 25);
        Set Slow Motion(50);
        Set Match Time(60);
        Set Global Variable(g, Add(1, 2));
    }
}
"#;
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "host": true, "hero": "ana" }],
        "events": [{ "kind": "playerJoined", "time": 0, "eventPlayer": "p1" }]
    }"#;
    for trace in [
        run(&set_global("Add(1, 2)")),
        run_scenario(program, scenario).unwrap_or_else(|error| panic!("{error}")),
    ] {
        let json = serde_json::to_string(&trace).expect("trace serializes");
        assert!(!json.contains(r#"{"type":"number","value":null}"#));
    }
}

#[test]
fn first_of_and_count_of_see_null_as_the_empty_array() {
    let trace = run(&set_global("First Of(Array(5, 6))"));
    assert_eq!(global(&trace), &Val::number(5.0));

    let trace = run(&set_global("First Of(Empty Array)"));
    assert_eq!(global(&trace), &Val::Null);

    let trace = run(&set_global("First Of(Null)"));
    assert_eq!(global(&trace), &Val::Null);

    let trace = run(&set_global("Count Of(Array(1, 2, 3))"));
    assert_eq!(global(&trace), &Val::number(3.0));

    let trace = run(&set_global("Count Of(Null)"));
    assert_eq!(global(&trace), &Val::number(0.0));
}

/// `Last Of` is the array tail, symmetric to `First Of` (athena#105):
/// the last element, `Null` on an empty array — the `0` the catalog
/// promises — and `Null` seen as the empty array.
#[test]
fn last_of_sees_null_as_the_empty_array() {
    let trace = run(&set_global("Last Of(Array(5, 6))"));
    assert_eq!(global(&trace), &Val::number(6.0));

    let trace = run(&set_global("Last Of(Array(5))"));
    assert_eq!(global(&trace), &Val::number(5.0));

    let trace = run(&set_global("Last Of(Empty Array)"));
    assert_eq!(global(&trace), &Val::Null);

    let trace = run(&set_global("Last Of(Null)"));
    assert_eq!(global(&trace), &Val::Null);
}

/// `Sine From Radians` is plain math on the evaluated argument
/// (athena#105).
#[test]
fn sine_from_radians_evaluates_its_argument() {
    for (value, expected) in [
        ("Sine From Radians(0)", 0.0),
        ("Sine From Radians(3.14159265 / 2)", 1.0),
        ("Sine From Radians(-3.14159265 / 2)", -1.0),
    ] {
        let trace = run(&set_global(value));
        let Val::Number(sine) = global(&trace) else {
            panic!("{value}: {:?}", global(&trace));
        };
        assert!(
            (sine.get() - expected).abs() < 1e-9,
            "{value}: {}",
            sine.get()
        );
    }
}

/// `Value In Array` yields the element at the index, or `Null` when no
/// element exists there — an out-of-range or negative index, or a
/// non-array operand, which has no element at any index (athena#97,
/// modeled not measured).
#[test]
fn value_in_array_yields_null_when_no_element_exists() {
    let trace = run(&set_global("Value In Array(Array(1, 2), 0)"));
    assert_eq!(global(&trace), &Val::number(1.0));

    // Out-of-range and negative indexes already yielded `Null`.
    let trace = run(&set_global("Value In Array(Array(1, 2), 9)"));
    assert_eq!(global(&trace), &Val::Null);

    let trace = run(&set_global("Value In Array(Array(1, 2), -1)"));
    assert_eq!(global(&trace), &Val::Null);

    // A non-array operand has no element at any index.
    for operand in ["Null", "5", "\"text\"", "True", "Vector(1, 2, 3)"] {
        let trace = run(&set_global(&format!("Value In Array({operand}, 0)")));
        assert_eq!(global(&trace), &Val::Null, "{operand}");
    }

    // `First Of` of `Null` and of the empty array both yield `Null`, so
    // the `Value In Array(First Of(x), True)` form yields `Null` when
    // `x` is unset.
    let trace = run(&set_global("Value In Array(First Of(Null), True)"));
    assert_eq!(global(&trace), &Val::Null);

    // A `Player` is not an array either.
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "host": true }]
    }"#;
    let trace = run_scenario(&set_global("Value In Array(Host Player, 0)"), scenario)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(global(&trace), &Val::Null);
}

/// The `Null` answer compares equal to `0` inside `Compare`, and the
/// value reads the same inside an element-context operand.
#[test]
fn value_in_array_on_a_non_array_works_inside_compare_and_is_true_for_any() {
    let trace = run(&set_global("Compare(Value In Array(Null, 0), ==, 0)"));
    assert_eq!(global(&trace), &Val::Bool(true));

    // Every element here is a non-array, so each read yields `Null` == 0.
    let trace = run(&set_global(
        "Is True For Any(Array(1, Null), Compare(Value In Array(Current Array Element, 0), ==, 0))",
    ));
    assert_eq!(global(&trace), &Val::Bool(true));
}

#[test]
fn array_contains_and_index_of_array_value_search_by_compare_equality() {
    let trace = run(&set_global("Array Contains(Array(1, 2), Null)"));
    assert_eq!(global(&trace), &Val::Bool(false));

    // `Null` equals `0` under the `Compare` coercion.
    let trace = run(&set_global("Array Contains(Array(Null, 2), 0)"));
    assert_eq!(global(&trace), &Val::Bool(true));

    let trace = run(&set_global("Index Of Array Value(Array(4, 5), 5)"));
    assert_eq!(global(&trace), &Val::number(1.0));

    let trace = run(&set_global("Index Of Array Value(Array(4, 5), 9)"));
    assert_eq!(global(&trace), &Val::number(-1.0));

    let trace = run(&set_global("Index Of Array Value(Null, 9)"));
    assert_eq!(global(&trace), &Val::number(-1.0));
}

#[test]
fn array_slice_clamps_its_bounds() {
    let trace = run(&set_global("Array Slice(Array(1, 2, 3, 4), 1, 2)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::number(2.0), Val::number(3.0)])
    );

    // A start past the end yields the empty array.
    let trace = run(&set_global("Array Slice(Array(1, 2), 9, 2)"));
    assert_eq!(global(&trace), &Val::Array(vec![]));

    // A count past the end stops at the end.
    let trace = run(&set_global("Array Slice(Array(1, 2, 3), 1, 99)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::number(2.0), Val::number(3.0)])
    );
}

#[test]
fn append_to_array_grows_arrays_and_null() {
    let trace = run(&set_global("Append To Array(Array(1), 2)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::number(1.0), Val::number(2.0)])
    );

    let trace = run(&set_global("Append To Array(Null, 2)"));
    assert_eq!(global(&trace), &Val::Array(vec![Val::number(2.0)]));
}

/// `Remove From Array` drops every element equal to the operand — the
/// same removal `Remove From Array By Value` performs — and an array
/// operand drops every element equal to any element it holds
/// (athena#105, modeled not measured).
#[test]
fn remove_from_array_drops_every_match() {
    let trace = run(&set_global("Remove From Array(Array(1, 2, 1, 3), 1)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::number(2.0), Val::number(3.0)])
    );

    // `Null` reads as the empty array.
    let trace = run(&set_global("Remove From Array(Null, 1)"));
    assert_eq!(global(&trace), &Val::Array(vec![]));

    // An absent operand removes nothing.
    let trace = run(&set_global("Remove From Array(Array(1, 2), 9)"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::number(1.0), Val::number(2.0)])
    );

    // `Null` equals `0` under the `Compare` coercion.
    let trace = run(&set_global("Remove From Array(Array(Null, 0, 1), Null)"));
    assert_eq!(global(&trace), &Val::Array(vec![Val::number(1.0)]));

    // An array operand drops each element it holds.
    let trace = run(&set_global(
        "Remove From Array(Array(1, 2, 3, 2), Array(2, 3))",
    ));
    assert_eq!(global(&trace), &Val::Array(vec![Val::number(1.0)]));

    // The Bastion form: a player operand out of a player array.
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "host": true },
            { "id": "p2", "host": false }
        ]
    }"#;
    let trace = run_scenario(
        &set_global("Remove From Array(All Players(All Teams), Host Player)"),
        scenario,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::Player("p2".to_string())])
    );
}

#[test]
fn modify_global_variable_covers_the_operator_set() {
    for (op, initial, operand, expected) in [
        ("Divide", "10", "4", Val::number(2.5)),
        ("Modulo", "10", "4", Val::number(2.0)),
        ("Min", "10", "4", Val::number(4.0)),
        ("Max", "10", "4", Val::number(10.0)),
        ("Raise To Power", "2", "8", Val::number(256.0)),
        (
            "Append To Array",
            "Null",
            "7",
            Val::Array(vec![Val::number(7.0)]),
        ),
        (
            "Remove From Array By Value",
            "Array(1, 2, 1, 3)",
            "1",
            Val::Array(vec![Val::number(2.0), Val::number(3.0)]),
        ),
        (
            "Remove From Array By Index",
            "Array(1, 2, 3)",
            "9",
            Val::Array(vec![Val::number(1.0), Val::number(2.0), Val::number(3.0)]),
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
        Val::Array(vec![Val::number(10.0), Val::number(15.0)])
    );
    assert_eq!(trace.globals["skipped"], Val::Null);
    // Index 1 is modified; index 9 writes nothing.
    assert_eq!(
        trace.globals["m"],
        Val::Array(vec![Val::number(1.0), Val::number(12.0)])
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
    assert_eq!(trace.globals["members"], Val::number(1.0));
}
