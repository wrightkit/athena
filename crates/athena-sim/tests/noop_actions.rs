//! Actions with no effect on any value a program can read — inspector
//! recording, announcer, music, HUD and in-world-UI toggles, the
//! built-in game-mode flags, movement-collision toggles, camera, forced
//! names/outlines/voice-line pitch, scaling and `Preload Hero` — plus
//! `Destroy All Effects` (athena#103).

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

/// Every no-read action runs, records nothing but the variable writes
/// around it, and the rule finishes — the run shows in the trace.
#[test]
fn presentation_and_game_mode_actions_run_and_change_nothing() {
    let text = r#"
variables {
    global:
        0: before
        1: after
}

rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(before, 1);
        Disable Inspector Recording;
        Disable Built-In Game Mode Announcer;
        Disable Built-In Game Mode Music;
        Enable Built-In Game Mode Music;
        Disable Built-In Game Mode Completion;
        Disable Built-In Game Mode Scoring;
        Disable Game Mode HUD(Host Player);
        Disable Game Mode In-World UI(Host Player);
        Disable Hero HUD(Host Player);
        Disable Movement Collision With Environment(Host Player, True);
        Enable Movement Collision With Environment(Host Player);
        Disable Movement Collision With Players(Host Player);
        Disable Built-In Game Mode Respawning(All Players(All Teams));
        Enable Built-In Game Mode Respawning(All Players(All Teams));
        Start Camera(Host Player, Vector(0, 10, 0), Vector(0, 0, 0), 2);
        Stop Camera(Host Player);
        Start Forcing Dummy Bot Name(Host Player, Custom String("bot"));
        Start Forcing Player Outlines(Host Player, All Players(All Teams), True, Color(White), Always);
        Stop Forcing Player Outlines(Host Player, All Players(All Teams));
        Start Modifying Hero Voice Lines(Host Player, 1.5, False);
        Stop Modifying Hero Voice Lines(Host Player);
        Start Scaling Barriers(Host Player, 0.4, False);
        Stop Scaling Barriers(Host Player);
        Start Scaling Player(Host Player, 0.4, False);
        Stop Scaling Player(Host Player);
        Preload Hero(Host Player, Hero(Ana));
        Set Global Variable(after, 2);
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["before"], Val::number(1.0));
    assert_eq!(trace.globals["after"], Val::number(2.0));
    // The run is in the trace, finished; its only changes are the two
    // variable writes — the no-read actions recorded nothing.
    let entry = &trace.timeline.entries[0];
    assert!(
        entry
            .rules
            .iter()
            .any(|run| run.name == "t" && run.ended == RunEnd::Finished),
        "{entry:?}"
    );
    assert_eq!(entry.changes.len(), 2, "{entry:?}");
    assert!(trace.hud.is_empty() && trace.effects.is_empty());
}

/// A no-read action still evaluates its arguments left to right: a live
/// input inside one stops the run as `missing_input`, not skipped.
#[test]
fn a_no_read_action_still_evaluates_its_arguments() {
    let text = r#"
rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Disable Game Mode HUD(Position Of(Host Player));
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("expected one missing input, got {:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Position);
    assert_eq!(input.player.as_deref(), Some("p1"));
}

/// `Destroy All Effects` removes every live `createEffect` entity; a
/// played effect is already destroyed on its tick.
#[test]
fn destroy_all_effects_removes_every_live_effect() {
    let text = r#"
rule ("t") {
    event {
        Ongoing - Global;
    }
    actions {
        Create Effect(All Players(All Teams), Good Aura, Color(White), Host Player, 2, Position and Radius);
        Create Effect(All Players(All Teams), Bad Aura, Color(Red), Host Player, 3, Position and Radius);
        Play Effect(All Players(All Teams), Ring Explosion, Color(White), Host Player, 3);
        Destroy All Effects;
    }
}
"#;
    let trace = run(text, ANA);
    assert_eq!(trace.outcome, Outcome::Complete);
    let [first, second, played] = trace.effects.as_slice() else {
        panic!("expected three effect entities, got {:?}", trace.effects);
    };
    assert_eq!(first.kind, EffectKind::Effect);
    assert_eq!(first.destroyed_tick, Some(0));
    assert_eq!(second.kind, EffectKind::Effect);
    assert_eq!(second.destroyed_tick, Some(0));
    assert_eq!(played.kind, EffectKind::PlayedEffect);
    assert_eq!(played.destroyed_tick, Some(played.created_tick));
    let destroys = trace
        .timeline
        .entries
        .iter()
        .flat_map(|entry| entry.changes.iter())
        .filter(|change| matches!(change, athena_sim::Change::EffectDestroy { .. }))
        .count();
    assert_eq!(destroys, 2);
}
