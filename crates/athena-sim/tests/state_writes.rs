//! The per-player state cells and stored modifiers (athena#116): the
//! state-write actions write a `state` key of the player — the cell
//! lands even on a key the scenario never supplied, a later read
//! returns it, and the write is a `state` change naming the player and
//! the key. The stored-modifier actions record a `modifier` change
//! with the action, the player and the evaluated arguments while no
//! read changes. Everything else the actions would move — damage,
//! healing, movement, physics — is not simulated.

use athena_sim::{Change, MissingInputKind, Outcome, Val, run_scenario};

fn run(text: &str, scenario: &str) -> athena_sim::Trace {
    run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"))
}

/// One `Ongoing - Global` rule running `actions`.
fn program(actions: &str) -> String {
    format!(
        r#"
variables {{
    global:
        0: g
    player:
        0: streak
}}

rule ("probe") {{
    event {{
        Ongoing - Global;
    }}
    actions {{
        {actions}
    }}
}}
"#
    )
}

/// The scenario host player with no `state` map — `team1`, so the
/// team-targeted actions reach it.
const PLAYER: &str = r#"{ "id": "p1", "host": true, "hero": "ana", "team": "team1" }"#;

fn scenario(players: &str) -> String {
    format!(r#"{{ "scenarioVersion": 2, "seconds": 1, "players": [{players}] }}"#)
}

/// A player with `health`, `maxHealth` and `isDead` — the keys the
/// health actions read.
const HEALTHY: &str = r#"{ "id": "p1", "host": true, "hero": "ana",
    "state": { "health": 175, "maxHealth": 200, "isDead": false } }"#;

/// A state write lands on a key the scenario never supplied, and the
/// read after the write returns it.
#[test]
fn a_state_write_defines_the_key_for_later_reads() {
    let trace = run(
        &program(
            "Set Ultimate Charge(Host Player, 42);
             Set Global Variable(g, Ultimate Charge Percent(Host Player));",
        ),
        &scenario(PLAYER),
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["g"], Val::number(42.0));
    assert_eq!(trace.players["p1"].state["ultCharge"], Val::number(42.0));
}

/// `True`/`False` coerce to `1`/`0` under the `Number|Boolean`
/// parameter type.
#[test]
fn set_ultimate_charge_accepts_the_boolean_coercion() {
    let trace = run(
        &program(
            "Set Ultimate Charge(Host Player, False);
             Set Global Variable(g, Ultimate Charge Percent(Host Player));",
        ),
        &scenario(PLAYER),
    );
    assert_eq!(trace.globals["g"], Val::number(0.0));
}

/// A read of a key nothing wrote and the scenario does not supply is
/// still the `missing_input` the scenario format promises.
#[test]
fn an_unwritten_key_is_a_missing_input() {
    let trace = run(
        &program("Set Global Variable(g, Ultimate Charge Percent(Host Player));"),
        &scenario(PLAYER),
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let missing = &trace.missing_inputs[0];
    assert_eq!(missing.input, MissingInputKind::State);
    assert_eq!(missing.name.as_deref(), Some("state.ultCharge"));
}

/// The write is a `state` change naming the player and the key, with
/// the value the key read before — the declared `state` entry.
#[test]
fn a_state_write_is_a_change_naming_player_and_key() {
    let trace = run(
        &program("Set Ultimate Charge(Host Player, 40);"),
        &scenario(r#"{ "id": "p1", "host": true, "hero": "ana", "state": { "ultCharge": 12 } }"#),
    );
    let change = trace.timeline.entries[0]
        .changes
        .iter()
        .find_map(|change| match change {
            Change::State {
                player,
                key,
                old,
                new,
            } => Some((player, key, old, new)),
            _ => None,
        })
        .expect("a state change");
    assert_eq!(*change.0, "p1");
    assert_eq!(*change.1, "ultCharge");
    assert_eq!(*change.2, Val::number(12.0));
    assert_eq!(*change.3, Val::number(40.0));
}

/// A write on a key nothing answered records `old: null`.
#[test]
fn a_state_write_of_an_unknown_key_reports_null_as_the_old_value() {
    let trace = run(
        &program("Set Ultimate Charge(Host Player, 40);"),
        &scenario(PLAYER),
    );
    let change = trace.timeline.entries[0]
        .changes
        .iter()
        .find(|change| matches!(change, Change::State { key, .. } if key == "ultCharge"))
        .expect("a state change");
    let Change::State { old, .. } = change else {
        unreachable!()
    };
    assert_eq!(*old, Val::Null);
}

/// `Set Player Health` writes `health`; the catalog's "only has an
/// effect on living players" gates it on `isDead`.
#[test]
fn set_health_writes_living_players_only() {
    let trace = run(
        &program("Set Player Health(Host Player, 80);"),
        &scenario(HEALTHY),
    );
    assert_eq!(trace.players["p1"].state["health"], Val::number(80.0));
    let dead = scenario(
        r#"{ "id": "p1", "host": true, "hero": "ana",
            "state": { "health": 175, "maxHealth": 200, "isDead": true } }"#,
    );
    let trace = run(&program("Set Player Health(Host Player, 80);"), &dead);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.players["p1"].state["health"], Val::number(175.0));
    assert!(
        !trace.timeline.entries[0]
            .changes
            .iter()
            .any(|change| matches!(change, Change::State { key, .. } if key == "health"))
    );
}

