//! Player events (athena#47): injected `events` entries dispatch as
//! before, and the modeled `Damage`/`Heal` state changes raise the
//! player events they cause — `Player Took Damage`, `Player Dealt
//! Damage`, `Player Died`, `Player Dealt Final Blow`, `Player Received
//! Healing`, `Player Dealt Healing` — in occurrence order, binding
//! `Event Player`, `Attacker`, `Victim`, `Event Damage` and
//! `Event Healing`. One liveness predicate — the `isDead` read — gates
//! `Damage`, `Heal` and `Resurrect`, and a revive restores the pool it
//! gates on so a revived player can die again.

use athena_sim::{Outcome, RunError, ScenarioError, Val, run_scenario};

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

/// The names of the rules that ran on a tick, in execution order.
fn run_names(trace: &athena_sim::Trace, tick: usize) -> Vec<&str> {
    trace.timeline.entries[tick]
        .rules
        .iter()
        .map(|run| run.name.as_str())
        .collect()
}

/// A `Player Left Match` counter.
const LEFT_RULE: &str = r#"
variables {
    global:
        0: left
        1: who
}

rule ("left") {
    event {
        Player Left Match;
        All;
        All;
    }
    actions {
        Modify Global Variable(left, Add, 1);
        Set Global Variable(who, Hero Of(Event Player));
    }
}
"#;

/// A scenario event runs a matching rule once, bound to its event
/// player — the join/leave family only ever comes from `events`.
#[test]
fn an_injected_player_event_runs_a_matching_rule_once() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "host": true, "hero": "ana", "team": "team1" },
            { "id": "p2", "hero": "reaper", "team": "team2" }
        ],
        "events": [
            { "kind": "playerLeft", "time": 0, "eventPlayer": "p2" },
            { "kind": "playerJoined", "time": 0, "eventPlayer": "p1" }
        ]
    }"#;
    let trace = run(LEFT_RULE, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["left"], Val::number(1.0));
    assert_eq!(trace.globals["who"], Val::Text("REAPER".into()));
    let [run] = trace.timeline.entries[0].rules.as_slice() else {
        panic!(
            "expected one rule run, got {:?}",
            trace.timeline.entries[0].rules
        );
    };
    assert_eq!(run.event_player.as_deref(), Some("p2"));
}

/// A `Damage` driver and rules counting each raised event, with reads
/// of the bound event values. `blow` is declared before `died`: the
/// runs order is the events' occurrence order, not the file's.
const DAMAGE_EVENTS: &str = r#"
variables {
    global:
        0: took
        1: dealt
        2: blow
        3: died
        4: dmg
        5: atk
        6: vic
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Host Player, 50);
        Damage(All Players(Team 2), Host Player, 9999);
    }
}

rule ("took") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(took, Add, 1);
        Set Global Variable(dmg, Event Damage);
        Set Global Variable(atk, Hero Of(Attacker));
        Set Global Variable(vic, Hero Of(Victim));
    }
}

rule ("dealt") {
    event {
        Player Dealt Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(dealt, Add, 1);
    }
}

rule ("blow") {
    event {
        Player Dealt Final Blow;
        All;
        All;
    }
    actions {
        Modify Global Variable(blow, Add, 1);
    }
}

rule ("died") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Modify Global Variable(died, Add, 1);
    }
}
"#;

/// Two players: the host dealer on team 1, the victim on team 2.
const DUEL: &str = r#"{
    "scenarioVersion": 1,
    "seconds": 1,
    "players": [
        { "id": "p1", "host": true, "hero": "ana", "team": "team1" },
        { "id": "p2", "hero": "reaper", "team": "team2" }
    ]
}"#;

