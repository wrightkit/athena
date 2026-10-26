//! HUD state: the elements a run creates, their evaluated arguments, their
//! recipients and destruction, as `docs/simulator-interface.md` defines
//! them (athena#10).

use athena_sim::{HudKind, MissingInputKind, Outcome, Val, run_scenario};

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

fn scenario(players: &str) -> String {
    format!(r#"{{ "scenarioVersion": 1, "seconds": 2, "players": {players} }}"#)
}

/// Two players on different teams.
const TWO_TEAMS: &str = r#"[{ "id": "p1", "team": "team1" }, { "id": "p2", "team": "team2" }]"#;

/// One HUD text, visible to `All Players(Team 1)`.
const TEAM_HUD: &str = r#"
variables {
    global:
        0: score
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create HUD Text(All Players(Team 1), Custom String("Score: {0}", Global.score), Null, Null, Left, 0, Color(White), Color(White), Color(White), Visible To and String, Default Visibility);
    }
}
"#;

#[test]
fn a_hud_text_visible_to_one_team_lists_only_its_members() {
    let trace = run(TEAM_HUD, &scenario(TWO_TEAMS));
    assert_eq!(trace.outcome, Outcome::Complete);
    let [hud] = trace.hud.as_slice() else {
        panic!("expected one HUD element, got {:?}", trace.hud);
    };
    assert_eq!(hud.kind, HudKind::HudText);
    // Team 1 only: `p2` on team 2 is not a recipient.
    assert_eq!(hud.recipients, ["p1"]);
    assert_eq!(hud.created_tick, 0);
    assert_eq!(hud.destroyed_tick, None);
    assert_eq!(hud.args.header, Some(Val::Text("Score: 0".into())));
    assert_eq!(hud.args.subheader, Some(Val::Null));
    assert_eq!(hud.args.location.as_deref(), Some("LEFT"));
    assert_eq!(hud.args.sort_order, Some(Val::Number(0.0)));
    assert_eq!(hud.args.header_color, Some(Val::Text("WHITE".into())));
    assert_eq!(
        hud.args.reevaluation.as_deref(),
        Some("VISIBILITY_AND_STRING")
    );
    assert_eq!(hud.args.spec_visibility.as_deref(), Some("DEFAULT"));
    assert!(hud.construct.contains("createHudText"), "{}", hud.construct);
}

/// Two HUD texts; the second is destroyed through `Last Text ID`, the
/// first by `Destroy All HUD Text` after a `Wait`.
const DESTROY: &str = r#"
variables {
    global:
        0: t
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create HUD Text(All Players(All Teams), "first", Null, Null, Top, 0, Color(White), Color(White), Color(White), None, Default Visibility);
        Create HUD Text(All Players(All Teams), "second", Null, Null, Top, 0, Color(White), Color(White), Color(White), None, Default Visibility);
        Set Global Variable(t, Last Text ID);
        Destroy HUD Text(Global.t);
        Wait(0.5, Ignore Condition);
        Destroy All HUD Text;
    }
}
"#;

#[test]
fn a_destroyed_element_keeps_its_entry_with_destroyed_tick() {
    let trace = run(DESTROY, &scenario(TWO_TEAMS));
    assert_eq!(trace.outcome, Outcome::Complete);
    let [first, second] = trace.hud.as_slice() else {
        panic!("expected two HUD elements, got {:?}", trace.hud);
    };
    // `Last Text ID` named the newest element.
    assert_eq!(trace.globals["t"], Val::Number(1.0));
    assert_eq!(second.id, 1);
    assert_eq!(second.destroyed_tick, Some(0));
    // Wait(0.5) wakes on tick 32 (0.5 / 0.016 = 31.25, rounded up).
    assert_eq!(first.id, 0);
    assert_eq!(first.destroyed_tick, Some(32));
    // Both stay listed: present means `destroyedTick` is `null`.
    assert_eq!(trace.hud.len(), 2);
}

