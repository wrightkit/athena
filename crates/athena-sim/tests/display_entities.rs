//! Entity-creating display actions (athena#118): the four text creates
//! share the `TextId` space `Last Text ID` reads and the text destroys
//! target; the four entity creates share the `EntityId` space `Last
//! Created Entity`, `Entity Exists` and `Entity Count` read and
//! `Destroy Effect`/`Destroy Icon` target.

use athena_sim::{EffectKind, HudKind, Outcome, Val, run_scenario};

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

fn scenario(players: &str) -> String {
    format!(r#"{{ "scenarioVersion": 1, "seconds": 2, "players": {players} }}"#)
}

const TWO_TEAMS: &str = r#"[{ "id": "p1", "team": "team1" }, { "id": "p2", "team": "team2" }]"#;

/// One of each create action, then `Last Text ID`/`Last Created Entity`
/// read back into globals.
const CREATES: &str = r#"
variables {
    global:
        0: t
        1: e
        2: tc
        3: ec
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create HUD Text(All Players(All Teams), "h", Null, Null, Top, 0, Color(White), Color(White), Color(White), None, Default Visibility);
        Create In-World Text(All Players(Team 1), "w", Vector(1, 2, 3), 0.7, Clip Against Surfaces, None, Color(Yellow), Visible Always);
        Create Progress Bar In-World Text(All Players(All Teams), 25, "bar", Vector(0, 1, 0), 1, Do Not Clip, Color(Blue), Color(Red), Visible To and Values, Default Visibility);
        Create Effect(All Players(All Teams), Good Aura, Color(White), Vector(0, 0, 0), 2, None);
        Create Beam Effect(All Players(Team 2), Good Beam, Vector(0, 0, 0), Vector(1, 1, 1), Color(Green), None);
        Create Icon(All Players(All Teams), Vector(0, 5, 0), Warning, Visible To and Position, Color(Blue), True);
        Set Global Variable(t, Last Text ID);
        Set Global Variable(e, Last Created Entity);
        Set Global Variable(tc, Text Count);
        Set Global Variable(ec, Entity Count);
    }
}
"#;

/// Each create action records an element with its evaluated arguments
/// and creating action; the text and entity id spaces each span their
/// create actions.
#[test]
fn creates_record_elements_and_hand_out_shared_ids() {
    let trace = run(CREATES, &scenario(TWO_TEAMS));
    assert_eq!(trace.outcome, Outcome::Complete);
    let [hud_text, in_world, bar] = trace.hud.as_slice() else {
        panic!("expected three display elements, got {:?}", trace.hud);
    };
    assert_eq!(hud_text.kind, HudKind::HudText);
    assert_eq!(in_world.kind, HudKind::InWorldText);
    assert_eq!(in_world.id, 1);
    assert_eq!(in_world.recipients, ["p1"]);
    assert_eq!(in_world.construct, "action createInWorldText");
    assert_eq!(in_world.args.text, Some(Val::Text("w".into())));
    assert_eq!(
        in_world.args.position,
        Some(Val::Vector {
            x: 1.0,
            y: 2.0,
            z: 3.0
        })
    );
    assert_eq!(in_world.args.scale, Some(Val::Number(0.7)));
    assert_eq!(
        in_world.args.clipping.as_deref(),
        Some("CLIP_AGAINST_SURFACES")
    );
    assert_eq!(in_world.args.reevaluation.as_deref(), Some("NONE"));
    assert_eq!(in_world.args.color, Some(Val::Text("YELLOW".into())));
    assert_eq!(
        in_world.args.spec_visibility.as_deref(),
        Some("VISIBLE_ALWAYS")
    );
    assert_eq!(bar.kind, HudKind::ProgressBarInWorldText);
    assert_eq!(bar.args.value, Some(Val::Number(25.0)));
    assert_eq!(bar.args.progress_bar_color, Some(Val::Text("BLUE".into())));
    assert_eq!(
        bar.args.reevaluation.as_deref(),
        Some("VISIBILITY_AND_VALUES")
    );
    let [effect, beam, icon] = trace.effects.as_slice() else {
        panic!("expected three entities, got {:?}", trace.effects);
    };
    assert_eq!(effect.kind, EffectKind::Effect);
    assert_eq!(beam.kind, EffectKind::Beam);
    assert_eq!(beam.id, 1);
    assert_eq!(beam.recipients, ["p2"]);
    assert_eq!(beam.construct, "action createBeamEffect");
    assert_eq!(beam.args.effect.as_deref(), Some("GOOD"));
    assert_eq!(
        beam.args.end_position,
        Some(Val::Vector {
            x: 1.0,
            y: 1.0,
            z: 1.0
        })
    );
    assert_eq!(icon.kind, EffectKind::Icon);
    assert_eq!(icon.id, 2);
    assert_eq!(icon.args.icon.as_deref(), Some("WARNING"));
    assert_eq!(icon.args.show_when_offscreen, Some(Val::Bool(true)));
    // `Last Text ID` is the newest text element across the four
    // creates — the in-world progress bar — and `Last Created Entity`
    // the newest entity, the icon.
    assert_eq!(trace.globals["t"], Val::Number(2.0));
    assert_eq!(trace.globals["e"], Val::Number(2.0));
    assert_eq!(trace.globals["tc"], Val::Number(3.0));
    assert_eq!(trace.globals["ec"], Val::Number(3.0));
}

