//! `Set Status`/`Clear Status`/`Has Status`, `Allow`/`Disallow Button`,
//! `Is Button Held`, `Start Rule` and the effect actions (athena#58).

use athena_sim::{
    EffectKind, MissingInputKind, Outcome, RunEnd, Val, run_scenario_with_hero_values,
};

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario_with_hero_values(text, scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"))
}

/// A test-only hero-values set: `ana` has a health pool and an ultimate
/// cost, all a world-state touch needs.
fn test_values() -> workshop_rs::hero_values::HeroValues {
    workshop_rs::hero_values::load(
        r#"{
            "format": "workshop-rs/hero-values-v1",
            "balanceVersion": "test-only",
            "heroes": [
                { "id": "ana", "values": [
                    { "kind": "health", "pool": "health", "amount": 200 },
                    { "kind": "ultimateCost", "points": 2100 }
                ] }
            ]
        }"#,
    )
    .expect("the test hero values load")
}

/// One host player whose catalog state resolves.
const ANA: &str = r#"{ "scenarioVersion": 1, "seconds": 2,
    "players": [{ "id": "p1", "hero": "Ana", "host": true }] }"#;

/// A status applies until its duration elapses.
#[test]
fn a_status_is_held_until_its_duration_expires() {
    let text = r#"
variables {
    global:
        0: during
        1: after
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Status(Host Player, Null, Asleep, 0.1);
        Set Global Variable(during, Has Status(Host Player, Asleep));
        Wait(0.2, Ignore Condition);
        Set Global Variable(after, Has Status(Host Player, Asleep));
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["during"], Val::Bool(true));
    assert_eq!(trace.globals["after"], Val::Bool(false));
    let world = trace.players["p1"].world.as_ref().expect("world resolves");
    assert_eq!(world.statuses.len(), 1);
}

/// `Clear Status` removes a status early; `Has Status` reads false.
#[test]
fn clear_status_removes_the_status() {
    let text = r#"
variables {
    global:
        0: held
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Status(Host Player, Null, Asleep, 10);
        Clear Status(Host Player, Asleep);
        Set Global Variable(held, Has Status(Host Player, Asleep));
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["held"], Val::Bool(false));
}

/// A status write is a world-state touch: a player without a `hero` has
/// no resolved world state, so `Set Status` on it stops the run.
#[test]
fn a_status_on_an_unresolved_player_is_not_modeled() {
    let text = r#"
rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Status(Host Player, Null, Asleep, 1);
    }
}
"#;
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 2,
        "players": [{ "id": "p1", "host": true }] }"#;
    let trace = run(text, scenario);
    let Outcome::NotModeled(_) = trace.outcome else {
        panic!("expected not_modeled, got {:?}", trace.outcome);
    };
}

/// Button allow state is recorded per slot on the player's world state.
#[test]
fn disallow_and_allow_button_record_the_slot() {
    let text = r#"
rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Disallow Button(Host Player, Button(Ability 1));
        Disallow Button(Host Player, Button(Jump));
        Allow Button(Host Player, Button(Jump));
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    let world = trace.players["p1"].world.as_ref().expect("world resolves");
    assert_eq!(world.buttons.get("ability1"), Some(&false));
    assert_eq!(world.buttons.get("jump"), Some(&true));
}

/// `Is Button Held` reads a live input; a scenario that does not supply
/// it stops the run as `missing_input` naming the slot.
#[test]
fn is_button_held_is_a_missing_input_the_scenario_does_not_supply() {
    let text = r#"
rule ("t") {
    event {
        Ongoing - Global;
    }
    conditions {
        Is Button Held(Host Player, Button(Jump));
    }
    actions {
        Abort;
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("expected one missing input, got {:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Button);
    assert_eq!(input.player.as_deref(), Some("p1"));
    assert_eq!(input.name.as_deref(), Some("jump"));
}

/// A scenario `buttons` map supplies the held state the value reads.
#[test]
fn is_button_held_reads_the_scenario_buttons_map() {
    let text = r#"
variables {
    global:
        0: held
        1: low
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(held, Is Button Held(Host Player, Button(Jump)));
        Set Global Variable(low, Is Button Held(Host Player, Button(Crouch)));
    }
}
"#;
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 2,
        "players": [{ "id": "p1", "hero": "Ana", "host": true,
            "buttons": { "jump": true, "crouch": false } }] }"#;
    let trace = run(text, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["held"], Val::Bool(true));
    assert_eq!(trace.globals["low"], Val::Bool(false));
}

/// A button the scenario does not list is missing, not `false`.
#[test]
fn an_unlisted_button_is_still_a_missing_input() {
    let text = r#"
rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(g, Is Button Held(Host Player, Button(Jump)));
    }
}
"#;
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 2,
        "players": [{ "id": "p1", "hero": "Ana", "host": true,
            "buttons": { "crouch": false } }] }"#;
    let trace = run(text, scenario);
    assert_eq!(trace.outcome, Outcome::MissingInput);
}

/// `Start Rule` launches the subroutine's own rule instance; the started
/// run shows in the timeline.
#[test]
fn start_rule_runs_the_subroutine_rule() {
    let text = r#"
variables {
    global:
        0: a
        1: b
}

subroutines {
    0: sub
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Start Rule(sub, Do Nothing);
        Set Global Variable(a, 1);
    }
}

rule ("Subroutine sub") {
    event {
        Subroutine;
        sub;
    }
    actions {
        Set Global Variable(b, 2);
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["a"], Val::Number(1.0));
    assert_eq!(trace.globals["b"], Val::Number(2.0));
    let entry = &trace.timeline.entries[0];
    assert!(
        entry
            .rules
            .iter()
            .any(|run| run.name == "Subroutine sub" && run.ended == RunEnd::Finished),
        "{entry:?}"
    );
}