/// A non-lethal hit raises `Player Took Damage` then `Player Dealt
/// Damage`; the killing hit adds `Player Died` then `Player Dealt Final
/// Blow`. `blow` is declared before `died` — occurrence order wins.
#[test]
fn a_modeled_damage_hit_raises_the_damage_events_in_occurrence_order() {
    let trace = run(DAMAGE_EVENTS, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["took"], Val::number(2.0));
    assert_eq!(trace.globals["dealt"], Val::number(2.0));
    assert_eq!(trace.globals["died"], Val::number(1.0));
    assert_eq!(trace.globals["blow"], Val::number(1.0));
    // `Event Damage` is the applied hit; `Attacker`/`Victim` the dealer
    // and the target.
    assert_eq!(trace.globals["dmg"], Val::number(9999.0));
    assert_eq!(trace.globals["atk"], Val::Text("ANA".into()));
    assert_eq!(trace.globals["vic"], Val::Text("REAPER".into()));
    assert_eq!(
        run_names(&trace, 0),
        ["driver", "took", "dealt", "took", "dealt", "died", "blow"]
    );
    // Each dealt-side run binds the dealer as `Event Player`, the
    // victim's `Player Died` run the victim.
    assert_eq!(trace.timeline.entries[0].rules[2].name, "dealt");
    assert_eq!(
        trace.timeline.entries[0].rules[2].event_player.as_deref(),
        Some("p1")
    );
    assert_eq!(
        trace.timeline.entries[0].rules[5].event_player.as_deref(),
        Some("p2")
    );
    assert_eq!(
        trace.timeline.entries[0].rules[6].event_player.as_deref(),
        Some("p1")
    );
    // The killing damage wrote the `isDead` cell.
    assert_eq!(trace.players["p2"].state["isDead"], Val::Bool(true));
}

/// `Damage` without a dealer raises only the victim's event; `Attacker`
/// reads `null` where the scenario's `values` would read the same.
#[test]
fn an_environment_hit_raises_no_dealt_events() {
    let text = r#"
variables {
    global:
        0: took
        1: dealt
        2: atk
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Null, 50);
    }
}

rule ("took") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(took, Add, 1);
        Set Global Variable(atk, Attacker);
    }
}

rule ("dealt") {
    event {
        Player Dealt Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(dealt, Add, 1);
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["took"], Val::number(1.0));
    // The dealt rule never ran: the variable stays `null`.
    assert_eq!(trace.globals["dealt"], Val::Null);
    assert_eq!(trace.globals["atk"], Val::Null);
}

/// `Heal` raises `Player Received Healing` then `Player Dealt Healing`,
/// binding `Event Healing` to the applied amount.
#[test]
fn a_modeled_heal_raises_the_healing_events() {
    let text = r#"
variables {
    global:
        0: received
        1: dealt
        2: healed
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Null, 50);
        Heal(All Players(Team 2), Host Player, 20);
    }
}

rule ("received") {
    event {
        Player Received Healing;
        All;
        All;
    }
    actions {
        Modify Global Variable(received, Add, 1);
        Set Global Variable(healed, Event Healing);
    }
}

rule ("dealt") {
    event {
        Player Dealt Healing;
        All;
        All;
    }
    actions {
        Modify Global Variable(dealt, Add, 1);
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["received"], Val::number(1.0));
    assert_eq!(trace.globals["dealt"], Val::number(1.0));
    assert_eq!(trace.globals["healed"], Val::number(20.0));
    assert_eq!(run_names(&trace, 0), ["driver", "received", "dealt"]);
    let dealt_run = &trace.timeline.entries[0].rules[2];
    assert_eq!(dealt_run.event_player.as_deref(), Some("p1"));
}

/// The rule's team and player filters apply to a raised event's player
/// exactly as to an injected one's.
#[test]
fn a_nonmatching_filter_does_not_run_a_raised_event() {
    let text = r#"
variables {
    global:
        0: team2
        1: team1
        2: reaper
        3: bastion
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Null, 50);
    }
}

rule ("team2") {
    event {
        Player Took Damage;
        Team 2;
        All;
    }
    actions {
        Modify Global Variable(team2, Add, 1);
    }
}

rule ("team1") {
    event {
        Player Took Damage;
        Team 1;
        All;
    }
    actions {
        Modify Global Variable(team1, Add, 1);
    }
}

rule ("reaper") {
    event {
        Player Took Damage;
        All;
        Reaper;
    }
    actions {
        Modify Global Variable(reaper, Add, 1);
    }
}

