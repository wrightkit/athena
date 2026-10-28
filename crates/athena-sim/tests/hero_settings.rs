//! Hero settings applied to the player state table (athena#29): the
//! `settings.heroes` scalars the simulator applies, their scopes, and the
//! authored-but-unapplied settings that leave a player's world unresolved.

use athena_sim::{Outcome, run_scenario_with_hero_values};

/// A test-only hero-values set, loaded through `workshop_rs::hero_values::load`.
/// `ana` has all three pools, both ability cooldowns and an ultimate cost;
/// `ashe` has health and an ultimate cost only; `dva` has health but no
/// ultimate cost; every other hero (for example `baptiste`) is absent, so all
/// of its values are `None`.
fn test_values() -> workshop_rs::hero_values::HeroValues {
    workshop_rs::hero_values::load(
        r#"{
            "format": "workshop-rs/hero-values-v1",
            "balanceVersion": "test-only",
            "heroes": [
                { "id": "ana", "values": [
                    { "kind": "health", "pool": "health", "amount": 200 },
                    { "kind": "health", "pool": "shield", "amount": 100 },
                    { "kind": "health", "pool": "armor", "amount": 50 },
                    { "kind": "cooldown", "slot": "ability1", "seconds": 10 },
                    { "kind": "cooldown", "slot": "ability2", "seconds": 12 },
                    { "kind": "ultimateCost", "points": 2100 }
                ] },
                { "id": "ashe", "values": [
                    { "kind": "health", "pool": "health", "amount": 200 },
                    { "kind": "ultimateCost", "points": 2400 }
                ] },
                { "id": "dva", "values": [
                    { "kind": "health", "pool": "health", "amount": 650 }
                ] }
            ]
        }"#,
    )
    .expect("the test hero values load")
}

/// A program with a `settings.heroes` block and a rule whose actions are
/// given verbatim; the event player triggers them.
fn hero_settings_program(heroes: &str, actions: &str) -> String {
    format!(
        r#"
settings {{
    heroes {{
{heroes}
    }}
}}

rule ("act") {{
    event {{
        Player Joined Match;
        All;
        All;
    }}
    actions {{
{actions}
    }}
}}
"#
    )
}

/// A scenario with the given players and one join event on `event_player`.
fn hero_settings_scenario(players: &str, event_player: &str) -> String {
    format!(
        r#"{{ "scenarioVersion": 1, "seconds": 1,
            "players": [{players}],
            "events": [{{ "kind": "playerJoined", "time": 0, "eventPlayer": "{event_player}" }}] }}"#
    )
}

/// A version-2 scenario with the given players, where `state` is the
/// player-state map (athena#91 tests the version-2 meaning).
fn hero_settings_scenario_v2(players: &str, event_player: &str) -> String {
    format!(
        r#"{{ "scenarioVersion": 2, "seconds": 1,
            "players": [{players}],
            "events": [{{ "kind": "playerJoined", "time": 0, "eventPlayer": "{event_player}" }}] }}"#
    )
}

