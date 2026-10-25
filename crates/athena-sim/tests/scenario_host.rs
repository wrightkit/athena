//! `Host Player`, `Has Spawned` and `Wait Until` reading scenario players, as
//! `docs/scenario-format.md` defines them.

use athena_sim::{Outcome, RunError, ScenarioError, Val, run_scenario};

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

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

const HOST_SCENARIO: &str = r#"{ "scenarioVersion": 1, "seconds": 1,
    "players": [{ "id": "p1", "hero": "ana" }, { "id": "p2", "hero": "ana", "host": true }] }"#;

#[test]
fn host_player_is_the_marked_player() {
    let text = global_rule(
        r#"Global.x = Host Player; Log To Inspector(Custom String("{0}", Hero Of(Host Player), Null, Null));"#,
    );
    let trace = run(&text, HOST_SCENARIO);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["x"], Val::Player("p2".into()));
}

#[test]
fn host_player_without_a_host_is_not_modeled() {
    let text = global_rule("Global.x = Host Player;");
    let trace = run(
        &text,
        r#"{ "scenarioVersion": 1, "seconds": 1, "players": [{ "id": "p1", "hero": "ana" }] }"#,
    );
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(not_modeled.construct.contains("Host Player"));
}

#[test]
fn two_hosts_fail_to_load() {
    let text = global_rule("Global.x = 1;");
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "host": true }, { "id": "p2", "host": true }] }"#;
    assert!(matches!(
        run_scenario(&text, scenario),
        Err(RunError::Scenario(ScenarioError::Load(_)))
    ));
}

#[test]
fn has_spawned_needs_a_hero() {
    let text = global_rule("Global.x = Has Spawned(Host Player);");
    let trace = run(&text, HOST_SCENARIO);
    assert_eq!(trace.globals["x"], Val::Bool(true));
    let trace = run(
        &text,
        r#"{ "scenarioVersion": 1, "seconds": 1, "players": [{ "id": "p1", "host": true }] }"#,
    );
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(not_modeled.construct.contains("hasSpawned"));
}

#[test]
fn wait_until_on_an_unmodeled_value_is_not_modeled() {
    let text = global_rule(
        "Wait Until(Is Alive(Host Player) && Is Button Held(Host Player, Ability 1), 1);",
    );
    let trace = run(&text, HOST_SCENARIO);
    assert!(matches!(trace.outcome, Outcome::NotModeled(_)));
}