rule ("bastion") {
    event {
        Player Took Damage;
        All;
        Bastion;
    }
    actions {
        Modify Global Variable(bastion, Add, 1);
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["team2"], Val::number(1.0));
    assert_eq!(trace.globals["reaper"], Val::number(1.0));
    // The non-matching rules never ran: the variables stay `null`.
    assert_eq!(trace.globals["team1"], Val::Null);
    assert_eq!(trace.globals["bastion"], Val::Null);
}

/// An event value the modeled cause did not bind — `Event Was Critical
/// Hit` here — has no scenario `values` path, so the read is
/// `not_modeled`, never a missing input.
#[test]
fn an_unbound_event_value_on_a_raised_event_is_not_modeled() {
    let text = r#"
variables {
    global:
        0: crit
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Null, 50);
    }
}

rule ("crit") {
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
    let trace = run(text, DUEL);
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(not_modeled.construct, "value eventWasCriticalHit");
}

/// `Kill` writes `isDead`, and that one read gates `Damage` and `Heal`:
/// a dead target's pools do not move and no events come out of it.
#[test]
fn a_dead_target_takes_no_damage_healing_or_events() {
    let text = r#"
variables {
    global:
        0: events
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Kill(All Players(Team 2), Null);
        Damage(All Players(Team 2), Host Player, 50);
        Heal(All Players(Team 2), Host Player, 50);
    }
}

rule ("events") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(events, Add, 1);
    }
}

rule ("healed") {
    event {
        Player Received Healing;
        All;
        All;
    }
    actions {
        Modify Global Variable(events, Add, 1);
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["events"], Val::Null);
    let world = trace.players["p2"].world.as_ref().expect("world state");
    assert_eq!(world.health.current, world.health.max);
    assert_eq!(run_names(&trace, 0), ["driver"]);
}

/// A `Damage`-killed player revives through `Resurrect`, which also
/// refills the pool — the read `Is Dead` makes — so it can die again.
#[test]
fn a_killed_player_dies_again_after_resurrect() {
    let text = r#"
variables {
    global:
        0: died
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Host Player, 9999);
        Resurrect(All Players(Team 2));
        Damage(All Players(Team 2), Host Player, 9999);
    }
}

rule ("died") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Modify Global Variable(died, Add, 1);
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    // Without the revive's health-pool refill the second hit could not
    // reach zero, so `Player Died` would fire once.
    assert_eq!(trace.globals["died"], Val::number(2.0));
    assert_eq!(trace.players["p2"].state["isDead"], Val::Bool(true));
}

/// The same with `Respawn`: "with full health" refills the pool.
#[test]
fn a_killed_player_dies_again_after_respawn() {
    let text = r#"
variables {
    global:
        0: died
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Host Player, 9999);
        Respawn(All Players(Team 2));
        Damage(All Players(Team 2), Host Player, 9999);
    }
}

rule ("died") {
    event {
        Player Died;
        All;
        All;
    }
    actions {
        Modify Global Variable(died, Add, 1);
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["died"], Val::number(2.0));
}

/// The catalog splits the revives: `Resurrect` acts on dead players, so
/// a living target keeps its partial pool; `Respawn` restores "with
/// full health, even if they were already alive".
#[test]
fn resurrect_does_not_heal_a_living_player_but_respawn_does() {
    let text = r#"
rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Null, 50);
        Resurrect(All Players(Team 2));
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    let world = trace.players["p2"].world.as_ref().expect("world state");
    assert_eq!(world.health.current.get(), world.health.max.get() - 50.0);

    let text = r#"
rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Null, 50);
        Respawn(All Players(Team 2));
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    let world = trace.players["p2"].world.as_ref().expect("world state");
    assert_eq!(world.health.current, world.health.max);
}

/// `Is Dead` reads the `isDead` cell `Damage` wrote on a killing hit,
/// and that same read gates the next `Damage`/`Heal` on the dead
/// player: no pools move, no events raise.
#[test]
fn damage_death_is_dead_to_reads_and_further_damage() {
    let text = r#"
variables {
    global:
        0: dead
        1: events
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Null, 9999);
        Set Global Variable(dead, Is Dead(Value In Array(All Players(Team 2), 0)));
        Damage(All Players(Team 2), Null, 50);
        Heal(All Players(Team 2), Null, 50);
    }
}

