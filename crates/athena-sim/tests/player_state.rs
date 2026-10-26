//! The per-player `state` map (athena#89): the no-extra-argument player
//! reads answer from the scenario player's `state` object, keyed by the
//! state keys `docs/scenario.schema.json` lists. A key the program
//! reads but the map does not list is a `missing_input` naming
//! `state.<key>`; an unknown key or a wrong JSON type is malformed.

use athena_sim::{MissingInputKind, Outcome, RunError, ScenarioError, Val, run_scenario};

/// The scenario-format message a malformed scenario reports.
fn format_error(error: RunError) -> String {
    let RunError::Scenario(ScenarioError::Format(message)) = error else {
        panic!("expected a format error, got {error:?}");
    };
    message
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

const PROGRAM_VARIABLE: &str = r#"
variables {
    player:
        0: streak
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(g, Player Variable(Host Player, streak));
    }
}
"#;

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

fn global(trace: &athena_sim::Trace) -> &Val {
    assert_eq!(trace.outcome, Outcome::Complete);
    &trace.globals["g"]
}

/// The `value`-spelled name of each table row and the `state` entry it
/// answers with: (Workshop spelling, catalog value id, state key,
/// JSON type, a scenario JSON value, the `Val` the read yields).
const READS: &[(&str, &str, &str, &str, &str)] = &[
    ("Health", "getHealth", "health", "number", "175"),
    ("Max Health", "getMaxHealth", "maxHealth", "number", "200"),
    (
        "Normalized Health",
        "getNormalizedHealth",
        "normalizedHealth",
        "number",
        "0.5",
    ),
    (
        "Velocity Of",
        "getVelocity",
        "velocity",
        "vector",
        r#"{ "x": 3, "y": 4, "z": 0 }"#,
    ),
    // `Speed Of` derives the length of `velocity`; it reads no key of
    // its own.
    ("Speed Of", "getSpeed", "velocity", "derived", "5"),
    (
        "Eye Position",
        "getEyePosition",
        "eyePosition",
        "vector",
        r#"{ "x": 0, "y": 1.5, "z": 0 }"#,
    ),
    (
        "Ultimate Charge Percent",
        "getUltCharge",
        "ultCharge",
        "number",
        "40",
    ),
    ("Is Dead", "isDead", "isDead", "boolean", "false"),
    (
        "Is Using Ultimate",
        "isUsingUltimate",
        "isUsingUltimate",
        "boolean",
        "true",
    ),
    ("Is Meleeing", "isMeleeing", "isMeleeing", "boolean", "true"),
    (
        "Is Firing Secondary",
        "isFiringSecondary",
        "isFiringSecondary",
        "boolean",
        "true",
    ),
    (
        "Is In Alternate Form",
        "isInAlternateForm",
        "isInAlternateForm",
        "boolean",
        "true",
    ),
    (
        "Is Communicating Any Emote",
        "isCommunicatingEmote",
        "isCommunicatingEmote",
        "boolean",
        "true",
    ),
    (
        "Is In Spawn Room",
        "isInSpawnRoom",
        "isInSpawnRoom",
        "boolean",
        "true",
    ),
    ("Is Dummy Bot", "isDummy", "isDummy", "boolean", "false"),
    ("Is Moving", "isMoving", "isMoving", "boolean", "true"),
];

