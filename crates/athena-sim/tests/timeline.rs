//! The per-tick `timeline` of a run: rule executions, state changes and
//! truncation, as `docs/simulator-interface.md` defines them (athena#48).

use athena_sim::{
    Change, HudKind, MissingInputKind, Outcome, RunEnd, StopIndex, StopLocation,
    TIMELINE_CHANGE_LIMIT, TIMELINE_ENTRY_LIMIT, TIMELINE_RULE_LIMIT, Val, run_scenario,
    run_scenario_with_hero_values, run_text,
};

/// `Set Global Variable`, a `Wait(0.5)` and a second write: one entry per
/// tick with activity, the wait's wake tick recorded, `old`/`new` correct.
const WAIT_WRITE: &str = r#"
variables {
    global:
        0: a
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(a, 1);
        Wait(0.5, Ignore Condition);
        Set Global Variable(a, 2);
    }
}
"#;

#[test]
fn writes_around_a_wait_land_on_their_ticks() {
    let trace = run_text(WAIT_WRITE, athena_sim::Options { seconds: 1.0 }).unwrap();
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(!trace.timeline.truncated);
    // `Wait(0.5)` is 32 ticks (P1 rounding): the rule suspends on tick 0
    // and resumes on tick 32; the ticks between have no activity.
    let [start, resume] = trace.timeline.entries.as_slice() else {
        panic!("expected two entries, got {:?}", trace.timeline.entries);
    };
    assert_eq!(start.tick, 0);
    assert_eq!(start.time, 15.0);
    let [run] = start.rules.as_slice() else {
        panic!("expected one rule run, got {:?}", start.rules);
    };
    assert_eq!(run.rule, 0);
    assert_eq!(run.name, "init");
    assert_eq!(run.event_player, None);
    assert_eq!(run.ended, RunEnd::Waiting { wake: 32 });
    assert_eq!(
        start.changes.as_slice(),
        [Change::GlobalVariable {
            name: "a".into(),
            index: None,
            old: Val::Null,
            new: Val::Number(1.0),
        }]
    );
    assert_eq!(resume.tick, 32);
    assert_eq!(resume.time, 527.0);
    let [run] = resume.rules.as_slice() else {
        panic!("expected one rule run, got {:?}", resume.rules);
    };
    assert_eq!(run.ended, RunEnd::Finished);
    assert_eq!(
        resume.changes.as_slice(),
        [Change::GlobalVariable {
            name: "a".into(),
            index: None,
            old: Val::Number(1.0),
            new: Val::Number(2.0),
        }]
    );
}

/// A `not_modeled` stop still returns the timeline up to the stop: the
/// suspended wait's entry and the resumed tick's write stay, and the
/// execution that hit the unmodeled action is `stopped`.
#[test]
fn a_not_modeled_stop_keeps_the_timeline_before_it() {
    let text = r#"
variables {
    global:
        0: a
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(a, 1);
        Wait(0.5, Ignore Condition);
        Set Global Variable(a, 2);
        Kill(Null, Null);
    }
}
"#;
    let trace = run_text(text, athena_sim::Options { seconds: 1.0 }).unwrap();
    assert!(matches!(trace.outcome, Outcome::NotModeled(_)));
    let [start, resume] = trace.timeline.entries.as_slice() else {
        panic!("expected two entries, got {:?}", trace.timeline.entries);
    };
    assert_eq!(start.tick, 0);
    assert_eq!(start.rules[0].ended, RunEnd::Waiting { wake: 32 });
    assert_eq!(resume.tick, 32);
    assert_eq!(resume.rules[0].ended, RunEnd::Stopped);
    assert_eq!(
        resume.changes.as_slice(),
        [Change::GlobalVariable {
            name: "a".into(),
            index: None,
            old: Val::Number(1.0),
            new: Val::Number(2.0),
        }]
    );
}

/// Past `TIMELINE_ENTRY_LIMIT` active ticks the timeline stops growing and
/// sets `truncated`: a `Wait Until` with a one-tick timeout is activity on
/// every tick.
#[test]
fn the_entry_cap_truncates() {
    let text = r#"
rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        While(True);
            Wait Until(False, 0.016);
        End;
    }
}
"#;
    let trace = run_text(text, athena_sim::Options { seconds: 200.0 }).unwrap();
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(trace.timeline.truncated);
    assert_eq!(trace.timeline.entries.len(), TIMELINE_ENTRY_LIMIT);
    assert_eq!(trace.timeline.entries[0].tick, 0);
    assert_eq!(
        trace.timeline.entries[TIMELINE_ENTRY_LIMIT - 1].tick,
        TIMELINE_ENTRY_LIMIT as u64 - 1
    );
}