/// `Do Nothing` leaves a running subroutine alone; `Restart Rule` starts
/// it over. Both matter while the subroutine sits in a `Wait`.
#[test]
fn start_rule_behavior_applies_to_a_running_subroutine() {
    let program = |behavior: &str| {
        format!(
            r#"
variables {{
    global:
        0: c
        1: b
}}

subroutines {{
    0: sub
}}

rule ("t") {{
    event {{
        Ongoing - Global;
    }}
    actions {{
        Start Rule(sub, {behavior});
        Wait(0.1, Ignore Condition);
        Start Rule(sub, {behavior});
    }}
}}

rule ("Subroutine sub") {{
    event {{
        Subroutine;
        sub;
    }}
    actions {{
        Modify Global Variable(c, Add, 1);
        Wait(0.5, Ignore Condition);
        Modify Global Variable(b, Add, 1);
    }}
}}
"#
        )
    };
    // Do Nothing: the second start is dropped while `sub` waits.
    let trace = run(&program("Do Nothing"), ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["c"], Val::Number(1.0));
    assert_eq!(trace.globals["b"], Val::Number(1.0));
    // Restart Rule: the second start re-runs the head, so `c` counts twice.
    let trace = run(&program("Restart Rule"), ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["c"], Val::Number(2.0));
    assert_eq!(trace.globals["b"], Val::Number(1.0));
}

/// A `Wait Until` inside a started subroutine re-checks its own
/// condition every tick and resumes when it holds, not at the timeout.
#[test]
fn a_wait_until_inside_a_started_subroutine_rechecks_its_condition() {
    let text = r#"
variables {
    global:
        0: go
        1: done
}

subroutines {
    0: sub
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Start Rule(sub, Do Nothing);
        Wait(0.1, Ignore Condition);
        Set Global Variable(go, True);
    }
}

rule ("Subroutine sub") {
    event {
        Subroutine;
        sub;
    }
    actions {
        Wait Until(Compare(Global Variable(go), ==, True), 5);
        Set Global Variable(done, Total Time Elapsed);
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    // `go` turns true around 0.11 s; the 5 s timeout never applies.
    let Val::Number(done) = trace.globals["done"] else {
        panic!("expected the subroutine to finish, got {:?}", trace.globals)
    };
    assert!(done < 1.0, "resumed at {done}s");
}

/// The simulator never starts a chase, so `Stop Chasing` has nothing to
/// stop; the action resolves its player and continues.
#[test]
fn stop_chasing_a_variable_nothing_chases_is_a_no_op() {
    let text = r#"
variables {
    global:
        0: a
    player:
        0: x
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Stop Chasing Player Variable(Host Player, x);
        Set Global Variable(a, 1);
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["a"], Val::Number(1.0));
}

/// `Create Effect` records an entity like a HUD element — recipients and
/// evaluated arguments — and `Last Created Entity` + `Destroy Effect`
/// track it by id. `Play Effect` records a one-tick entity.
#[test]
fn effects_are_recorded_and_destroyed_by_id() {
    let text = r#"
variables {
    global:
        0: e
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create Effect(All Players(All Teams), Good Aura, Color(White), Host Player, 2, Position and Radius);
        Set Global Variable(e, Last Created Entity);
        Play Effect(All Players(All Teams), Ring Explosion, Color(White), Host Player, 3);
        Destroy Effect(e);
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["e"], Val::Number(0.0));
    let [created, played] = trace.effects.as_slice() else {
        panic!("expected two effect entities, got {:?}", trace.effects);
    };
    assert_eq!(created.kind, EffectKind::Effect);
    assert_eq!(created.recipients, ["p1"]);
    assert_eq!(created.args.effect.as_deref(), Some("GOOD_AURA"));
    assert_eq!(created.args.color.as_deref(), Some("WHITE"));
    assert_eq!(created.args.position, Some(Val::Player("p1".into())));
    assert_eq!(created.args.radius, Some(Val::Number(2.0)));
    assert_eq!(
        created.args.reevaluation.as_deref(),
        Some("POSITION_AND_RADIUS")
    );
    assert_eq!(created.created_tick, 0);
    assert_eq!(created.destroyed_tick, Some(0));
    assert_eq!(played.kind, EffectKind::PlayedEffect);
    assert_eq!(played.destroyed_tick, Some(played.created_tick));
    // A played effect hands out no entity id.
    assert_eq!(played.args.effect.as_deref(), Some("RING_EXPLOSION"));
}

/// An unknown, `null` or already-destroyed entity destroys nothing — the
/// same rule a HUD destroy follows.
#[test]
fn destroying_null_or_a_dead_effect_changes_nothing() {
    let text = r#"
rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Destroy Effect(Null);
        Create Effect(All Players(All Teams), Good Aura, Color(White), Host Player, 2, Position and Radius);
        Destroy Effect(Last Created Entity);
        Destroy Effect(0);
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    let destroys = trace
        .timeline
        .entries
        .iter()
        .flat_map(|entry| entry.changes.iter())
        .filter(|change| matches!(change, athena_sim::Change::EffectDestroy { .. }))
        .count();
    assert_eq!(destroys, 1);
}

/// A position argument that needs geometry — here `Position Of` — stops
/// the run as `not_modeled` instead of recording an element.
#[test]
fn an_effect_position_needing_geometry_is_not_modeled() {
    let text = r#"
rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Play Effect(All Players(All Teams), Ring Explosion, Color(White), Position Of(Host Player), 3);
    }
}
"#;
    let trace = run(text, ANA);
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("expected not_modeled, got {:?}", trace.outcome);
    };
    assert!(stop.construct.contains("getPosition"), "{stop:?}");
}
