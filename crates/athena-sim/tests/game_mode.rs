//! `Current Game Mode` reading the scenario's `gameMode` field
//! (athena#105): the field names a catalog `Gamemode` member in the
//! form `map` accepts — the member id or an en-US spelling,
//! case-insensitive — and `Current Game Mode` yields its member id as
//! text, so it compares equal to a `Game Mode` literal. A run without
//! the field ends `missing_input` naming `gameMode`; an unknown name
//! fails the load.

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

fn run(text: &str, game_mode: &str) -> athena_sim::Trace {
    let scenario = format!(
        r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], "gameMode": {game_mode} }}"#
    );
    run_scenario(text, &scenario).unwrap_or_else(|error| panic!("{error}"))
}

/// The member id comes back whatever spelling the scenario used — the
/// canonical `Gamemode` member id, an en-US spelling, or either in
/// another case.
#[test]
fn the_supplied_game_mode_is_returned_as_its_member_id() {
    let text = set_global("Current Game Mode");
    for game_mode in [r#""SKIRMISH""#, r#""Skirmish""#, r#""skirmish""#] {
        let trace = run(&text, game_mode);
        assert_eq!(trace.outcome, Outcome::Complete, "{game_mode}");
        assert_eq!(
            trace.globals["g"],
            Val::Text("SKIRMISH".into()),
            "{game_mode}"
        );
    }
}

/// A member id is the same text a `Game Mode` literal yields, so the
/// two compare equal — the comparison Bastion's rule 188 makes.
#[test]
fn current_game_mode_compares_equal_to_a_game_mode_literal() {
    let text = set_global("Compare(Current Game Mode, !=, Game Mode(Skirmish))");
    let trace = run(&text, r#""Skirmish""#);
    assert_eq!(trace.globals["g"], Val::Bool(false));

    let text = set_global("Compare(Current Game Mode, ==, Game Mode(Skirmish))");
    let trace = run(&text, r#""Skirmish""#);
    assert_eq!(trace.globals["g"], Val::Bool(true));
}

#[test]
fn without_the_field_a_current_game_mode_read_is_a_missing_input() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1, "players": [] }"#;
    let text = set_global("Current Game Mode");
    let trace = run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::GameMode);
    assert_eq!(input.name.as_deref(), Some("gameMode"));
    assert_eq!(input.tick, Some(0));

    // A run without a scenario reports it the same way.
    let trace = run_text(&text, athena_sim::Options { seconds: 1.0 })
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::GameMode);
}

/// `gameMode` names a catalog `Gamemode` member, like `map` names a
/// `Map` one: an unknown name is a load error and an empty one is
/// malformed.
#[test]
fn an_unknown_or_empty_game_mode_fails_the_load() {
    let text = set_global("Current Game Mode");
    for game_mode in [r#""Mystery""#, r#""Skirmish2""#] {
        let scenario = format!(
            r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], "gameMode": {game_mode} }}"#
        );
        match run_scenario(&text, &scenario) {
            Err(RunError::Scenario(ScenarioError::Load(_))) => {}
            other => panic!("gameMode {game_mode}: {other:?}"),
        }
    }
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1, "players": [], "gameMode": "" }"#;
    match run_scenario(&text, scenario) {
        Err(RunError::Scenario(ScenarioError::Format(_))) => {}
        other => panic!("empty gameMode: {other:?}"),
    }
}