/// A scenario v2 player whose `state` holds every declared key.
fn player(state: &str) -> String {
    format!(r#"{{ "id": "p1", "host": true, "hero": "ana", "state": {{ {state} }} }}"#)
}

fn full_state() -> String {
    READS
        .iter()
        .filter(|(_, _, _, ty, _)| *ty != "derived")
        .map(|(_, _, key, _, value)| format!(r#""{key}": {value}"#))
        .collect::<Vec<_>>()
        .join(", ")
}

fn scenario(players: &str) -> String {
    format!(r#"{{ "scenarioVersion": 2, "seconds": 1, "players": [{players}] }}"#)
}

#[test]
fn the_table_drives_every_listed_read() {
    let scenario = scenario(&player(&full_state()));
    for &(spelling, id, key, ty, value) in READS {
        let trace = run(&set_global(&format!("{spelling}(Host Player)")), &scenario);
        let expected = match (ty, key) {
            ("vector", "velocity") => Val::Vector {
                x: 3.0,
                y: 4.0,
                z: 0.0,
            },
            ("vector", "eyePosition") => Val::Vector {
                x: 0.0,
                y: 1.5,
                z: 0.0,
            },
            ("boolean", _) => Val::Bool(value.parse::<bool>().unwrap()),
            _ => Val::Number(value.parse::<f64>().unwrap()),
        };
        assert_eq!(global(&trace), &expected, "{id}");
    }
}

#[test]
fn boolean_number_and_vector_reads_come_from_state() {
    let trace = run(
        &set_global("Is Moving(Host Player)"),
        &scenario(&player(r#""isMoving": true"#)),
    );
    assert_eq!(global(&trace), &Val::Bool(true));
    let trace = run(
        &set_global("Health(Host Player)"),
        &scenario(&player(r#""health": 175"#)),
    );
    assert_eq!(global(&trace), &Val::Number(175.0));
    let trace = run(
        &set_global("Velocity Of(Host Player)"),
        &scenario(&player(r#""velocity": { "x": 1, "y": 2, "z": 3 }"#)),
    );
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 1.0,
            y: 2.0,
            z: 3.0
        }
    );
}

#[test]
fn speed_of_is_the_length_of_velocity() {
    let trace = run(
        &set_global("Speed Of(Host Player)"),
        &scenario(&player(r#""velocity": { "x": 3, "y": 4, "z": 0 }"#)),
    );
    assert_eq!(global(&trace), &Val::Number(5.0));
}

#[test]
fn a_missing_state_key_is_a_missing_input() {
    let trace = run(
        &set_global("Is Moving(Host Player)"),
        &scenario(&player(r#""health": 200"#)),
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [missing] = trace.missing_inputs.as_slice() else {
        panic!("expected one missing input, got {:?}", trace.missing_inputs);
    };
    assert_eq!(missing.input, MissingInputKind::State);
    assert_eq!(missing.player.as_deref(), Some("p1"));
    assert_eq!(missing.name.as_deref(), Some("state.isMoving"));
    assert_eq!(missing.construct, "value isMoving");
}

#[test]
fn every_absent_key_is_a_missing_input_under_a_declared_state() {
    // Every declared key but one: each read whose key was removed
    // stops `missing_input` naming `state.<key>` — `Speed Of` shares
    // `velocity` with `Velocity Of`.
    for key in READS
        .iter()
        .map(|(_, _, key, _, _)| *key)
        .collect::<std::collections::BTreeSet<_>>()
    {
        let state = READS
            .iter()
            .filter(|(_, _, k, ty, _)| *ty != "derived" && *k != key)
            .map(|(_, _, k, _, value)| format!(r#""{k}": {value}"#))
            .collect::<Vec<_>>()
            .join(", ");
        let scenario = scenario(&player(&state));
        for (spelling, _, _, _, _) in READS.iter().filter(|(_, _, k, _, _)| *k == key) {
            let trace = run(&set_global(&format!("{spelling}(Host Player)")), &scenario);
            assert_eq!(trace.outcome, Outcome::MissingInput, "{spelling}");
            let missing = &trace.missing_inputs[0];
            assert_eq!(missing.input, MissingInputKind::State, "{spelling}");
            assert_eq!(missing.player.as_deref(), Some("p1"), "{spelling}");
            assert_eq!(
                missing.name.as_deref(),
                Some(format!("state.{key}").as_str()),
                "{spelling}"
            );
        }
    }
}

#[test]
fn a_declared_empty_state_map_is_a_missing_input() {
    let trace = run(
        &set_global("Is Moving(Host Player)"),
        &scenario(&player("")),
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(
        trace.missing_inputs[0].name.as_deref(),
        Some("state.isMoving")
    );
}

#[test]
fn an_absent_state_field_keeps_the_established_models() {
    // No `state` field: `Health` reads the resolved pools and
    // `Is In Spawn Room` the empty world; a read with no model is
    // still a missing input.
    let scenario = scenario(r#"{ "id": "p1", "host": true, "hero": "ana" }"#);
    let trace = run(&set_global("Is In Spawn Room(Host Player)"), &scenario);
    assert_eq!(global(&trace), &Val::Bool(false));
    let values = workshop_rs::hero_values::load(
        r#"{
            "format": "workshop-rs/hero-values-v1",
            "balanceVersion": "test-only",
            "heroes": [
                { "id": "ana", "values": [
                    { "kind": "health", "pool": "health", "amount": 250 },
                    { "kind": "cooldown", "slot": "ability1", "seconds": 14 },
                    { "kind": "ultimateCost", "points": 2300 }
                ] }
            ]
        }"#,
    )
    .expect("the test hero values load");
    let trace = athena_sim::run_scenario_with_hero_values(
        &set_global("Health(Host Player)"),
        &scenario,
        &values,
    )
    .expect("the run resolves");
    assert_eq!(global(&trace), &Val::Number(250.0));
    let trace = run(&set_global("Is Moving(Host Player)"), &scenario);
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(
        trace.missing_inputs[0].name.as_deref(),
        Some("state.isMoving")
    );
}

#[test]
fn speed_of_reports_velocity_as_its_missing_input() {
    let trace = run(&set_global("Speed Of(Host Player)"), &scenario(&player("")));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let missing = &trace.missing_inputs[0];
    assert_eq!(missing.input, MissingInputKind::State);
    assert_eq!(missing.name.as_deref(), Some("state.velocity"));
}

#[test]
fn a_wrong_type_state_value_is_malformed() {
    for (key, value, ty) in [
        ("isMoving", "1", "a boolean"),
        ("health", "true", "a number"),
        ("velocity", "2", "a vector"),
        ("velocity", r#"{ "x": 1, "y": 2 }"#, "a vector"),
    ] {
        let scenario = scenario(&player(&format!(r#""{key}": {value}"#)));
        let error = run_scenario(&set_global("Null"), &scenario)
            .expect_err("a wrong-typed state value must fail");
        assert_eq!(
            format_error(error),
            format!("player `p1` `state.{key}` must be {ty}")
        );
    }
}

#[test]
fn an_unknown_state_key_is_malformed() {
    let scenario = scenario(&player(r#""speed": 5"#));
    let error =
        run_scenario(&set_global("Null"), &scenario).expect_err("an unknown state key must fail");
    assert_eq!(format_error(error), "player `p1` has unknown `state.speed`");
}

#[test]
fn version_1_state_still_means_variables() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "host": true, "state": { "streak": 2 } }] }"#;
    let trace = run(PROGRAM_VARIABLE, scenario);
    assert_eq!(global(&trace), &Val::Number(2.0));
}

#[test]
fn version_2_reads_variables_from_vars() {
    let scenario = r#"{ "scenarioVersion": 2, "seconds": 1,
        "players": [{ "id": "p1", "host": true, "vars": { "streak": 3 } }] }"#;
    let trace = run(PROGRAM_VARIABLE, scenario);
    assert_eq!(global(&trace), &Val::Number(3.0));
}

#[test]
fn vars_in_version_1_is_malformed() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "vars": { "streak": 3 } }] }"#;
    let error =
        run_scenario(&set_global("Null"), scenario).expect_err("`vars` in version 1 must fail");
    assert_eq!(
        format_error(error),
        "player `p1` has `vars`, new in scenarioVersion 2"
    );
}

/// `state` and `vars` are objects or absent: `null` is malformed, not
/// "absent" — the rule `position`, `facing` and `serverLoad` already
/// follow.
#[test]
fn a_null_state_or_vars_is_malformed() {
    for (version, field) in [(1, "state"), (2, "state"), (2, "vars")] {
        let scenario = format!(
            r#"{{ "scenarioVersion": {version}, "seconds": 1,
                "players": [{{ "id": "p1", "{field}": null }}] }}"#
        );
        match run_scenario(&set_global("Null"), &scenario) {
            Err(RunError::Scenario(ScenarioError::Format(_))) => {}
            other => panic!("version {version} `{field}`: {other:?}"),
        }
    }
}
