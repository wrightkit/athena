//! Player events: scenario `events` trigger the matching rules, and the
//! modeled `Damage`/`Heal` state changes raise the player events the
//! catalog defines for the cause (athena#47).

use athena_sim::{ExpectationStatus, Outcome, Val, run_scenario};

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

/// A `Player Left Match` rule counting its fires and marking the player.
const LEFT: &str = r#"
variables {
    global:
        0: hits
    player:
        0: left
}

rule ("left") {
    event {
        Player Left Match;
        All;
        All;
    }
    actions {
        Modify Global Variable(hits, Add, 1);
        Set Player Variable(Event Player, left, true);
    }
}
"#;

#[test]
fn a_scenario_player_left_event_runs_the_matching_rule_once() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 2,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper", "team": "team2" }
        ],
        "events": [{ "kind": "playerLeft", "time": 0.5, "eventPlayer": "p1" }],
        "expect": [
            { "kind": "playerVariable", "player": "p1", "name": "left", "value": true },
            { "kind": "playerVariable", "player": "p2", "name": "left", "value": null }
        ]
    }"#;
    let trace = run(LEFT, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["hits"], Val::Number(1.0));
    assert!(
        trace
            .expectations
            .iter()
            .all(|result| result.status == ExpectationStatus::Pass),
        "{:?}",
        trace.expectations
    );
    // The rule ran exactly once, for the leaving player.
    let fires: Vec<_> = trace
        .timeline
        .entries
        .iter()
        .flat_map(|entry| entry.rules.iter())
        .filter(|run| run.name == "left")
        .collect();
    assert_eq!(fires.len(), 1);
    assert_eq!(fires[0].event_player.as_deref(), Some("p1"));
}

/// The four damage events record their firing order; the rules are
/// declared out of the occurrence order on purpose — an event reaches
/// its rules when it occurs, not when the tick loop reaches the rule.
const KILL_ORDER: &str = r#"
variables {
    global:
        0: seq
}

rule ("hit") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(Value In Array(All Players(Team 2), 0), Value In Array(All Players(Team 1), 0), 9999);
    }
}

rule ("blow") {
    event {
        Player Dealt Final Blow;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 4);
    }
}

rule ("took") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 1);
    }
}

rule ("died") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 3);
    }
}

rule ("dealt") {
    event {
        Player Dealt Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 2);
    }
}
"#;

#[test]
fn a_killing_damage_runs_died_then_final_blow_in_occurrence_order() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper", "team": "team2" }
        ]
    }"#;
    let trace = run(KILL_ORDER, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    // Took Damage, Dealt Damage, Died, Dealt Final Blow: the occurrence
    // order, even though the rules are filed in another order.
    assert_eq!(
        trace.globals["seq"],
        Val::Array(vec![
            Val::Number(1.0),
            Val::Number(2.0),
            Val::Number(3.0),
            Val::Number(4.0)
        ])
    );
    // The kill wrote the victim's `isDead` cell.
    assert_eq!(trace.players["p2"].state["isDead"], Val::Bool(true));
}

/// The damage events' bindings: `Event Player`, `Attacker`, `Victim`
/// and `Event Damage`.
const BINDINGS: &str = r#"
variables {
    global:
        0: ep
        1: dmg
        2: atk
        3: vic
        4: atkNull
}

rule ("hit") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(Value In Array(All Players(Team 2), 0), Value In Array(All Players(Team 1), 0), 30);
        Damage(Value In Array(All Players(Team 1), 0), Null, 40);
    }
}

rule ("dealt") {
    event {
        Player Dealt Damage;
        All;
        All;
    }
    actions {
        Set Global Variable(ep, Event Player);
        Set Global Variable(dmg, Event Damage);
        Set Global Variable(atk, Hero Of(Attacker));
        Set Global Variable(vic, Hero Of(Victim));
    }
}

rule ("took") {
    event {
        Player Took Damage;
        Team 1;
        All;
    }
    actions {
        Set Global Variable(atkNull, Compare(Attacker, ==, Null));
    }
}
"#;

#[test]
fn damage_events_bind_event_player_attacker_victim_and_event_damage() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper", "team": "team2" }
        ]
    }"#;
    let trace = run(BINDINGS, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    // `Player Dealt Damage` runs for the dealer: `Attacker` is the
    // dealer, `Victim` the player the pool moved on, `Event Damage`
    // the applied amount.
    assert_eq!(trace.globals["ep"], Val::Player("p1".to_string()));
    assert_eq!(trace.globals["dmg"], Val::Number(30.0));
    assert_eq!(trace.globals["atk"], Val::Text("ANA".to_string()));
    assert_eq!(trace.globals["vic"], Val::Text("REAPER".to_string()));
    // A `Null` dealer raises `Player Took Damage` with `Attacker`
    // bound to `Null` — and no `Player Dealt Damage` at all (the dealt
    // rule ran once, for the 30-damage hit).
    assert_eq!(trace.globals["atkNull"], Val::Bool(true));
}

/// `Player Died` filtered to `Team 2`; the killed player is on Team 1.
const FILTERED_DIED: &str = r#"
variables {
    global:
        0: hits
}

rule ("hit") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(Value In Array(All Players(Team 1), 0), Null, 9999);
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
fn a_damage_event_filtered_away_does_not_run() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "team": "team1" }]
    }"#;
    let trace = run(FILTERED_DIED, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["hits"], Val::Null);
}