/// A progress bar and a HUD text, each destroyed by its own destroy action.
const PROGRESS_BAR: &str = r#"
rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create HUD Text(All Players(All Teams), "h", Null, Null, Top, 0, Color(White), Color(White), Color(White), None, Default Visibility);
        Create Progress Bar HUD Text(All Players(Team 1), 0.5, "boss", Top, 1, Color(Yellow), Color(Red), None, Visible Always);
        Destroy Progress Bar HUD Text(Last Text ID);
        Destroy All Progress Bar HUD Text;
    }
}
"#;

#[test]
fn a_progress_bar_records_its_arguments() {
    let trace = run(PROGRESS_BAR, &scenario(TWO_TEAMS));
    assert_eq!(trace.outcome, Outcome::Complete);
    let [hud_text, bar] = trace.hud.as_slice() else {
        panic!("expected two HUD elements, got {:?}", trace.hud);
    };
    assert_eq!(bar.kind, HudKind::ProgressBarHud);
    assert_eq!(bar.recipients, ["p1"]);
    assert_eq!(bar.args.value, Some(Val::Number(0.5)));
    assert_eq!(bar.args.text, Some(Val::Text("boss".into())));
    assert_eq!(bar.args.location.as_deref(), Some("TOP"));
    assert_eq!(bar.args.sort_order, Some(Val::Number(1.0)));
    assert_eq!(
        bar.args.progress_bar_color,
        Some(Val::Text("YELLOW".into()))
    );
    assert_eq!(bar.args.text_color, Some(Val::Text("RED".into())));
    assert_eq!(bar.args.reevaluation.as_deref(), Some("NONE"));
    assert_eq!(bar.args.spec_visibility.as_deref(), Some("VISIBLE_ALWAYS"));
    // `Last Text ID` was the progress bar; the kind-scoped destroy hit it.
    assert_eq!(bar.destroyed_tick, Some(0));
    // The HUD text is the only element left present.
    assert_eq!(hud_text.kind, HudKind::HudText);
    assert_eq!(hud_text.destroyed_tick, None);
}

/// Messages and an objective description in an event rule.
const MESSAGES: &str = r#"
variables {
    player:
        0: streak
}

rule ("kill") {
    event {
        Player Dealt Final Blow;
        All;
        All;
    }
    actions {
        Small Message(Event Player, Custom String("streak {0}", (Event Player).streak));
        Big Message(All Players(All Teams), "big");
        Set Objective Description(All Players(Team 1), "attack", None);
        Set Objective Description(All Players(All Teams), "defend", None);
    }
}
"#;

#[test]
fn a_text_built_from_a_scenario_player_variable_shows_its_value() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 2,
        "players": [
            { "id": "p1", "team": "team1", "state": { "streak": 3 } },
            { "id": "p2", "team": "team2" }
        ],
        "events": [
            { "kind": "playerDealtFinalBlow", "time": 1, "eventPlayer": "p1" }
        ]
    }"#;
    let trace = run(MESSAGES, scenario);
    assert_eq!(trace.outcome, Outcome::Complete);
    let [small, big, first, second] = trace.hud.as_slice() else {
        panic!("expected four HUD elements, got {:?}", trace.hud);
    };
    // `time: 1` is tick 62.
    assert_eq!(small.kind, HudKind::SmallMessage);
    assert_eq!(small.created_tick, 62);
    assert_eq!(small.recipients, ["p1"]);
    assert_eq!(small.args.header, Some(Val::Text("streak 3.00".into())));
    assert_eq!(big.kind, HudKind::BigMessage);
    assert_eq!(big.recipients, ["p1", "p2"]);
    assert_eq!(big.args.header, Some(Val::Text("big".into())));
    // The objective description is a singleton per player: the second call
    // replaced the first for `p1`, leaving it no recipient, so it is
    // destroyed. `p2` of team 2 was never a recipient.
    assert_eq!(first.kind, HudKind::ObjectiveDescription);
    assert_eq!(first.recipients, Vec::<String>::new());
    assert_eq!(first.destroyed_tick, Some(62));
    assert_eq!(second.kind, HudKind::ObjectiveDescription);
    assert_eq!(second.recipients, ["p1", "p2"]);
    assert_eq!(second.destroyed_tick, None);
    assert_eq!(second.args.text, Some(Val::Text("defend".into())));
    assert_eq!(second.args.reevaluation.as_deref(), Some("NONE"));
}