/// A version-2 player row declaring `state` keys.
fn state_player(id: &str, hero: &str, team: &str, state: &str) -> String {
    format!(r#"{{ "id": "{id}", "hero": "{hero}", "team": "{team}", "state": {state} }}"#)
}

#[test]
fn health_percent_scales_every_pool_for_all_teams() {
    let text = hero_settings_program(
        r#"        General {
            health%: 150
        }"#,
        "        Wait(0, Ignore Condition);",
    );
    let scenario = hero_settings_scenario(
        r#"{ "id": "p1", "hero": "ana", "team": "team1" },
           { "id": "p2", "hero": "ashe", "team": "team2" }"#,
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    // ana: 200 health, 100 shield, 50 armor at 150%.
    let world = trace.players["p1"].world.as_ref().expect("p1 world");
    assert_eq!(
        (world.health.current.get(), world.health.max.get()),
        (300.0, 300.0)
    );
    assert_eq!(
        (
            world.shield.as_ref().unwrap().current.get(),
            world.shield.as_ref().unwrap().max.get()
        ),
        (150.0, 150.0)
    );
    assert_eq!(
        (
            world.armor.as_ref().unwrap().current.get(),
            world.armor.as_ref().unwrap().max.get()
        ),
        (75.0, 75.0)
    );
    // ashe is on the other team but `allTeams` still applies.
    let world = trace.players["p2"].world.as_ref().expect("p2 world");
    assert_eq!(
        (world.health.current.get(), world.health.max.get()),
        (300.0, 300.0)
    );
}

#[test]
fn a_team_scoped_setting_applies_only_to_that_team() {
    let text = hero_settings_program(
        r#"        Team 1 {
            health%: 150
        }"#,
        "        Wait(0, Ignore Condition);",
    );
    let scenario = hero_settings_scenario(
        r#"{ "id": "p1", "hero": "ana", "team": "team1" },
           { "id": "p2", "hero": "ana", "team": "team2" }"#,
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        trace.players["p1"].world.as_ref().unwrap().health.max.get(),
        300.0
    );
    assert_eq!(
        trace.players["p2"].world.as_ref().unwrap().health.max.get(),
        200.0
    );
}

#[test]
fn a_hero_and_team_setting_requires_both() {
    let text = hero_settings_program(
        r#"        Team 1 {
            Ana {
                health%: 150
            }
        }
        General {
            Ashe {
                health%: 50
            }
        }"#,
        "        Wait(0, Ignore Condition);",
    );
    let scenario = hero_settings_scenario(
        r#"{ "id": "p1", "hero": "ana", "team": "team1" },
           { "id": "p2", "hero": "ana", "team": "team2" },
           { "id": "p3", "hero": "ashe", "team": "team2" },
           { "id": "p4", "hero": "ashe", "team": "team1" }"#,
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let max = |id: &str| trace.players[id].world.as_ref().unwrap().health.max.get();
    // `Team 1 / Ana` applies to p1 only; `General / Ashe` applies to both
    // ashe players regardless of team.
    assert_eq!(max("p1"), 300.0);
    assert_eq!(max("p2"), 200.0);
    assert_eq!(max("p3"), 100.0);
    assert_eq!(max("p4"), 100.0);
}

#[test]
fn damage_dealt_and_received_scale_the_damage_amount() {
    let text = hero_settings_program(
        r#"        Team 1 {
            Ana {
                damageDealt%: 200
            }
        }
        Team 2 {
            Ashe {
                damageReceived%: 50
            }
        }"#,
        r#"        Damage(Value In Array(All Players(Team 2), 0), Event Player, 100);
        Damage(Event Player, Value In Array(All Players(Team 2), 0), 40);"#,
    );
    let scenario = hero_settings_scenario(
        r#"{ "id": "p1", "hero": "ana", "team": "team1" },
           { "id": "p2", "hero": "ashe", "team": "team2" }"#,
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    // p1 ana deals 100 at 200% to p2 ashe at 50% received: 100 lands.
    let p2 = trace.players["p2"].world.as_ref().unwrap();
    assert_eq!(
        (p2.health.current.get(), p2.health.max.get()),
        (100.0, 200.0)
    );
    // p2 ashe deals 40 with no `damageDealt%` to p1 ana with no
    // `damageReceived%`: 40 lands on the shield pool.
    let p1 = trace.players["p1"].world.as_ref().unwrap();
    assert_eq!(p1.shield.as_ref().unwrap().current.get(), 60.0);
    assert_eq!(p1.health.current.get(), 200.0);
}

