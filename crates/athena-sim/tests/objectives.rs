//! `Objective Position` reading the scenario's `objectives` list
//! (athena#82): entry `i` is the position of objective index `i` — the
//! evaluated argument under the number coercion, so `False`/`True` read
//! as 0/1, the catalog's index "starting at 0 and counting up". A run
//! without the field, or whose index names no entry, ends
//! `missing_input` naming `objectives[i]`; other objective-derived
//! values stay `not_modeled`.

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

/// Three objective positions, entry `i` the point `(10·i + 1, i, 0)`.
const OBJECTIVES: &str = r#""objectives": [
        { "x": 1, "y": 0, "z": 0 },
        { "x": 11, "y": 1, "z": 0 },
        { "x": 21, "y": 2, "z": 0 }
    ]"#;

fn run(text: &str) -> athena_sim::Trace {
    let scenario =
        format!(r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], {OBJECTIVES} }}"#);
    run_scenario(text, &scenario).unwrap_or_else(|error| panic!("{error}"))
}

fn vec(x: f64, y: f64, z: f64) -> Val {
    Val::Vector { x, y, z }
}

#[test]
fn objective_position_returns_the_entry_at_the_evaluated_index() {
    for (index, expected) in [(0, vec(1.0, 0.0, 0.0)), (2, vec(21.0, 2.0, 0.0))] {
        let trace = run(&set_global(&format!("Objective Position({index})")));
        assert_eq!(trace.outcome, Outcome::Complete, "{index}");
        assert_eq!(trace.globals["g"], expected, "{index}");
    }

    // The argument evaluates: `Add(1, 1)` names index 2.
    let trace = run(&set_global("Objective Position(Add(1, 1))"));
    assert_eq!(trace.globals["g"], vec(21.0, 2.0, 0.0));
}

/// `False`/`True` read as indexes 0/1 under the number coercion — the
/// boolean form Bastion uses at bastion.ow:1175.
#[test]
fn the_boolean_form_reads_as_indexes_0_and_1() {
    for (argument, expected) in [("False", vec(1.0, 0.0, 0.0)), ("True", vec(11.0, 1.0, 0.0))] {
        let trace = run(&set_global(&format!("Objective Position({argument})")));
        assert_eq!(trace.globals["g"], expected, "{argument}");
    }
}

#[test]
fn a_missing_field_or_entry_is_a_missing_input_naming_objectives_i() {
    // No `objectives` field at all.
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1, "players": [] }"#;
    let text = set_global("Objective Position(0)");
    let trace = run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Objectives);
    assert_eq!(input.player, None);
    assert_eq!(input.name.as_deref(), Some("objectives[0]"));
    assert_eq!(input.tick, Some(0));

    // A run without a scenario reports it the same way.
    let trace = run_text(&text, athena_sim::Options { seconds: 1.0 })
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Objectives);

    // An index that names no entry — past the end, negative or
    // fractional — is missing the same way.
    for (argument, name) in [
        ("3", "objectives[3]"),
        ("-1", "objectives[-1]"),
        ("0.5", "objectives[0.5]"),
    ] {
        let trace = run(&set_global(&format!("Objective Position({argument})")));
        assert_eq!(trace.outcome, Outcome::MissingInput, "{argument}");
        let [input] = trace.missing_inputs.as_slice() else {
            panic!("{argument}: {:?}", trace.missing_inputs);
        };
        assert_eq!(input.input, MissingInputKind::Objectives, "{argument}");
        assert_eq!(input.name.as_deref(), Some(name), "{argument}");
    }
}

/// `objectives` is supplied whole or not at all: `null`, a non-list and
/// an entry that is not a whole position object are all malformed.
#[test]
fn a_malformed_objectives_field_fails_the_load() {
    let text = set_global("1");
    for objectives in [
        r#""objectives": null"#,
        r#""objectives": { "x": 1, "y": 2, "z": 3 }"#,
        r#""objectives": [{ "x": 1, "y": 2 }]"#,
        r#""objectives": [{ "x": 1, "y": 2, "z": 3, "w": 4 }]"#,
        r#""objectives": [1]"#,
    ] {
        let scenario =
            format!(r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], {objectives} }}"#);
        match run_scenario(&text, &scenario) {
            Err(RunError::Scenario(ScenarioError::Format(_))) => {}
            other => panic!("{objectives}: {other:?}"),
        }
    }
}

/// The rest of the objective family has no list to read and stays
/// `not_modeled`.
#[test]
fn other_objective_values_stay_not_modeled() {
    for (value, construct) in [
        ("Objective Index", "value getCurrentObjective"),
        ("Is Objective Complete(0)", "value isObjectiveComplete"),
        ("Payload Position", "value getPayloadPosition"),
    ] {
        let trace = run(&set_global(value));
        match &trace.outcome {
            Outcome::NotModeled(stop) => assert_eq!(stop.construct, construct, "{value}"),
            other => panic!("{value}: {other:?}"),
        }
    }
}