/// `enableMessages`/`disableMessages` resolve their player argument but
/// produce no element.
const MESSAGE_TOGGLES: &str = r#"
rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Disable Messages(All Players(Team 2));
        Enable Messages(All Players(All Teams));
    }
}
"#;

#[test]
fn message_toggles_resolve_their_player_argument() {
    let trace = run(MESSAGE_TOGGLES, &scenario(TWO_TEAMS));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(trace.hud.is_empty());
}

/// `disableMessages` reads its argument: an event player the scenario does
/// not supply is a missing input like any other read.
const DISABLE_EVENT_PLAYER: &str = r#"
rule ("kill") {
    event {
        Player Dealt Final Blow;
        All;
        All;
    }
    actions {
        Disable Messages(Event Player);
    }
}
"#;

#[test]
fn a_message_toggle_on_a_missing_event_player_is_a_missing_input() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 2,
        "players": [{ "id": "p1" }],
        "events": [{ "kind": "playerDealtFinalBlow", "time": 0 }]
    }"#;
    let trace = run(DISABLE_EVENT_PLAYER, scenario);
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::EventPlayer);
}

/// `Create Dummy Bot` is display-adjacent but not modeled — the roster
/// family needs a mutable player roster: the run stops, and elements
/// created before the stop are still listed.
const UNMODELED: &str = r#"
rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create HUD Text(All Players(All Teams), "h", Null, Null, Top, 0, Color(White), Color(White), Color(White), None, Default Visibility);
        Create Dummy Bot(Hero(Ana), Team 1, 0, Vector(0, 0, 0), Vector(0, 0, 0));
    }
}
"#;

#[test]
fn an_unmodeled_action_stops_the_run_and_keeps_earlier_elements() {
    let trace = run(UNMODELED, &scenario(TWO_TEAMS));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(
        not_modeled.construct.contains("createDummyBot"),
        "{}",
        not_modeled.construct
    );
    assert_eq!(not_modeled.tick, Some(0));
    let [hud] = trace.hud.as_slice() else {
        panic!("expected one HUD element, got {:?}", trace.hud);
    };
    assert_eq!(hud.kind, HudKind::HudText);
    assert_eq!(hud.destroyed_tick, None);
}

#[test]
fn the_trace_json_covers_hud() {
    let trace = run(TEAM_HUD, &scenario(TWO_TEAMS));
    let json = serde_json::to_value(&trace).unwrap();
    let hud = &json["hud"][0];
    assert_eq!(hud["id"], 0);
    assert_eq!(hud["kind"], "hudText");
    assert_eq!(hud["createdTick"], 0);
    assert_eq!(hud["destroyedTick"], serde_json::Value::Null);
    assert_eq!(hud["recipients"], serde_json::json!(["p1"]));
    assert_eq!(hud["construct"], "action createHudText");
    assert_eq!(
        hud["header"],
        serde_json::json!({"type": "text", "value": "Score: 0"})
    );
    assert_eq!(hud["subheader"], serde_json::json!({"type": "null"}));
    assert_eq!(hud["location"], "LEFT");
    assert_eq!(
        hud["sortOrder"],
        serde_json::json!({"type": "number", "value": 0.0})
    );
    assert_eq!(hud["headerColor"], "WHITE");
    assert_eq!(hud["reevaluation"], "VISIBILITY_AND_STRING");
    assert_eq!(hud["specVisibility"], "DEFAULT");
    // Fields the action does not take are absent.
    assert!(hud.get("value").is_none());
    assert!(hud.get("progressBarColor").is_none());
}
