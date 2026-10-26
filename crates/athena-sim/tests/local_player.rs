//! `Local Player` reading the scenario player marked `local`
//! (athena#105): the client binds `Local Player` to the end user's
//! player, which the simulator cannot derive, so the scenario marks it.
//! A scenario without a `local` player ends the read `missing_input`
//! naming `local`; two marked players fail the load.

use athena_sim::{MissingInputKind, Outcome, RunError, ScenarioError, Val, run_scenario};

/// One `Ongoing - Global` rule that writes `g` once at tick 0.
fn global_rule(actions: &str) -> String {
    format!(
        r#"
variables {{
    global:
        0: x
}}

rule("r") {{
    event {{
        Ongoing - Global;
    }}
    actions {{
        {actions}
    }}
}}
"#
    )
}

#[test]
fn local_player_is_the_marked_player() {
    let text = global_rule("Global.x = Local Player;");
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana" }, { "id": "p2", "hero": "ana", "local": true }] }"#;
    let trace = run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["x"], Val::Player("p2".into()));
}

#[test]
fn local_player_without_a_mark_is_a_missing_input() {
    let text = global_rule("Global.x = Local Player;");
    let scenario =
        r#"{ "scenarioVersion": 1, "seconds": 1, "players": [{ "id": "p1", "hero": "ana" }] }"#;
    let trace = run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Local);
    assert_eq!(input.name.as_deref(), Some("local"));
    assert_eq!(input.tick, Some(0));
}

#[test]
fn two_local_players_fail_to_load() {
    let text = global_rule("Global.x = 1;");
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "local": true }, { "id": "p2", "local": true }] }"#;
    assert!(matches!(
        run_scenario(&text, scenario),
        Err(RunError::Scenario(ScenarioError::Load(_)))
    ));
}
