//! `Players On Hero(hero, team)` reading the scenario players' declared
//! `hero` (athena#86): the players on `team` — every player for `All
//! Teams` — whose declared `hero` is `hero`, in roster order. Hero and
//! team arguments resolve to catalog member ids, the same form `Hero
//! Of`/`Team Of` yield; an argument evaluating to anything else is
//! `not_modeled` naming the parameter position, and a player on the team
//! without a `hero` is a `missing_input`. The model makes no claim about
//! the hero-selection screen.

use athena_sim::{MissingInputKind, Outcome, Val, run_scenario};

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

fn players(ids: &[&str]) -> Val {
    Val::Array(
        ids.iter()
            .map(|id| Val::Player((*id).to_string()))
            .collect(),
    )
}

#[test]
fn the_players_on_the_team_with_the_hero_in_roster_order() {
    for (hero, team, ids) in [
        ("Ana", "Team 1", &["p1", "p3"][..]),
        ("Reaper", "Team 2", &["p2"][..]),
        ("Ana", "All Teams", &["p1", "p3"][..]),
        ("Reaper", "All Teams", &["p2"][..]),
    ] {
        let trace = run(
            &set_global(&format!("Players On Hero(Hero({hero}), {team})")),
            ROSTER,
        );
        assert_eq!(trace.outcome, Outcome::Complete, "{hero} on {team}");
        assert_eq!(trace.globals["g"], players(ids), "{hero} on {team}");
    }
}

#[test]
fn no_matching_player_is_the_empty_array() {
    for (hero, team) in [
        // No player plays the hero at all.
        ("Ashe", "Team 1"),
        ("Ashe", "All Teams"),
        // The hero is played, but on the other team.
        ("Reaper", "Team 1"),
        ("Ana", "Team 2"),
    ] {
        let trace = run(
            &set_global(&format!("Count Of(Players On Hero(Hero({hero}), {team}))")),
            ROSTER,
        );
        assert_eq!(trace.outcome, Outcome::Complete, "{hero} on {team}");
        assert_eq!(trace.globals["g"], Val::number(0.0), "{hero} on {team}");
    }
}

/// The hero argument evaluates: the member-id text `Hero Of` yields
/// names the same catalog member the `Hero` literal names.
#[test]
fn an_evaluated_member_id_names_the_hero() {
    let trace = run(
        &set_global("Players On Hero(Hero Of(Host Player), Team 1)"),
        ROSTER,
    );
    assert_eq!(trace.globals["g"], players(&["p1", "p3"]));
}

/// A player on the team without a `hero` is the `hero` missing input —
/// the same rule `Hero Of` follows; a player not on the team is never
/// read.
#[test]
fn a_player_without_a_hero_is_a_missing_input() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "team": "team1" }] }"#;
    let trace = run(&set_global("Players On Hero(Hero(Ana), Team 1)"), scenario);
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Hero);
    assert_eq!(input.player.as_deref(), Some("p1"));
    assert_eq!(input.tick, Some(0));

    // The same player under `All Teams` is read too.
    let trace = run(
        &set_global("Players On Hero(Hero(Ana), All Teams)"),
        scenario,
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);

    // Under the other team the player is never read, so the value is
    // the empty array rather than a missing input.
    let trace = run(&set_global("Players On Hero(Hero(Ana), Team 2)"), scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["g"], players(&[]));
}

/// An argument that is not a catalog member after evaluation is
/// `not_modeled` — a non-text value, or a text no `Hero`/`Team` member
/// is spelled — and the message names the parameter position.
#[test]
fn a_non_member_argument_is_not_modeled() {
    for (value, param) in [
        ("Players On Hero(1, Team 1)", "hero"),
        ("Players On Hero(Custom String(\"Ana\"), Team 1)", "hero"),
        ("Players On Hero(Hero(Ana), 1)", "team"),
        ("Players On Hero(Hero(Ana), Hero(Mercy))", "team"),
    ] {
        let trace = run(&set_global(value), ROSTER);
        let Outcome::NotModeled(stop) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
        assert!(
            stop.construct.contains("getPlayersOnHero") && stop.construct.contains(param),
            "{value}: {}",
            stop.construct
        );
    }
}

/// The result is a player array the array values read: `Count Of` its
/// length, `Filtered Array` over its elements, `Array Contains` its
/// membership.
#[test]
fn the_result_feeds_the_array_values() {
    let trace = run(
        &set_global("Count Of(Players On Hero(Hero(Ana), Team 1))"),
        ROSTER,
    );
    assert_eq!(trace.globals["g"], Val::number(2.0));

    let trace = run(
        &set_global(
            "Filtered Array(Players On Hero(Hero(Ana), All Teams), Current Array Element == Host Player)",
        ),
        ROSTER,
    );
    assert_eq!(trace.globals["g"], players(&["p1"]));

    for (value, contains) in [
        (
            "Array Contains(Players On Hero(Hero(Ana), Team 1), Host Player)",
            true,
        ),
        (
            "Array Contains(Players On Hero(Hero(Reaper), Team 1), Host Player)",
            false,
        ),
    ] {
        let trace = run(&set_global(value), ROSTER);
        assert_eq!(trace.outcome, Outcome::Complete, "{value}");
        assert_eq!(trace.globals["g"], Val::Bool(contains), "{value}");
    }
}
