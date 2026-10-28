//! The `slot` player field: what `Slot Of` reads and a `Slot N` rule-event
//! filter matches (athena#99).

use athena_sim::{MissingInputKind, Outcome, RunError, ScenarioError, Val, run_scenario};

/// Reads `Slot Of` of the event player.
const SLOT_OF: &str = r#"
variables {
    global:
        0: s
}

rule ("slot") {
    event {
        Player Dealt Final Blow;
        All;
        All;
    }
    actions {
        Set Global Variable(s, Slot Of(Event Player));
    }
}
"#;

/// Reads `Slot Of` of a non-player operand.
const SLOT_OF_NUMBER: &str = r#"
variables {
    global:
        0: s
}

rule ("slot") {
    event {
        Player Dealt Final Blow;
        All;
        All;
    }
    actions {
        Set Global Variable(s, Slot Of(5));
    }
}
"#;

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn slot_of_reads_the_declared_slot() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 2,
        "players": [{ "id": "p1", "slot": 5 }],
        "events": [{ "kind": "playerDealtFinalBlow", "time": 0, "eventPlayer": "p1" }]
    }"#;
    let trace = run(SLOT_OF, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["s"], Val::number(5.0));
}

#[test]
fn a_missing_slot_is_a_missing_input() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 2,
        "players": [{ "id": "p1" }],
        "events": [{ "kind": "playerDealtFinalBlow", "time": 0, "eventPlayer": "p1" }]
    }"#;
    let trace = run(SLOT_OF, scenario);
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [missing] = trace.missing_inputs.as_slice() else {
        panic!("expected one missing input, got {:?}", trace.missing_inputs);
    };
    assert_eq!(missing.input, MissingInputKind::Slot);
    assert_eq!(missing.player.as_deref(), Some("p1"));
    assert_eq!(missing.name.as_deref(), Some("slot"));
    assert_eq!(missing.tick, Some(0));
}

#[test]
fn slot_of_a_non_player_operand_is_not_modeled() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 2,
        "players": [{ "id": "p1", "slot": 5 }],
        "events": [{ "kind": "playerDealtFinalBlow", "time": 0, "eventPlayer": "p1" }]
    }"#;
    let trace = run(SLOT_OF_NUMBER, scenario);
    assert!(matches!(trace.outcome, Outcome::NotModeled(_)));
}

#[test]
fn the_slot_field_is_validated() {
    let scenario_error = |json: &str| match run_scenario(SLOT_OF, json) {
        Err(RunError::Scenario(error)) => error,
        other => panic!("expected a scenario refusal, got {other:?}"),
    };
    // `slot` past 11 is malformed, as is a non-integer.
    for slot in ["12", "1.5", "\"3\""] {
        let json = format!(
            r#"{{ "scenarioVersion": 1, "seconds": 1,
                 "players": [{{ "id": "p1", "slot": {slot} }}] }}"#
        );
        assert!(
            matches!(scenario_error(&json), ScenarioError::Format(_)),
            "slot {slot}: expected a format refusal"
        );
    }
    // Two players may not share a slot.
    assert!(matches!(
        scenario_error(
            r#"{ "scenarioVersion": 1, "seconds": 1,
                "players": [{ "id": "p1", "slot": 3 }, { "id": "p2", "slot": 3 }] }"#
        ),
        ScenarioError::Load(_)
    ));
    // The bounds are inclusive and version 2 takes the field too.
    for json in [
        r#"{ "scenarioVersion": 1, "seconds": 1,
            "players": [{ "id": "p1", "slot": 0 }, { "id": "p2", "slot": 11 }] }"#,
        r#"{ "scenarioVersion": 2, "seconds": 1,
            "players": [{ "id": "p1", "slot": 7 }] }"#,
    ] {
        run_scenario(SLOT_OF, json).unwrap_or_else(|error| panic!("{json}: {error}"));
    }
}
