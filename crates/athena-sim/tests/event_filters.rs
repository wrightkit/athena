//! Rule event `team`/`player` filters and `Ongoing - Each Player`
//! instantiation (athena#46).

use athena_sim::{ExpectationStatus, MissingInputKind, Outcome, Val, run_scenario};

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

/// `Ongoing - Each Player` writing the event player's team back to it.
const EACH_PLAYER: &str = r#"
variables {
    player:
        0: marker
}

rule ("each") {
    event {
        Ongoing - Each Player;
        Team 2;
        All;
    }
    actions {
        Set Player Variable(Event Player, marker, Team Of(Event Player));
    }
}
"#;

#[test]
fn an_each_player_rule_instantiates_once_per_matching_player() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "Reaper", "team": "team2" }
        ],
        "expect": [
            { "kind": "playerVariable", "player": "p1", "name": "marker", "value": null },
            { "kind": "playerVariable", "player": "p2", "name": "marker", "value": "team2" }
        ]
    }"#;
    let trace = run(EACH_PLAYER, scenario);
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

/// An each-player rule filtered by hero name, and a filtered `Player Died`
/// rule that the scenario never triggers.
const FILTERED_RULES: &str = r#"
variables {
    global:
        0: hits
    player:
        0: marker
}

rule ("heroes") {
    event {
        Ongoing - Each Player;
        All;
        Bastion;
    }
    actions {
        Set Player Variable(Event Player, marker, 1);
    }
}

rule ("died") {
    event {
        Player Died;
        Team 2;
        All;
    }
    actions {
        Modify Global Variable(hits, Add, 1);
    }
}
"#;

#[test]
fn a_hero_filter_binds_the_matching_hero_player() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "dva", "team": "team2" },
            { "id": "p3", "hero": "Bastion", "team": "team2" }
        ],
        "expect": [
            { "kind": "playerVariable", "player": "p1", "name": "marker", "value": null },
            { "kind": "playerVariable", "player": "p2", "name": "marker", "value": null },
            { "kind": "playerVariable", "player": "p3", "name": "marker", "value": 1 }
        ]
    }"#;
    let trace = run(FILTERED_RULES, scenario);
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

#[test]
fn a_rule_filtered_to_an_empty_team_never_fires_and_does_not_stop_the_run() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "team": "team1" }],
        "expect": [{ "kind": "global", "name": "hits", "value": null }]
    }"#;
    let trace = run(FILTERED_RULES, scenario);
    // Nobody matches `Team 2` or `Bastion`: neither rule fires and nothing
    // is reported.
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(
        trace
            .expectations
            .iter()
            .all(|result| result.status == ExpectationStatus::Pass),
        "{:?}",
        trace.expectations
    );
    assert_eq!(trace.globals["hits"], Val::Null);
}

/// A `Player Died` rule filtered to `Team 2`.
const TEAM_DIED: &str = r#"
variables {
    global:
        0: hits
    player:
        0: dead
}

rule ("died") {
    event {
        Player Died;
        Team 2;
        All;
    }
    actions {
        Modify Global Variable(hits, Add, 1);
        Set Player Variable(Event Player, dead, true);
    }
}
"#;

#[test]
fn a_filtered_event_rule_runs_only_for_a_matching_event_player() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "Reaper", "team": "team2" }
        ],
        "events": [
            { "kind": "playerDied", "time": 0.5, "eventPlayer": "p1" },
            { "kind": "playerDied", "time": 0.5, "eventPlayer": "p2" }
        ],
        "expect": [
            { "kind": "global", "name": "hits", "value": 1 },
            { "kind": "playerVariable", "player": "p2", "name": "dead", "value": true },
            { "kind": "playerVariable", "player": "p1", "name": "dead", "value": null }
        ]
    }"#;
    let trace = run(TEAM_DIED, scenario);
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

#[test]
fn a_filtered_event_rule_reads_the_event_player_to_match() {
    // The event names no `eventPlayer`, so a filtered rule cannot tell
    // whether it applies: the filter reads a group 2 input.
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "team": "team1" }],
        "events": [{ "kind": "playerDied", "time": 0.5 }]
    }"#;
    let trace = run(TEAM_DIED, scenario);
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [missing] = trace.missing_inputs.as_slice() else {
        panic!("expected one missing input, got {:?}", trace.missing_inputs);
    };
    assert_eq!(missing.input, MissingInputKind::EventPlayer);
    assert_eq!(missing.tick, Some(31));
}

#[test]
fn a_slot_target_filter_is_not_modeled() {
    let text = r#"
rule ("slotted") {
    event {
        Ongoing - Each Player;
        All;
        Slot 3;
    }
    actions {
        Wait(0, Ignore Condition);
    }
}
"#;
    let trace = run(
        text,
        r#"{ "scenarioVersion": 1, "seconds": 1,
            "players": [{ "id": "p1", "hero": "ana", "team": "team1" }] }"#,
    );
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(not_modeled.construct.contains("slot"));
    assert_eq!(not_modeled.tick, None);
}