/// A projectile effect is a durable entity `Last Created Entity` reads;
/// no destroy action names it, and `Entity Count` does not count it.
#[test]
fn a_projectile_effect_is_an_undestroyable_entity() {
    let text = r#"
variables {
    global:
        0: e
        1: ec
        2: exists
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create Projectile Effect(All Players(All Teams), Orb Projectile, All Players(Team 1), Vector(0, 0, 0), Forward, 0.5, Visible To Friendly To Position Direction and Size);
        Set Global Variable(e, Last Created Entity);
        Set Global Variable(ec, Entity Count);
        Set Global Variable(exists, Entity Exists(Last Created Entity));
        Destroy All Effects;
        Destroy All Icons;
    }
}
"#;
    let trace = run(text, &scenario(TWO_TEAMS));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["e"], Val::Number(0.0));
    assert_eq!(trace.globals["ec"], Val::Number(0.0));
    assert_eq!(trace.globals["exists"], Val::Bool(true));
    let [projectile] = trace.effects.as_slice() else {
        panic!("expected one entity, got {:?}", trace.effects);
    };
    assert_eq!(projectile.kind, EffectKind::ProjectileEffect);
    assert_eq!(projectile.args.effect.as_deref(), Some("ORB"));
    assert_eq!(projectile.args.friendly_to, Some(vec!["p1".to_string()]));
    assert_eq!(projectile.args.oversize, Some(Val::Number(0.5)));
    assert_eq!(projectile.destroyed_tick, None);
}

/// Every destroy hits only its own kind's ids; `Destroy Effect` covers
/// `createEffect` and `createBeamEffect` per the catalog sentence.
#[test]
fn destroys_are_scoped_to_their_kinds() {
    let text = r#"
variables {
    global:
        0: icon
        1: beam
        2: world
        3: bar
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create In-World Text(All Players(All Teams), "w", Vector(0, 0, 0), 1, Do Not Clip, None, Color(White), Default Visibility);
        Create Progress Bar In-World Text(All Players(All Teams), 25, "bar", Vector(0, 0, 0), 1, Do Not Clip, Color(Blue), Color(Red), None, Default Visibility);
        Create Effect(All Players(All Teams), Good Aura, Color(White), Vector(0, 0, 0), 2, None);
        Create Beam Effect(All Players(All Teams), Good Beam, Vector(0, 0, 0), Vector(1, 1, 1), Color(Green), None);
        Create Icon(All Players(All Teams), Vector(0, 5, 0), Warning, Visible To and Position, Color(Blue), True);
        Set Global Variable(world, Last Text ID - 1);
        Set Global Variable(bar, Last Text ID);
        Set Global Variable(icon, Last Created Entity);
        Set Global Variable(beam, Last Created Entity - 1);
        Destroy In-World Text(Global.world);
        Destroy Progress Bar In-World Text(Global.bar);
        Destroy Effect(Global.beam);
        Destroy Icon(Global.icon);
        Destroy All In-World Text;
        Destroy All Effects;
    }
}
"#;
    let trace = run(text, &scenario(TWO_TEAMS));
    assert_eq!(trace.outcome, Outcome::Complete);
    let [in_world, bar] = trace.hud.as_slice() else {
        panic!("expected two display elements, got {:?}", trace.hud);
    };
    assert_eq!(in_world.destroyed_tick, Some(0));
    assert_eq!(bar.destroyed_tick, Some(0));
    let [effect, beam, icon] = trace.effects.as_slice() else {
        panic!("expected three entities, got {:?}", trace.effects);
    };
    // The beam died to `Destroy Effect` and the icon to `Destroy Icon`;
    // `Destroy All Effects` took the `createEffect` entity and leaves a
    // played effect alone — it is already destroyed on its tick.
    assert_eq!(effect.destroyed_tick, Some(0));
    assert_eq!(beam.destroyed_tick, Some(0));
    assert_eq!(icon.destroyed_tick, Some(0));
}