#[test]
fn healing_dealt_and_received_scale_the_heal_amount() {
    let text = hero_settings_program(
        r#"        Team 1 {
            Ana {
                healingDealt%: 150
            }
        }
        Team 2 {
            Ashe {
                healingReceived%: 50
            }
        }"#,
        r#"        Damage(Event Player, Null, 300);
        Damage(Value In Array(All Players(Team 2), 0), Null, 150);
        Heal(Value In Array(All Players(Team 2), 0), Event Player, 100);
        Heal(Event Player, Event Player, 40);"#,
    );
    let scenario = hero_settings_scenario(
        r#"{ "id": "p1", "hero": "ana", "team": "team1" },
           { "id": "p2", "hero": "ashe", "team": "team2" }"#,
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    // The 100 heal is dealt by ana at 150% and received by ashe at 50%:
    // 75 on top of the 50 left by the 150 damage.
    let p2 = trace.players["p2"].world.as_ref().unwrap();
    assert_eq!(p2.health.current.get(), 125.0);
    // ana took 300 through shield and armor down to 50 health; the 40 heal
    // dealt by ana lands at 150%: 110.
    let p1 = trace.players["p1"].world.as_ref().unwrap();
    assert_eq!(p1.health.current.get(), 110.0);
}

#[test]
fn ability_cooldown_scales_stack_across_scopes() {
    let text = hero_settings_program(
        r#"        General {
            Ana {
                ability1Cooldown%: 50
            }
        }
        Team 1 {
            abilityCooldown%: 50
        }"#,
        "        Wait(0, Ignore Condition);",
    );
    let scenario = hero_settings_scenario(
        r#"{ "id": "p1", "hero": "ana", "team": "team1" },
           { "id": "p2", "hero": "ana", "team": "team2" }"#,
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let cooldowns = &trace.players["p1"].world.as_ref().unwrap().cooldowns;
    // p1 ana (team1): team `abilityCooldown%` halves every slot and the
    // hero `ability1Cooldown%` halves ability1 again.
    assert_eq!(cooldowns["ability1"].get(), 2.5);
    assert_eq!(cooldowns["ability2"].get(), 6.0);
    // p2 ana (team2): only the hero-scope scalar applies.
    let cooldowns = &trace.players["p2"].world.as_ref().unwrap().cooldowns;
    assert_eq!(cooldowns["ability1"].get(), 5.0);
    assert_eq!(cooldowns["ability2"].get(), 12.0);
}

#[test]
fn an_unapplied_hero_setting_is_not_modeled_when_touched() {
    let text = hero_settings_program(
        r#"        General {
            ultGen%: 200
        }"#,
        "        Damage(Event Player, Null, 10);",
    );
    let scenario =
        hero_settings_scenario(r#"{ "id": "p1", "hero": "ana", "team": "team1" }"#, "p1");
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(
        not_modeled.construct.contains("ultGen%"),
        "{}",
        not_modeled.construct
    );
    assert!(not_modeled.construct.contains("p1"));
    assert_eq!(trace.players["p1"].world, None);
}

#[test]
fn an_unapplied_setting_on_an_untouched_player_still_completes() {
    let text = hero_settings_program(
        r#"        General {
            Ashe {
                passiveHealthRegen: Off
            }
        }"#,
        "        Damage(Event Player, Null, 10);",
    );
    let scenario = hero_settings_scenario(
        r#"{ "id": "p1", "hero": "ana", "team": "team1" },
           { "id": "p2", "hero": "ashe", "team": "team2" }"#,
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    // The authored `passiveHealthRegen` fails p2's world resolution, but no
    // `Damage`/`Heal` touches p2: the run completes and the row is null.
    assert_eq!(trace.outcome, Outcome::Complete);
    assert!(trace.players["p1"].world.is_some());
    assert_eq!(trace.players["p2"].world, None);
}

