//! `Server Load`, `Server Load Average` and `Server Load Peak` reading
//! the scenario's `serverLoad` constant (athena#67): each value returns
//! the supplied number, a run without the field ends `missing_input`,
//! and a `Wait` on a load expression lasts the derived time.

use athena_sim::{
    Change, MissingInputKind, Outcome, RunEnd, RunError, ScenarioError, Val, run_scenario, run_text,
};

fn loaded(text: &str) -> athena_sim::Trace {
    let scenario = r#"{
        "scenarioVersion": 1, "seconds": 1, "players": [],
        "serverLoad": { "average": 25, "peak": 90, "current": 40 }
    }"#;
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
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

#[test]
fn each_value_returns_the_supplied_number() {
    for (value, expected) in [
        ("Server Load", 40.0),
        ("Server Load Average", 25.0),
        ("Server Load Peak", 90.0),
    ] {
        let trace = loaded(&set_global(value));
        assert_eq!(trace.outcome, Outcome::Complete, "{value}");
        assert_eq!(trace.globals["g"], Val::number(expected), "{value}");
    }
}

#[test]
fn without_the_field_a_server_load_read_is_a_missing_input() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1, "players": [] }"#;
    let text = set_global("Server Load Average");
    let trace = run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::ServerLoad);
    assert_eq!(input.name.as_deref(), Some("serverLoad"));
    assert_eq!(input.tick, Some(0));

    // A run without a scenario reports it the same way.
    let trace = run_text(&text, athena_sim::Options { seconds: 1.0 })
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::ServerLoad);
}

/// Bastion's pacing idiom: `Wait(Divide(Server Load Average, 100) * k)`.
/// `average` 25 gives 0.25 s, 16 ticks (P1 rounding): the rule suspends
/// on tick 0 and resumes on tick 16.
#[test]
fn a_wait_on_a_load_expression_advances_by_the_derived_time() {
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
        Wait(Divide(Server Load Average, 100), Ignore Condition);
        Set Global Variable(g, 1);
    }
}
"#;
    let trace = loaded(text);
    assert_eq!(trace.outcome, Outcome::Complete);
    let [start, resume] = trace.timeline.entries.as_slice() else {
        panic!("expected two entries, got {:?}", trace.timeline.entries);
    };
    assert_eq!(start.tick, 0);
    assert_eq!(start.rules[0].ended, RunEnd::Waiting { wake: 16 });
    assert_eq!(resume.tick, 16);
    assert_eq!(
        resume.changes.as_slice(),
        [Change::GlobalVariable {
            name: "g".into(),
            index: None,
            old: Val::Null,
            new: Val::number(1.0),
        }]
    );
}

/// `serverLoad` is an object of the three numbers: `null` is not
/// "absent", a partial object is malformed, and so are an extra field
/// and a non-number member.
#[test]
fn the_field_is_an_object_of_three_numbers() {
    let text = set_global("Server Load Average");
    for server_load in [
        "null",
        r#"{ "average": 25, "peak": 90 }"#,
        r#"{ "average": 25, "peak": 90, "current": 40, "extra": 1 }"#,
        r#"{ "average": "high", "peak": 90, "current": 40 }"#,
    ] {
        let scenario = format!(
            r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], "serverLoad": {server_load} }}"#
        );
        match run_scenario(&text, &scenario) {
            Err(RunError::Scenario(ScenarioError::Format(_))) => {}
            other => panic!("serverLoad {server_load}: {other:?}"),
        }
    }
}