/// `Entity Exists` on a `Player` answers true — the scenario roster is
/// fixed; on a live entity id true, on a destroyed or unknown id or on
/// `Null` false. The reader compares the number against the entity
/// table only — a text id is just a number, so it answers by whether
/// some live entity happens to hold that id.
#[test]
fn entity_exists_answers_by_entity_kind() {
    let text = r#"
variables {
    global:
        0: player
        1: live
        2: dead
        3: unknown
        4: null
    player:
        0: e
}

rule ("init") {
    event {
        Ongoing - Each Player;
        All;
        All;
    }
    actions {
        Create Icon(All Players(All Teams), Vector(0, 5, 0), Warning, Visible To and Position, Color(Blue), True);
        Create In-World Text(All Players(All Teams), "w", Vector(0, 0, 0), 1, Do Not Clip, None, Color(White), Default Visibility);
        Set Player Variable(Event Player, e, Last Created Entity);
        Destroy Icon(Event Player.e);
        Create Effect(All Players(All Teams), Good Aura, Color(White), Vector(0, 0, 0), 2, None);
        Set Global Variable(player, Entity Exists(Event Player));
        Set Global Variable(live, Entity Exists(Last Created Entity));
        Set Global Variable(dead, Entity Exists(Event Player.e));
        Set Global Variable(unknown, Entity Exists(100));
        Set Global Variable(null, Entity Exists(Null));
    }
}
"#;
    let trace = run(text, &scenario(TWO_TEAMS));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["player"], Val::Bool(true));
    assert_eq!(trace.globals["live"], Val::Bool(true));
    assert_eq!(trace.globals["dead"], Val::Bool(false));
    assert_eq!(trace.globals["unknown"], Val::Bool(false));
    assert_eq!(trace.globals["null"], Val::Bool(false));
}

/// A `Player|Array` `Entity Exists` argument that yields an array has no
/// answer — the catalog's `Player|EntityId` names a single entity.
#[test]
fn entity_exists_on_an_array_is_not_modeled() {
    let text = r#"
rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create Icon(All Players(All Teams), Vector(0, 5, 0), Warning, Visible To and Position, Color(Blue), True);
        Small Message(All Players(All Teams), Entity Exists(All Players(Team 1)));
    }
}
"#;
    let trace = run(text, &scenario(TWO_TEAMS));
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("expected not_modeled, got {:?}", trace.outcome);
    };
    assert!(stop.construct.contains("entityExists"), "{stop:?}");
}

/// An id reader outside the modeled families still stops the run and
/// names the construct — here `Last Created Health Pool`.
#[test]
fn an_unmodeled_id_reader_stops_the_run() {
    let text = r#"
variables {
    global:
        0: g
}

rule ("init") {
    event {
        Ongoing - Global;
    }
    actions {
        Create Icon(All Players(All Teams), Vector(0, 5, 0), Warning, Visible To and Position, Color(Blue), True);
        Set Global Variable(g, Last Created Health Pool);
    }
}
"#;
    let trace = run(text, &scenario(TWO_TEAMS));
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("expected not_modeled, got {:?}", trace.outcome);
    };
    assert!(
        stop.construct.contains("getLastCreatedHealthPool"),
        "{stop:?}"
    );
    // The icon was still recorded — the viewer sees what is on screen.
    let [icon] = trace.effects.as_slice() else {
        panic!("expected one entity, got {:?}", trace.effects);
    };
    assert_eq!(icon.kind, EffectKind::Icon);
}
