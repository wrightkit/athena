//! `Number Of Players(team)` counting the scenario roster (athena#56):
//! the players on `team` — every player for `All Teams` — the same
//! roster `All Players` enumerates, no action changing it. The `team`
//! argument resolves to a catalog `Team` member id the way `All
//! Players`' does — an enum literal or an evaluated member-id text;
//! anything else is `not_modeled` naming the argument.

use athena_sim::{Outcome, Val, run_scenario};

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

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

const ROSTER: &str = r#"{ "scenarioVersion": 1, "seconds": 1,
    "players": [
        { "id": "p1", "hero": "ana", "team": "team1", "host": true },
        { "id": "p2", "hero": "reaper", "team": "team2" },
        { "id": "p3", "hero": "ana", "team": "team1" }
    ] }"#;

#[test]
fn the_count_of_players_on_the_team() {
    for (team, count) in [("Team 1", 2.0), ("Team 2", 1.0), ("All Teams", 3.0)] {
        let trace = run(&set_global(&format!("Number Of Players({team})")), ROSTER);
        assert_eq!(trace.outcome, Outcome::Complete, "{team}");
        assert_eq!(trace.globals["g"], Val::Number(count), "{team}");
    }
}

/// A team nobody is on counts zero, and so does an empty roster — the
/// `Number Of Players(team) == Null` check Bastion makes.
#[test]
fn an_empty_team_or_roster_counts_zero() {
    let roster = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "team": "team1" }] }"#;
    for (value, scenario) in [
        ("Number Of Players(Team 2)", roster),
        (
            "Number Of Players(Team 1)",
            r#"{ "scenarioVersion": 1, "seconds": 1, "players": [] }"#,
        ),
        (
            "Number Of Players(All Teams)",
            r#"{ "scenarioVersion": 1, "seconds": 1, "players": [] }"#,
        ),
    ] {
        let trace = run(&set_global(value), scenario);
        assert_eq!(trace.outcome, Outcome::Complete, "{value}");
        assert_eq!(trace.globals["g"], Val::Number(0.0), "{value}");

        let trace = run(
            &set_global(&format!("Compare({value}, ==, Null)")),
            scenario,
        );
        assert_eq!(trace.globals["g"], Val::Bool(true), "{value} == Null");
    }
}

/// A player that declares no `team` counts under `All Teams` alone —
/// the same filter `All Players` applies, no field read to miss.
#[test]
fn a_player_without_a_team_counts_only_for_all_teams() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper" }
        ] }"#;
    for (team, count) in [("Team 1", 1.0), ("Team 2", 0.0), ("All Teams", 2.0)] {
        let trace = run(&set_global(&format!("Number Of Players({team})")), scenario);
        assert_eq!(trace.globals["g"], Val::Number(count), "{team}");
    }
}

/// The team argument evaluates: the member-id text `Team Of` yields
/// names the same catalog member the `Team` literal names.
#[test]
fn an_evaluated_member_id_names_the_team() {
    let trace = run(
        &set_global("Number Of Players(Team Of(Host Player))"),
        ROSTER,
    );
    assert_eq!(trace.globals["g"], Val::Number(2.0));
}

/// An argument that is not a catalog `Team` member after evaluation is
/// `not_modeled` — a non-text value, or a text no `Team` member is
/// spelled — the rule `All Players` follows.
#[test]
fn a_non_member_argument_is_not_modeled() {
    for value in [
        "Number Of Players(1)",
        "Number Of Players(Custom String(\"team1\"))",
        "Number Of Players(Hero(Ana))",
    ] {
        let trace = run(&set_global(value), ROSTER);
        let Outcome::NotModeled(stop) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
        assert!(
            stop.construct.contains("numberOfPlayers"),
            "{value}: {}",
            stop.construct
        );
    }
}