rule ("events") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(events, Add, 1);
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["dead"], Val::Bool(true));
    // Only the killing hit's `Player Took Damage` ran.
    assert_eq!(trace.globals["events"], Val::number(1.0));
}

/// An instance mid-`Wait` queues a raised event like an injected one
/// and fires it when the run is idle.
#[test]
fn a_busy_instance_queues_a_raised_event_until_idle() {
    let text = r#"
variables {
    global:
        0: took
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        Damage(All Players(Team 2), Null, 50);
        Damage(All Players(Team 2), Null, 50);
    }
}

rule ("took") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Modify Global Variable(took, Add, 1);
        Wait(0.032, Ignore Condition);
    }
}
"#;
    let trace = run(text, DUEL);
    assert_eq!(trace.outcome, Outcome::Complete);
    // The second event queued behind the suspended run and fired when
    // the `Wait` resumed and ended it.
    assert_eq!(trace.globals["took"], Val::number(2.0));
}

/// The per-tick bound on raised events keeps a `Damage` fan-out — a
/// loop re-raising events faster than they drain — from growing the
/// queue without limit.
#[test]
fn a_tick_raising_more_than_the_bound_stops_not_modeled() {
    let text = r#"
variables {
    global:
        0: i
}

rule ("driver") {
    event {
        Ongoing - Global;
    }
    actions {
        For Global Variable(i, 0, 2000, 1);
            Heal(All Players(Team 2), Null, 1);
        End;
    }
}
"#;
    let trace = run(text, DUEL);
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "more than 1024 player events raised in one tick"
    );
}

/// `Attacker`/`Victim` in scenario `values` name a declared player `id`
/// or `null`; any other JSON kind is a load error, and so is an id the
/// scenario does not declare.
#[test]
fn event_attacker_and_victim_values_are_player_ids_or_null() {
    let scenario_error = |json: &str| match run_scenario(LEFT_RULE, json) {
        Err(RunError::Scenario(error)) => error,
        other => panic!("expected a scenario refusal, got {other:?}"),
    };
    let base = |values: &str| {
        format!(
            r#"{{ "scenarioVersion": 1, "seconds": 1,
                "players": [{{ "id": "p1", "hero": "ana" }}],
                "events": [{{ "kind": "playerLeft", "time": 0, "eventPlayer": "p1",
                    "values": {values} }}] }}"#
        )
    };
    // A declared player id loads.
    run_scenario(LEFT_RULE, &base(r#"{ "attacker": "p1", "victim": "p1" }"#))
        .unwrap_or_else(|error| panic!("{error}"));
    // `null` loads.
    run_scenario(LEFT_RULE, &base(r#"{ "attacker": null }"#))
        .unwrap_or_else(|error| panic!("{error}"));
    // An undeclared id refuses.
    assert!(matches!(
        scenario_error(&base(r#"{ "attacker": "ghost" }"#)),
        ScenarioError::Load(_)
    ));
    // Every other JSON kind refuses.
    for value in ["5", "true", "[]", "{}"] {
        let json = base(&format!(r#"{{ "victim": {value} }}"#));
        assert!(
            matches!(
                scenario_error(&json),
                ScenarioError::Load(_) | ScenarioError::Format(_)
            ),
            "{value} should be refused"
        );
    }
}

/// An injected event's `attacker`/`victim` values reach the rule's
/// reads as `Val::Player`/`Val::Null`.
#[test]
fn injected_attacker_and_victim_values_are_read() {
    let text = r#"
variables {
    global:
        0: atk
        1: vic
}

rule ("hit") {
    event {
        Player Took Damage;
        All;
        All;
    }
    actions {
        Set Global Variable(atk, Attacker);
        Set Global Variable(vic, Victim);
    }
}
"#;
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [
            { "id": "p1", "hero": "ana" },
            { "id": "p2", "hero": "reaper" }
        ],
        "events": [{
            "kind": "playerTookDamage",
            "time": 0,
            "eventPlayer": "p2",
            "values": { "attacker": "p1", "victim": "p2", "eventDamage": 30 }
        }]
    }"#;
    let trace = run(text, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["atk"], Val::Player("p1".into()));
    assert_eq!(trace.globals["vic"], Val::Player("p2".into()));
}
