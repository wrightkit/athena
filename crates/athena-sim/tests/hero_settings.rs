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
        Player Dealt Damage;
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

/// A scenario with the given players and one damage event on `event_player`.
fn hero_settings_scenario(players: &str, event_player: &str) -> String {
    format!(
        r#"{{ "scenarioVersion": 1, "seconds": 1,
            "players": [{players}],
            "events": [{{ "kind": "playerDealtDamage", "time": 0, "eventPlayer": "{event_player}" }}] }}"#
    )
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
    assert_eq!((world.health.current, world.health.max), (300.0, 300.0));
    assert_eq!(
        (
            world.shield.as_ref().unwrap().current,
            world.shield.as_ref().unwrap().max
        ),
        (150.0, 150.0)
    );
    assert_eq!(
        (
            world.armor.as_ref().unwrap().current,
            world.armor.as_ref().unwrap().max
        ),
        (75.0, 75.0)
    );
    // ashe is on the other team but `allTeams` still applies.
    let world = trace.players["p2"].world.as_ref().expect("p2 world");
    assert_eq!((world.health.current, world.health.max), (300.0, 300.0));
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
        trace.players["p1"].world.as_ref().unwrap().health.max,
        300.0
    );
    assert_eq!(
        trace.players["p2"].world.as_ref().unwrap().health.max,
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
    let max = |id: &str| trace.players[id].world.as_ref().unwrap().health.max;
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
    assert_eq!((p2.health.current, p2.health.max), (100.0, 200.0));
    // p2 ashe deals 40 with no `damageDealt%` to p1 ana with no
    // `damageReceived%`: 40 lands on the shield pool.
    let p1 = trace.players["p1"].world.as_ref().unwrap();
    assert_eq!(p1.shield.as_ref().unwrap().current, 60.0);
    assert_eq!(p1.health.current, 200.0);
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
    assert_eq!(p2.health.current, 125.0);
    // ana took 300 through shield and armor down to 50 health; the 40 heal
    // dealt by ana lands at 150%: 110.
    let p1 = trace.players["p1"].world.as_ref().unwrap();
    assert_eq!(p1.health.current, 110.0);
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
    assert_eq!(cooldowns["ability1"], 2.5);
    assert_eq!(cooldowns["ability2"], 6.0);
    // p2 ana (team2): only the hero-scope scalar applies.
    let cooldowns = &trace.players["p2"].world.as_ref().unwrap().cooldowns;
    assert_eq!(cooldowns["ability1"], 5.0);
    assert_eq!(cooldowns["ability2"], 12.0);
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