/// The heal events a `Heal` raises: `Player Received Healing` for the
/// healed, `Player Dealt Healing` for a player healer, `Event Healing`
/// bound to the applied amount.
const HEAL_EVENTS: &str = r#"
variables {
    global:
        0: seq
        1: healing
}

rule ("soften") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(Value In Array(All Players(Team 2), 0), Null, 100);
        Heal(Value In Array(All Players(Team 2), 0), Value In Array(All Players(Team 1), 0), 40);
    }
}

rule ("dealtHealing") {
    event {
        Player Dealt Healing;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 2);
        Set Global Variable(healing, Event Healing);
    }
}

rule ("receivedHealing") {
    event {
        Player Received Healing;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 1);
    }
}
"#;

#[test]
fn a_heal_raises_received_then_dealt_healing() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper", "team": "team2" }
        ]
    }"#;
    let trace = run(HEAL_EVENTS, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    // Received then dealt, even though the dealt rule is filed first.
    assert_eq!(
        trace.globals["seq"],
        Val::Array(vec![Val::Number(1.0), Val::Number(2.0)])
    );
    assert_eq!(trace.globals["healing"], Val::Number(40.0));
}

/// `Kill` writes the `isDead` marker the `Damage`/`Heal` gate reads: a
/// dead target takes no pool change and raises no events, and a
/// `Resurrect`ed target takes them again.
const DEAD_GATE: &str = r#"
variables {
    global:
        0: seq
}

rule ("cause") {
    event {
        Ongoing - Global;
    }
    actions {
        Kill(Value In Array(All Players(Team 2), 0), Null);
        Damage(Value In Array(All Players(Team 2), 0), Null, 9999);
        Heal(Value In Array(All Players(Team 2), 0), Null, 30);
        Resurrect(Value In Array(All Players(Team 2), 0));
        Damage(Value In Array(All Players(Team 2), 0), Null, 60);
        Heal(Value In Array(All Players(Team 2), 0), Null, 30);
    }
}

rule ("took") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 1);
    }
}

rule ("died") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 3);
    }
}

rule ("healed") {
    event {
        Player Received Healing;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 5);
    }
}
"#;

#[test]
fn a_dead_target_takes_no_pool_change_or_events_and_a_revived_one_does() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper", "team": "team2" }
        ]
    }"#;
    let trace = run(DEAD_GATE, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    // The `Kill`-gated `Damage` and `Heal` raised nothing — no second
    // `Player Died` either; the post-`Resurrect` actions raised their
    // events once each.
    assert_eq!(
        trace.globals["seq"],
        Val::Array(vec![Val::Number(1.0), Val::Number(5.0)])
    );
    // The gated actions never touched the pool: max − 60 + 30.
    let world = trace.players["p2"].world.as_ref().unwrap();
    assert_eq!(world.health.current, world.health.max - 30.0);
}

/// A `Damage` kill leaves the health pool at zero where `Kill` does
/// not: `Resurrect` and `Respawn` refill it, so a revived player
/// takes damage and dies — raising `Player Died` — again.
const DAMAGE_REVIVE: &str = r#"
variables {
    global:
        0: seq
}

rule ("cause") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(Value In Array(All Players(Team 2), 0), Null, 9999);
        Resurrect(Value In Array(All Players(Team 2), 0));
        Damage(Value In Array(All Players(Team 2), 0), Null, 9999);
        Respawn(Value In Array(All Players(Team 2), 0));
        Damage(Value In Array(All Players(Team 2), 0), Null, 9999);
        Damage(Value In Array(All Players(Team 2), 0), Null, 9999);
        Heal(Value In Array(All Players(Team 2), 0), Null, 30);
    }
}

rule ("took") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 1);
    }
}

rule ("died") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 3);
    }
}

rule ("healed") {
    event {
        Player Received Healing;
        All;
        All;
    }
    actions {
        Modify Global Variable(seq, Append To Array, 5);
    }
}
"#;

#[test]
fn a_damage_killed_then_revived_player_dies_again() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper", "team": "team2" }
        ]
    }"#;
    let trace = run(DAMAGE_REVIVE, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    // `Player Died` fires once per revive-and-kill cycle — after the
    // `Resurrect` and after the `Respawn`; the last kill leaves the
    // target dead, so the trailing `Damage` and `Heal` raise nothing.
    assert_eq!(
        trace.globals["seq"],
        Val::Array(vec![
            Val::Number(1.0),
            Val::Number(3.0),
            Val::Number(1.0),
            Val::Number(3.0),
            Val::Number(1.0),
            Val::Number(3.0)
        ])
    );
    assert_eq!(trace.players["p2"].state["isDead"], Val::Bool(true));
    let world = trace.players["p2"].world.as_ref().unwrap();
    assert_eq!(world.health.current, 0.0);
}

/// `Event Was Critical Hit` has no binding on a `Damage`-raised event
/// and no scenario `values` path — an unbound read is `not_modeled`.
const CRITICAL: &str = r#"
variables {
    global:
        0: crit
}

rule ("hit") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(Value In Array(All Players(Team 2), 0), Null, 30);
    }
}

rule ("took") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Set Global Variable(crit, Event Was Critical Hit);
    }
}
"#;

#[test]
fn an_event_value_a_raised_event_does_not_bind_is_not_modeled() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper", "team": "team2" }
        ]
    }"#;
    let trace = run(CRITICAL, scenario);
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(not_modeled.construct, "value eventWasCriticalHit");
}
