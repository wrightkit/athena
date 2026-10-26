//! `Is Hero Being Played(hero, team)` reading the scenario players'
//! declared `hero` (athena#84): true when at least one player on the
//! team — every player for `All Teams` — has the hero as its declared
//! `hero`. Hero and team arguments resolve to catalog member ids, the
//! same form `Hero Of`/`Team Of` yield; an argument evaluating to
//! anything else is `not_modeled` naming the parameter position, and a
//! player on the team without a `hero` is a `missing_input`. The model
//! makes no claim about the hero-selection screen.

use athena_sim::{MissingInputKind, Outcome, RunEnd, Val, run_scenario};

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

const ANA_REAPER: &str = r#"{ "scenarioVersion": 1, "seconds": 1,
    "players": [
        { "id": "p1", "hero": "ana", "team": "team1", "host": true },
        { "id": "p2", "hero": "reaper", "team": "team2" }
    ] }"#;

#[test]
fn true_when_a_player_on_the_team_has_the_hero() {
    for team in ["Team 1", "All Teams"] {
        let trace = run(
            &set_global(&format!("Is Hero Being Played(Hero(Ana), {team})")),
            ANA_REAPER,
        );
        assert_eq!(trace.outcome, Outcome::Complete, "{team}");
        assert_eq!(trace.globals["g"], Val::Bool(true), "{team}");
    }
}

#[test]
fn false_when_no_player_on_the_team_has_the_hero() {
    for (hero, team) in [
        ("Ashe", "Team 1"),
        ("Ana", "Team 2"),
        ("Reaper", "Team 1"),
        ("Zenyatta", "All Teams"),
    ] {
        let trace = run(
            &set_global(&format!("Is Hero Being Played(Hero({hero}), {team})")),
            ANA_REAPER,
        );
        assert_eq!(trace.globals["g"], Val::Bool(false), "{hero} on {team}");
    }
}

/// The hero argument evaluates: the member-id text `Hero Of` yields
/// names the same catalog member the `Hero` literal names.
#[test]
fn an_evaluated_member_id_names_the_hero() {
    let trace = run(
        &set_global("Is Hero Being Played(Hero Of(Host Player), Team 1)"),
        ANA_REAPER,
    );
    assert_eq!(trace.globals["g"], Val::Bool(true));
}

/// A player on the team without a `hero` is the `hero` missing input —
/// the same rule `Hero Of` follows; a player not on the team is never
/// read.
#[test]
fn a_player_without_a_hero_is_a_missing_input() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "team": "team1" }] }"#;
    let trace = run(
        &set_global("Is Hero Being Played(Hero(Ana), Team 1)"),
        scenario,
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Hero);
    assert_eq!(input.player.as_deref(), Some("p1"));
    assert_eq!(input.tick, Some(0));

    // The same player under `All Teams` is read too.
    let trace = run(
        &set_global("Is Hero Being Played(Hero(Ana), All Teams)"),
        scenario,
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);

    // Under the other team the player is never read, so the value is
    // false rather than a missing input.
    let trace = run(
        &set_global("Is Hero Being Played(Hero(Ana), Team 2)"),
        scenario,
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["g"], Val::Bool(false));
}

/// An argument that is not a catalog member after evaluation is
/// `not_modeled` — a non-text value, or a text no `Hero`/`Team` member
/// is spelled — and the message names the parameter position.
#[test]
fn a_non_member_argument_is_not_modeled() {
    for (value, param) in [
        ("Is Hero Being Played(1, Team 1)", "hero"),
        (
            "Is Hero Being Played(Custom String(\"Ana\"), Team 1)",
            "hero",
        ),
        ("Is Hero Being Played(Hero(Ana), 1)", "team"),
        ("Is Hero Being Played(Hero(Ana), Hero(Mercy))", "team"),
    ] {
        let trace = run(&set_global(value), ANA_REAPER);
        let Outcome::NotModeled(stop) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
        assert!(
            stop.construct.contains("isHeroBeingPlayed") && stop.construct.contains(param),
            "{value}: {}",
            stop.construct
        );
    }
}

/// `Wait Until` re-checks the value on every tick; a scenario event
/// that changes nothing about the players leaves it stable, so the
/// wait runs to its timeout.
#[test]
fn wait_until_rechecks_the_value_while_an_event_fires() {
    let text = r#"
variables {
    global:
        0: g
        1: ev
}

rule ("wait") {
    event {
        Ongoing - Global;
    }
    actions {
        Wait Until(Not(Is Hero Being Played(Hero(Ana), Team 1)), 0.1);
        Set Global Variable(g, 1);
    }
}

rule ("event") {
    event {
        Player Dealt Final Blow;
        All;
        All;
    }
    actions {
        Set Global Variable(ev, 1);
    }
}
"#;
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "team": "team1" }],
        "events": [{ "kind": "playerDealtFinalBlow", "time": 0.03, "eventPlayer": "p1" }] }"#;
    let trace = run(text, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    // The event fired mid-wait on tick 1; the wait re-checked the value
    // there and held it true, so it suspended to its timeout — tick 7,
    // `0.1` s rounded up — and the actions after it ran.
    assert_eq!(trace.globals["ev"], Val::Number(1.0));
    assert_eq!(trace.globals["g"], Val::Number(1.0));
    let [started] = trace.timeline.entries[0].rules.as_slice() else {
        panic!("{:?}", trace.timeline.entries[0].rules);
    };
    assert_eq!(started.ended, RunEnd::Waiting { wake: 7 });
}