/// The ultimate-charge settings map to `state.ultCharge`: a player whose
/// `state` declares it skips the setting — the scenario-supplied value
/// stands, the setting is not simulated (athena#91).
#[test]
fn an_unapplied_ult_setting_is_skipped_when_state_supplies_the_charge() {
    for setting in [
        "ultGen%: 0",
        "passiveUltGen%: 0",
        "combatUltGen%: 0",
        "enableSpawningWithUlt: On",
    ] {
        let text = hero_settings_program(
            &format!("        General {{\n            {setting}\n        }}"),
            "        Damage(Event Player, Null, 10);",
        );
        let scenario = hero_settings_scenario_v2(
            &state_player("p1", "ana", "team1", r#"{ "ultCharge": 30 }"#),
            "p1",
        );
        let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
            .unwrap_or_else(|error| panic!("{setting}: {error}"));
        assert_eq!(trace.outcome, Outcome::Complete, "{setting}");
        // The world resolved: `Damage` lands on ana's shield pool.
        let world = trace.players["p1"].world.as_ref().expect("p1 world");
        assert_eq!(
            world.shield.as_ref().unwrap().current.get(),
            90.0,
            "{setting}"
        );
    }
}

/// The health settings map to `health`, `maxHealth` and
/// `normalizedHealth`: all three must be declared for the skip.
#[test]
fn an_unapplied_health_setting_needs_every_mapped_state_key() {
    let text = hero_settings_program(
        r#"        General {
            passiveHealthRegen: Off
            enableRolePassive: Off
        }"#,
        "        Damage(Event Player, Null, 10);",
    );
    let scenario = hero_settings_scenario_v2(
        &state_player(
            "p1",
            "ana",
            "team1",
            r#"{ "health": 200, "maxHealth": 200, "normalizedHealth": 1 }"#,
        ),
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(
        trace.players["p1"]
            .world
            .as_ref()
            .unwrap()
            .shield
            .as_ref()
            .unwrap()
            .current
            .get(),
        90.0
    );

    // One mapped key missing: the setting fails as before, and the
    // message names the `state` keys that would clear it.
    let scenario = hero_settings_scenario_v2(
        &state_player(
            "p1",
            "ana",
            "team1",
            r#"{ "health": 200, "maxHealth": 200 }"#,
        ),
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(
        not_modeled.construct.contains("passiveHealthRegen"),
        "{}",
        not_modeled.construct
    );
    assert!(
        not_modeled.construct.contains("state.normalizedHealth"),
        "{}",
        not_modeled.construct
    );
}

/// A player without `state` gets no skip: the failure names every `state`
/// key that would clear the setting.
#[test]
fn an_unapplied_setting_without_state_fails_naming_the_state_keys() {
    let text = hero_settings_program(
        r#"        General {
            combatUltGen%: 0
        }"#,
        "        Damage(Event Player, Null, 10);",
    );
    let scenario =
        hero_settings_scenario_v2(r#"{ "id": "p1", "hero": "ana", "team": "team1" }"#, "p1");
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(
        not_modeled.construct.contains("state.ultCharge"),
        "{}",
        not_modeled.construct
    );
}

/// A setting with no `state` coverage — `enableHeadshotsOnly`,
/// `spawnWithoutMech`, `shieldBashCooldown%` — fails exactly as before
/// even when `state` is fully declared; the message names no `state` key.
#[test]
fn an_unmapped_setting_still_fails_with_state() {
    let text = hero_settings_program(
        r#"        General {
            enableHeadshotsOnly: On
        }"#,
        "        Damage(Event Player, Null, 10);",
    );
    let scenario = hero_settings_scenario_v2(
        &state_player(
            "p1",
            "ana",
            "team1",
            r#"{ "health": 200, "maxHealth": 200, "normalizedHealth": 1, "ultCharge": 30 }"#,
        ),
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(
        not_modeled.construct.contains("enableHeadshotsOnly"),
        "{}",
        not_modeled.construct
    );
    assert!(
        !not_modeled.construct.contains("state."),
        "{}",
        not_modeled.construct
    );
}

/// An applied setting still applies for a player with `state`: `health%`
/// keeps scaling the pools — `state.<key>` governs reads, not the table.
#[test]
fn an_applied_setting_still_scales_with_state_declared() {
    let text = hero_settings_program(
        r#"        General {
            health%: 150
        }"#,
        "        Damage(Event Player, Null, 10);",
    );
    let scenario = hero_settings_scenario_v2(
        &state_player(
            "p1",
            "ana",
            "team1",
            r#"{ "health": 200, "maxHealth": 200, "normalizedHealth": 1 }"#,
        ),
        "p1",
    );
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    // 150% pools; the 10 damage lands on the shield pool.
    let world = trace.players["p1"].world.as_ref().expect("p1 world");
    assert_eq!(
        (world.health.current.get(), world.health.max.get()),
        (300.0, 300.0)
    );
    assert_eq!(world.shield.as_ref().unwrap().current.get(), 140.0);
}

/// A `health%` literal so large it saturates to `inf` makes the pool
/// scaling overflow: the player's world resolution fails `not_modeled`,
/// and touching it stops the run naming the conversion (athena#133).
#[test]
fn an_overflowing_health_percent_is_not_modeled_when_touched() {
    let big = "9".repeat(320);
    let text = hero_settings_program(
        &format!("        General {{\n            health%: {big}\n        }}"),
        "        Damage(Event Player, Null, 10);",
    );
    let scenario =
        hero_settings_scenario(r#"{ "id": "p1", "hero": "ana", "team": "team1" }"#, "p1");
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(
        not_modeled
            .construct
            .contains("hero settings produced a non-finite number"),
        "{}",
        not_modeled.construct
    );
}

/// Same for the cooldown setter: an `abilityCooldown%` literal that
/// saturates to `inf` overflows the `seconds * scale` write into the
/// player's `cooldowns` (athena#133).
#[test]
fn an_overflowing_cooldown_percent_is_not_modeled_when_touched() {
    let big = "9".repeat(320);
    let text = hero_settings_program(
        &format!("        Team 1 {{\n            abilityCooldown%: {big}\n        }}"),
        "        Damage(Event Player, Null, 10);",
    );
    let scenario =
        hero_settings_scenario(r#"{ "id": "p1", "hero": "ana", "team": "team1" }"#, "p1");
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(
        not_modeled
            .construct
            .contains("hero settings produced a non-finite number"),
        "{}",
        not_modeled.construct
    );
}

/// A finite `Damage` amount that overflows once the hero-setting scales
/// are applied stops `not_modeled` naming the action — the `applied`
/// amount bound into `Event Damage` is a produced number (athena#133).
#[test]
fn a_damage_amount_that_overflows_the_scales_is_not_modeled() {
    let text = hero_settings_program(
        r#"        Team 1 {
            Ana {
                damageReceived%: 200
            }
        }"#,
        "        Damage(Event Player, Null, Raise To Power(10, 308));",
    );
    let scenario =
        hero_settings_scenario(r#"{ "id": "p1", "hero": "ana", "team": "team1" }"#, "p1");
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "action damage produced a non-finite number"
    );
}

/// Same for `Heal` through `healingReceived%` — the amount bound into
/// `Event Healing` is checked the same way (athena#133).
#[test]
fn a_heal_amount_that_overflows_the_scales_is_not_modeled() {
    let text = hero_settings_program(
        r#"        Team 1 {
            Ana {
                healingReceived%: 200
            }
        }"#,
        "        Heal(Event Player, Null, Raise To Power(10, 308));",
    );
    let scenario =
        hero_settings_scenario(r#"{ "id": "p1", "hero": "ana", "team": "team1" }"#, "p1");
    let trace = run_scenario_with_hero_values(&text, &scenario, &test_values())
        .unwrap_or_else(|error| panic!("{error}"));
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert_eq!(
        not_modeled.construct,
        "action heal produced a non-finite number"
    );
}