/// One dense tick can produce more changes than the trace keeps: past
/// `TIMELINE_CHANGE_LIMIT` they are dropped — inside the tick — and
/// `truncated` reports the loss.
#[test]
fn the_change_cap_truncates_within_a_tick() {
    let text = r#"
variables {
    global:
        0: a
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        While(True);
            Set Global Variable(a, 1);
        End;
    }
}
"#;
    let trace = run_text(text, athena_sim::Options { seconds: 1.0 }).unwrap();
    // The loop without a wait runs to the step limit on tick 0.
    assert!(matches!(trace.outcome, Outcome::NotModeled(_)));
    assert!(trace.timeline.truncated);
    let [entry] = trace.timeline.entries.as_slice() else {
        panic!("expected one entry, got {:?}", trace.timeline.entries);
    };
    assert_eq!(entry.changes.len(), TIMELINE_CHANGE_LIMIT);
}

/// Rule executions are bounded the same way: more runs than
/// `TIMELINE_RULE_LIMIT` across the trace are dropped and `truncated`
/// reports it.
#[test]
fn the_rule_cap_truncates() {
    // 100 players resume their `Ongoing - Each Player` instance every
    // tick; 17 simulated seconds is 1,063 ticks, 106,300 runs in all.
    let players = (0..100)
        .map(|index| format!(r#"{{ "id": "p{index}" }}"#))
        .collect::<Vec<_>>()
        .join(", ");
    let scenario = format!(r#"{{ "scenarioVersion": 1, "seconds": 17, "players": [{players}] }}"#);
    let text = r#"
rule ("init") {
    event {
        Ongoing - Each Player;
    }
    actions {
        While(True);
            Wait Until(False, 0.016);
        End;
    }
}
"#;
    let trace = run_scenario(text, &scenario).unwrap();
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(trace.timeline.truncated);
    let recorded: usize = trace
        .timeline
        .entries
        .iter()
        .map(|entry| entry.rules.len())
        .sum();
    assert_eq!(recorded, TIMELINE_RULE_LIMIT);
}

/// One tick's `changes` hold every modeled state change in execution
/// order: variable writes, a log line, HUD creation and a world-state
/// change.
#[test]
fn changes_of_every_kind_keep_execution_order() {
    let text = r#"
variables {
    global:
        0: a
    player:
        0: s
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(a, 1);
        Set Player Variable(Host Player, s, 2);
        Log To Inspector("line");
        Create HUD Text(All Players(All Teams), "h", Null, Null, Left, 0, Color(White), Color(White), Color(White), None, Default Visibility);
        Damage(Host Player, Null, 10);
    }
}
"#;
    let values = workshop_rs::hero_values::load(
        r#"{
            "format": "workshop-rs/hero-values-v1",
            "balanceVersion": "test-only",
            "heroes": [
                { "id": "ashe", "values": [
                    { "kind": "health", "pool": "health", "amount": 200 },
                    { "kind": "ultimateCost", "points": 2400 }
                ] }
            ]
        }"#,
    )
    .expect("the test hero values load");
    let trace = run_scenario_with_hero_values(
        text,
        r#"{ "scenarioVersion": 1, "seconds": 1,
            "players": [{ "id": "p1", "hero": "Ashe", "host": true }] }"#,
        &values,
    )
    .unwrap();
    assert_eq!(trace.outcome, Outcome::Complete);
    let [entry] = trace.timeline.entries.as_slice() else {
        panic!("expected one entry, got {:?}", trace.timeline.entries);
    };
    let kinds: Vec<_> = entry
        .changes
        .iter()
        .map(|change| match change {
            Change::GlobalVariable { .. } => "globalVariable",
            Change::PlayerVariable { .. } => "playerVariable",
            Change::World { .. } => "world",
            Change::HudCreate { .. } => "hudCreate",
            Change::HudDestroy { .. } => "hudDestroy",
            Change::EffectCreate { .. } => "effectCreate",
            Change::EffectDestroy { .. } => "effectDestroy",
            Change::Log { .. } => "log",
            Change::RandomDraw { .. } => "randomDraw",
            Change::State { .. } => "state",
            Change::Modifier { .. } => "modifier",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "globalVariable",
            "playerVariable",
            "log",
            "hudCreate",
            "world"
        ]
    );
    assert_eq!(
        entry.changes[1],
        Change::PlayerVariable {
            player: "p1".into(),
            name: "s".into(),
            index: None,
            old: Val::Null,
            new: Val::Number(2.0),
        }
    );
    assert_eq!(
        entry.changes[3],
        Change::HudCreate {
            id: 0,
            element: HudKind::HudText,
        }
    );
    assert_eq!(
        entry.changes[4],
        Change::World {
            player: "p1".into(),
            field: "health".into(),
            old: Val::Number(200.0),
            new: Val::Number(190.0),
        }
    );
}

