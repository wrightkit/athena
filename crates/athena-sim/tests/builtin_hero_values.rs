//! `run_scenario` resolves heroes against the hero-values file
//! `workshop-rs` pins at `CURRENT_VERSION` (athena#95): the file must
//! record each hero's pools, or every world-state read stops
//! `not_modeled`.

use athena_sim::{Outcome, run_scenario};

/// One `Ongoing - Global` rule that writes `g` once at tick 0. World
/// resolution runs before the first tick, so the run carries the
/// resolved player world even when no rule reads it.
const SET_GLOBAL: &str = r#"
variables {
    global:
        0: g
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(g, 1);
    }
}
"#;

#[test]
fn the_builtin_hero_values_resolve_a_scenario_player() {
    let scenario = r#"{
        "scenarioVersion": 1,
        "seconds": 1,
        "players": [{ "id": "p1", "hero": "Reaper", "team": "team2" }]
    }"#;
    let trace = run_scenario(SET_GLOBAL, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    let world = trace.players["p1"]
        .world
        .as_ref()
        .expect("p1 has world state");
    assert!(world.health.max.get() > 0.0);
}