/// `Set Max Health` writes `maxHealth` as a percentage of the current
/// max and clamps `health` to it.
#[test]
fn set_max_health_scales_and_clamps_health() {
    let trace = run(
        &program("Set Max Health(Host Player, 50);"),
        &scenario(HEALTHY),
    );
    assert_eq!(trace.players["p1"].state["maxHealth"], Val::number(100.0));
    assert_eq!(trace.players["p1"].state["health"], Val::number(100.0));
    let trace = run(
        &program("Set Max Health(Host Player, 50);"),
        &scenario(
            r#"{ "id": "p1", "host": true, "hero": "ana",
                "state": { "health": 50, "maxHealth": 200, "isDead": false } }"#,
        ),
    );
    assert_eq!(trace.players["p1"].state["health"], Val::number(50.0));
}

/// `Set Max Health` scales the max by the percent: a product that
/// overflows to `inf` stops `not_modeled` instead of writing a value
/// that serializes `null` (athena#133).
#[test]
fn set_max_health_stops_on_a_non_finite_result() {
    let trace = run(
        &program("Set Max Health(Host Player, Raise To Power(10, 308));"),
        &scenario(HEALTHY),
    );
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        stop.construct,
        "action setMaxHealth produced a non-finite number"
    );
}

/// `Kill` writes `isDead`; `Respawn` clears it and restores `health`
/// to `maxHealth`; `Resurrect` clears it alone.
#[test]
fn kill_respawn_and_resurrect_write_is_dead() {
    let dead = run(&program("Kill(Host Player, Null);"), &scenario(HEALTHY));
    assert_eq!(dead.players["p1"].state["isDead"], Val::Bool(true));
    let trace = run(&program("Respawn(Host Player);"), &dead_scenario());
    assert_eq!(trace.players["p1"].state["isDead"], Val::Bool(false));
    assert_eq!(trace.players["p1"].state["health"], Val::number(200.0));
    let trace = run(&program("Resurrect(Host Player);"), &dead_scenario());
    assert_eq!(trace.players["p1"].state["isDead"], Val::Bool(false));
    assert_eq!(trace.players["p1"].state["health"], Val::number(0.0));
}

fn dead_scenario() -> String {
    scenario(
        r#"{ "id": "p1", "host": true, "hero": "ana",
            "state": { "health": 0, "maxHealth": 200, "isDead": true } }"#,
    )
}