/// A `not_modeled` stop inside an action reports the rule and the
/// zero-based action index in `outcome.location`, and the tick's `rules`
/// list shows the rule `stopped` (athena#93).
#[test]
fn a_stop_inside_an_action_names_the_rule_and_action() {
    let text = r#"
variables {
    global:
        0: a
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(a, 1);
        Kill(Null, Null);
    }
}
"#;
    let trace = run_text(text, athena_sim::Options { seconds: 1.0 }).unwrap();
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.location,
        Some(StopLocation {
            rule: 0,
            name: "init".into(),
            event_player: None,
            at: Some(StopIndex::Action(1)),
        })
    );
    let [entry] = trace.timeline.entries.as_slice() else {
        panic!("expected one entry, got {:?}", trace.timeline.entries);
    };
    assert_eq!(entry.tick, 0);
    let [run] = entry.rules.as_slice() else {
        panic!("expected one rule run, got {:?}", entry.rules);
    };
    assert_eq!(run.rule, 0);
    assert_eq!(run.ended, RunEnd::Stopped);
    // The outcome's location serializes into the trace JSON as documented.
    let location = &serde_json::to_value(&trace).unwrap()["outcome"]["location"];
    assert_eq!(
        *location,
        serde_json::json!({
            "rule": 0, "name": "init", "eventPlayer": null, "action": 1
        })
    );
}

/// A `not_modeled` stop inside a rule condition reports the rule, the
/// `Event Player` the check ran for and the zero-based condition index;
/// the timeline lists the rule `stopped` even though it never ran.
#[test]
fn a_stop_inside_a_condition_names_the_rule_event_player_and_condition() {
    let text = r#"
variables {
    global:
        0: a
    player:
        0: marker
}

rule ("each") {
    event {
        Ongoing - Each Player;
    }
    conditions {
        True;
        Current Array Element == 1;
    }
    actions {
        Set Player Variable(Event Player, marker, 1);
    }
}
"#;
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1" }, { "id": "p2" }] }"#;
    let trace = run_scenario(text, scenario).unwrap();
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.location,
        Some(StopLocation {
            rule: 0,
            name: "each".into(),
            event_player: Some("p1".into()),
            at: Some(StopIndex::Condition(1)),
        })
    );
    let [entry] = trace.timeline.entries.as_slice() else {
        panic!("expected one entry, got {:?}", trace.timeline.entries);
    };
    let [run] = entry.rules.as_slice() else {
        panic!("expected one rule run, got {:?}", entry.rules);
    };
    assert_eq!(run.event_player.as_deref(), Some("p1"));
    assert_eq!(run.ended, RunEnd::Stopped);
    let location = &serde_json::to_value(&trace).unwrap()["outcome"]["location"];
    assert_eq!(
        *location,
        serde_json::json!({
            "rule": 0, "name": "each", "eventPlayer": "p1", "condition": 1
        })
    );
}

/// A `missing_input` stop reports the rule, `Event Player` and index on
/// the `missingInputs` entry, and the timeline lists the rule `stopped`.
#[test]
fn a_missing_input_stop_names_the_rule_and_action() {
    let text = r#"
variables {
    global:
        0: a
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(a, 1);
        Set Global Variable(a, Is Button Held(Host Player, Ability 1));
    }
}
"#;
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "hero": "ana", "host": true }] }"#;
    let trace = run_scenario(text, scenario).unwrap();
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [missing] = trace.missing_inputs.as_slice() else {
        panic!("expected one missing input, got {:?}", trace.missing_inputs);
    };
    assert_eq!(missing.input, MissingInputKind::Button);
    assert_eq!(missing.player.as_deref(), Some("p1"));
    assert_eq!(missing.name.as_deref(), Some("ability1"));
    assert_eq!(missing.tick, Some(0));
    assert_eq!(
        missing.location,
        Some(StopLocation {
            rule: 0,
            name: "init".into(),
            event_player: None,
            at: Some(StopIndex::Action(1)),
        })
    );
    let [entry] = trace.timeline.entries.as_slice() else {
        panic!("expected one entry, got {:?}", trace.timeline.entries);
    };
    let [run] = entry.rules.as_slice() else {
        panic!("expected one rule run, got {:?}", entry.rules);
    };
    assert_eq!(run.ended, RunEnd::Stopped);
    let location = &serde_json::to_value(&trace).unwrap()["missingInputs"][0]["location"];
    assert_eq!(
        *location,
        serde_json::json!({
            "rule": 0, "name": "init", "eventPlayer": null, "action": 1
        })
    );
}
