//! `Current Map` reading the scenario's `map` field (athena#71): the
//! field names a catalog `Map` member in the form `hero` accepts — the
//! member id or an en-US spelling, case-insensitive — and `Current Map`
//! yields its member id as text, so it compares equal to a `Map`
//! literal. A run without the field ends `missing_input` naming `map`;
//! an unknown name fails the load.

use athena_sim::{MissingInputKind, Outcome, RunError, ScenarioError, Val, run_scenario, run_text};

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

fn run(text: &str, map: &str) -> athena_sim::Trace {
    let scenario =
        format!(r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], "map": {map} }}"#);
    run_scenario(text, &scenario).unwrap_or_else(|error| panic!("{error}"))
}

/// The member id comes back whatever spelling the scenario used — the
/// canonical `Map` member id, an en-US spelling, or either in another
/// case.
#[test]
fn the_supplied_map_is_returned_as_its_member_id() {
    let text = set_global("Current Map");
    for map in [r#""KINGS_ROW""#, r#""King's Row""#, r#""king's row""#] {
        let trace = run(&text, map);
        assert_eq!(trace.outcome, Outcome::Complete, "{map}");
        assert_eq!(trace.globals["g"], Val::Text("KINGS_ROW".into()), "{map}");
    }
}

/// A member id is the same text a `Map` literal yields, so the two
/// compare equal.
#[test]
fn current_map_compares_equal_to_a_map_literal() {
    let text = set_global("Compare(Current Map, ==, Map(King's Row))");
    let trace = run(&text, r#""King's Row""#);
    assert_eq!(trace.globals["g"], Val::Bool(true));
}

#[test]
fn without_the_field_a_current_map_read_is_a_missing_input() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1, "players": [] }"#;
    let text = set_global("Current Map");
    let trace = run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Map);
    assert_eq!(input.name.as_deref(), Some("map"));
    assert_eq!(input.tick, Some(0));

    // A run without a scenario reports it the same way.
    let trace = run_text(&text, athena_sim::Options { seconds: 1.0 })
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Map);
}

/// `map` names a catalog `Map` member, like `hero` names a `Hero` one:
/// an unknown name is a load error and an empty one is malformed.
#[test]
fn an_unknown_or_empty_map_fails_the_load() {
    let text = set_global("Current Map");
    for map in [r#""Sanctuary""#, r#""Dorado2""#] {
        let scenario =
            format!(r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], "map": {map} }}"#);
        match run_scenario(&text, &scenario) {
            Err(RunError::Scenario(ScenarioError::Load(_))) => {}
            other => panic!("map {map}: {other:?}"),
        }
    }
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1, "players": [], "map": "" }"#;
    match run_scenario(&text, scenario) {
        Err(RunError::Scenario(ScenarioError::Format(_))) => {}
        other => panic!("empty map: {other:?}"),
    }
}