/// `Is Alive` answers the `isDead` cell `Is Dead` reads — the catalog's
/// complement — and falls back to the hero model when `isDead` cannot
/// answer. `Has Spawned` stays hero-based: a kill does not unspawn.
#[test]
fn is_alive_reads_the_is_dead_cell() {
    let trace = run(
        &program(
            "Kill(Host Player, Null);
             Set Global Variable(g, Is Alive(Host Player));",
        ),
        &scenario(PLAYER),
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["g"], Val::Bool(false));
    // A declared `state.isDead` answers the read without a write.
    let trace = run(
        &program("Set Global Variable(g, Is Alive(Host Player));"),
        &dead_scenario(),
    );
    assert_eq!(trace.globals["g"], Val::Bool(false));
    // An unanswerable `isDead` keeps the hero model.
    let trace = run(
        &program("Set Global Variable(g, Is Alive(Host Player));"),
        &scenario(r#"{ "id": "p1", "host": true, "hero": "ana", "state": { "health": 200 } }"#),
    );
    assert_eq!(trace.globals["g"], Val::Bool(true));
    let trace = run(
        &program(
            "Kill(Host Player, Null);
             Set Global Variable(g, Has Spawned(Host Player));",
        ),
        &scenario(PLAYER),
    );
    assert_eq!(trace.globals["g"], Val::Bool(true));
}

/// `Set Facing` writes the scenario `facing` the facing reads derive —
/// `To World` names the direction's facing outright.
#[test]
fn set_facing_writes_the_scenario_facing() {
    let trace = run(
        &program(
            "Set Facing(Host Player, Vector(0, 0, 1), To World);
             Set Global Variable(g, Horizontal Angle From Direction(Facing Direction Of(Host Player)));",
        ),
        &scenario(PLAYER),
    );
    assert_eq!(trace.globals["g"], Val::number(0.0));
    let change = trace.timeline.entries[0]
        .changes
        .iter()
        .find(|change| matches!(change, Change::World { field, .. } if field == "facing"))
        .expect("a facing change");
    assert!(matches!(change, Change::World { old: Val::Null, .. }));
}

/// The catalog does not settle how `Set Facing` composes `To Player`,
/// so it is `not_modeled`.
#[test]
fn set_facing_to_player_is_not_modeled() {
    let trace = run(
        &program("Set Facing(Host Player, Forward, To Player);"),
        &scenario(PLAYER),
    );
    assert!(matches!(trace.outcome, Outcome::NotModeled(_)));
}

/// The stored-modifier actions record a `modifier` change — action,
/// player, evaluated arguments — and store the modifier on the player.
/// No read changes.
#[test]
fn every_stored_modifier_records_its_effect() {
    // (action call, catalog id, modifier key, the stored value, the
    // evaluated non-target arguments).
    let cases: &[(&str, &str, &str, Val, Vec<Val>)] = &[
        (
            "Set Ultimate Ability Enabled(Host Player, False)",
            "setUltEnabled",
            "ultEnabled",
            Val::Bool(false),
            vec![Val::Bool(false)],
        ),
        (
            "Set Ability 1 Enabled(Host Player, False)",
            "setAbility1Enabled",
            "ability1Enabled",
            Val::Bool(false),
            vec![Val::Bool(false)],
        ),
        (
            "Set Ability 2 Enabled(Host Player, False)",
            "setAbility2Enabled",
            "ability2Enabled",
            Val::Bool(false),
            vec![Val::Bool(false)],
        ),
        (
            "Set Damage Dealt(Host Player, 20)",
            "setDamageDealt",
            "damageDealt",
            Val::number(20.0),
            vec![Val::number(20.0)],
        ),
        (
            "Set Damage Received(Host Player, 20)",
            "setDamageReceived",
            "damageReceived",
            Val::number(20.0),
            vec![Val::number(20.0)],
        ),
        (
            "Set Healing Dealt(Host Player, 20)",
            "setHealingDealt",
            "healingDealt",
            Val::number(20.0),
            vec![Val::number(20.0)],
        ),
        (
            "Set Healing Received(Host Player, 20)",
            "setHealingReceived",
            "healingReceived",
            Val::number(20.0),
            vec![Val::number(20.0)],
        ),
        (
            "Set Move Speed(Host Player, 20)",
            "setMoveSpeed",
            "moveSpeed",
            Val::number(20.0),
            vec![Val::number(20.0)],
        ),
        (
            "Set Gravity(Host Player, 20)",
            "setGravity",
            "gravity",
            Val::number(20.0),
            vec![Val::number(20.0)],
        ),
        (
            "Set Knockback Received(Host Player, 20)",
            "setKnockbackReceived",
            "knockbackReceived",
            Val::number(20.0),
            vec![Val::number(20.0)],
        ),
        (
            "Set Player Allowed Heroes(Host Player, Array(Hero(Ana)))",
            "setAllowedHeroes",
            "allowedHeroes",
            Val::Array(vec![Val::Text("ANA".into())]),
            vec![Val::Array(vec![Val::Text("ANA".into())])],
        ),
        (
            "Start Forcing Player To Be Hero(Host Player, Hero(Ana))",
            "startForcingHero",
            "forcedHero",
            Val::Text("ANA".into()),
            vec![Val::Text("ANA".into())],
        ),
        (
            "Start Forcing Spawn Room(Team 1, 0)",
            "startForcingSpawn",
            "forcedSpawnRoom",
            Val::number(0.0),
            vec![Val::number(0.0)],
        ),
    ];
    for (call, id, key, stored, args) in cases {
        let trace = run(&program(&format!("{call};")), &scenario(PLAYER));
        assert_eq!(trace.outcome, Outcome::Complete, "{call}");
        assert_eq!(
            trace.players["p1"].modifiers.get(*key),
            Some(stored),
            "{call}"
        );
        let change = trace.timeline.entries[0]
            .changes
            .iter()
            .find_map(|change| match change {
                Change::Modifier {
                    player,
                    action,
                    args: recorded,
                } => Some((player, action, recorded)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{call} records a modifier change"));
        assert_eq!(*change.0, "p1", "{call}");
        assert_eq!(change.1.as_str(), *id, "{call}");
        assert_eq!(*change.2, *args, "{call}");
        // No `state` cell and no world value moved.
        assert!(
            !trace.timeline.entries[0]
                .changes
                .iter()
                .any(|change| matches!(change, Change::State { .. } | Change::World { .. })),
            "{call}"
        );
    }
}

/// `Stop Forcing Player To Be Hero` removes the `forcedHero` modifier.
#[test]
fn stop_forcing_current_hero_clears_the_modifier() {
    let trace = run(
        &program(
            "Start Forcing Player To Be Hero(Host Player, Hero(Ana));
             Stop Forcing Player To Be Hero(Host Player);",
        ),
        &scenario(PLAYER),
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(!trace.players["p1"].modifiers.contains_key("forcedHero"));
    assert_eq!(
        trace.timeline.entries[0]
            .changes
            .iter()
            .filter(|change| matches!(change, Change::Modifier { .. }))
            .count(),
        2
    );
}

/// `Start Forcing Spawn Room` targets a team: every player of the
/// team records the modifier.
#[test]
fn start_forcing_spawn_targets_the_team() {
    let trace = run(
        &program("Start Forcing Spawn Room(Team 2, 0);"),
        &scenario(
            r#"{ "id": "p1", "host": true, "hero": "ana", "team": "team1" },
               { "id": "p2", "hero": "ana", "team": "team2" }"#,
        ),
    );
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(
        !trace.players["p1"]
            .modifiers
            .contains_key("forcedSpawnRoom")
    );
    assert_eq!(
        trace.players["p2"].modifiers["forcedSpawnRoom"],
        Val::number(0.0)
    );
}

/// A read still answers the same after a stored modifier: the modifier
/// changes no readable value.
#[test]
fn a_stored_modifier_changes_no_read() {
    let trace = run(
        &program(
            "Set Ultimate Ability Enabled(Host Player, False);
             Set Global Variable(g, Ultimate Charge Percent(Host Player));",
        ),
        &scenario(r#"{ "id": "p1", "host": true, "hero": "ana", "state": { "ultCharge": 30 } }"#),
    );
    assert_eq!(trace.globals["g"], Val::number(30.0));
}

/// The class-3 actions still stop `not_modeled`: each moves a value or
/// a schedule no model covers.
#[test]
fn the_remaining_actions_are_still_not_modeled() {
    for call in [
        "Add Health Pool To Player(Host Player, Health, 100, False, False)",
        "Remove Health Pool From Player(0)",
        "Start Damage Modification(Host Player, Host Player, 2, None)",
        "Start Damage Over Time(Host Player, Host Player, 1, 10)",
        "Stop Damage Over Time(0)",
        "Stop All Damage Over Time(Host Player)",
        "Start Heal Over Time(Host Player, Host Player, 1, 10)",
        "Stop Heal Over Time(0)",
        "Stop All Heal Over Time(Host Player)",
        "Create Dummy Bot(Hero(Ana), Team 1, 0, Null, Null)",
        "Destroy All Dummy Bots",
        "Remove Player(Host Player)",
        "Press Button(Host Player, Button(Jump))",
        "Start Holding Button(Host Player, Button(Jump))",
        "Stop Holding Button(Host Player, Button(Jump))",
        "Start Forcing Throttle(Host Player, 0, 0, 0, 0, 0, 0)",
        "Stop Forcing Throttle(Host Player)",
        "Start Throttle In Direction(Host Player, Forward, 1, To World, Replace existing throttle, Direction and Magnitude)",
        "Stop Throttle In Direction(Host Player)",
        "Cancel Primary Action(Host Player)",
        "Apply Impulse(Host Player, Up, 5, To World, Cancel Contrary Motion)",
        "Start Accelerating(Host Player, Forward, 5, 10, To World, Direction Rate and Max Speed)",
        "Stop Accelerating(Host Player)",
        "Start Facing(Host Player, Forward, 1, To World, Direction and Turn Rate)",
        "Stop Facing(Host Player)",
        "Start Forcing Player Position(Host Player, Vector(0, 0, 0), False)",
        "Stop Forcing Player Position(Host Player)",
        "Attach Players(Host Player, Host Player, Vector(0, 0, 0))",
        "Detach Players(Host Player)",
        "Chase Player Variable Over Time(Host Player, streak, 1, 1, Destination and Duration)",
        "Chase Player Variable At Rate(Host Player, streak, 1, 1, Destination and Rate)",
        "Chase Global Variable Over Time(g, 1, 1, Destination and Duration)",
        "Chase Global Variable At Rate(g, 1, 1, Destination and Rate)",
        "Declare Team Victory(Team 1)",
        "Restart Match",
    ] {
        let trace = run(&program(&format!("{call};")), &scenario(PLAYER));
        assert!(
            matches!(trace.outcome, Outcome::NotModeled(_)),
            "{call}: {:?}",
            trace.outcome
        );
    }
}
