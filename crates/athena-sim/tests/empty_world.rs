//! The empty-world geometry model (athena#77): the run has no map
//! geometry, so a raycast never hits, line of sight always holds and
//! every position is walkable, on the ground and on no wall.
//! `Teleport` writes the scenario `position`; `Apply Impulse` needs a
//! velocity the model does not carry and stays `not_modeled`.
//! All empty world, modeled not measured.

use athena_sim::{Change, MissingInputKind, Options, Outcome, Val, run_scenario, run_text};

/// One `Ongoing - Global` rule that writes `g` once at tick 0.
fn set_global(value: &str) -> String {
    format!(
        r#"
variables {{
    global:
        0: g
}}

rule ("probe") {{
    event {{
        Ongoing - Global;
    }}
    actions {{
        Set Global Variable(g, {value});
    }}
}}
"#
    )
}

fn run(text: &str) -> athena_sim::Trace {
    run_text(text, Options { seconds: 1.0 }).unwrap_or_else(|error| panic!("{error}"))
}

fn run_with(text: &str, players: &str) -> athena_sim::Trace {
    let scenario = format!(r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [{players}] }}"#);
    run_scenario(text, scenario.as_str()).unwrap_or_else(|error| panic!("{error}"))
}

fn global(trace: &athena_sim::Trace) -> &Val {
    assert_eq!(trace.outcome, Outcome::Complete);
    &trace.globals["g"]
}

#[track_caller]
fn assert_not_modeled(trace: &athena_sim::Trace, construct: &str) {
    match &trace.outcome {
        Outcome::NotModeled(stop) => assert_eq!(stop.construct, construct),
        other => panic!("expected not_modeled, got {other:?}"),
    }
}

const HOST: &str = r#"{ "id": "p1", "host": true, "position": { "x": 0, "y": 0, "z": 0 }, "facing": { "horizontal": 0, "vertical": 0 } }"#;

#[test]
fn ray_cast_hit_position_returns_the_end_point() {
    let trace = run(&set_global(
        "Ray Cast Hit Position(Vector(0, 0, 0), Vector(1, 2, 3), Null, Null, False)",
    ));
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 1.0,
            y: 2.0,
            z: 3.0
        }
    );
}

#[test]
fn ray_cast_hit_player_returns_null() {
    let trace = run_with(
        &set_global(
            "Ray Cast Hit Player(Position Of(Host Player), Vector(1, 0, 0), All Players(All Teams), Null, False)",
        ),
        HOST,
    );
    assert_eq!(global(&trace), &Val::Null);
}

#[test]
fn ray_cast_hit_normal_points_from_the_end_back_to_the_start() {
    // The catalog's no-hit form: the direction from end pos to start
    // pos, not the zero vector the issue first guessed.
    let trace = run(&set_global(
        "Ray Cast Hit Normal(Vector(1, 0, 0), Vector(0, 0, 0), Null, Null, False)",
    ));
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 1.0,
            y: 0.0,
            z: 0.0
        }
    );
    let trace = run(&set_global(
        "Ray Cast Hit Normal(Vector(3, 0, 4), Vector(0, 0, 0), Null, Null, False)",
    ));
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 0.6,
            y: 0.0,
            z: 0.8
        }
    );
    // A zero-length cast has no direction; the client's answer is
    // unmeasured.
    let trace = run(&set_global(
        "Ray Cast Hit Normal(Vector(1, 1, 1), Vector(1, 1, 1), Null, Null, False)",
    ));
    assert_not_modeled(&trace, "raycastHitNormal between identical positions");
}

#[test]
fn is_in_line_of_sight_is_true() {
    for value in [
        "Is In Line of Sight(Vector(0, 0, 0), Vector(1, 0, 0), All Barriers Block LOS)",
        "Is In Line of Sight(Vector(0, 0, 0), Vector(1, 0, 0), Enemy Barriers Block LOS)",
        "Is In Line of Sight(Vector(0, 0, 0), Vector(1, 0, 0), Barriers Do Not Block LOS)",
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Bool(true), "{value}");
    }
}

#[test]
fn nearest_walkable_position_returns_its_input() {
    let trace = run(&set_global("Nearest Walkable Position(Vector(4, 5, 6))"));
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 4.0,
            y: 5.0,
            z: 6.0
        }
    );
}

#[test]
fn ground_queries_answer_for_an_empty_world() {
    for (value, expected) in [
        ("Is On Ground(Host Player)", true),
        ("Is In Air(Host Player)", false),
        ("Is On Wall(Host Player)", false),
    ] {
        let trace = run_with(&set_global(value), HOST);
        assert_eq!(global(&trace), &Val::Bool(expected), "{value}");
    }
    // The answer does not depend on the player, but a player the
    // scenario did not declare is still reported, not defaulted.
    let trace = run_with(&set_global("Is On Ground(Null)"), HOST);
    assert_not_modeled(&trace, "Null where a player is needed");
}

#[test]
fn a_raycast_start_or_end_can_be_a_player_position() {
    let trace = run_with(
        &set_global("Ray Cast Hit Position(Host Player, Vector(1, 2, 3), Null, Null, False)"),
        HOST,
    );
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 1.0,
            y: 2.0,
            z: 3.0
        }
    );
    // A `Player` whose `position` the scenario omits is a missing
    // input even in an empty world.
    let trace = run_with(
        &set_global("Ray Cast Hit Position(Host Player, Vector(1, 2, 3), Null, Null, False)"),
        r#"{ "id": "p1", "host": true }"#,
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Position);
}

#[test]
fn a_player_raycast_end_resolves_two_meters_above_the_feet() {
    // The catalog: "If a player is provided, a position 2 meters above
    // the player's feet is used" — the no-hit return is that resolved
    // end position.
    let players = r#"{ "id": "p1", "host": true, "position": { "x": 5, "y": 0, "z": 10 } }"#;
    let trace = run_with(
        &set_global("Ray Cast Hit Position(Vector(0, 0, 0), Host Player, Null, Null, False)"),
        players,
    );
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 5.0,
            y: 2.0,
            z: 10.0
        }
    );
    // The normal's back-to-the-start direction follows the same shift:
    // the start (0, 0, 0) minus the resolved end (0, 2, 0).
    let trace = run_with(
        &set_global("Ray Cast Hit Normal(Vector(0, 0, 0), Host Player, Null, Null, False)"),
        r#"{ "id": "p1", "host": true, "position": { "x": 0, "y": 0, "z": 0 } }"#,
    );
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 0.0,
            y: -1.0,
            z: 0.0
        }
    );
}

#[test]
fn teleport_writes_the_position_and_records_a_world_change() {
    let text = r#"
variables {
    global:
        0: g
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Teleport(Host Player, Vector(1, 2, 3));
        Set Global Variable(g, Position Of(Host Player));
    }
}
"#;
    let trace = run_with(text, HOST);
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 1.0,
            y: 2.0,
            z: 3.0
        }
    );
    let positions: Vec<_> = trace.timeline.entries[0]
        .changes
        .iter()
        .filter(|change| matches!(change, Change::World { field, .. } if field == "position"))
        .collect();
    let [
        Change::World {
            player,
            field,
            old,
            new,
            ..
        },
    ] = positions.as_slice()
    else {
        panic!(
            "expected one position change: {:?}",
            trace.timeline.entries[0]
        );
    };
    assert_eq!(player, "p1");
    assert_eq!(field, "position");
    assert_eq!(
        *old,
        Val::Vector {
            x: 0.0,
            y: 0.0,
            z: 0.0
        }
    );
    assert_eq!(
        *new,
        Val::Vector {
            x: 1.0,
            y: 2.0,
            z: 3.0
        }
    );
}

#[test]
fn teleport_accepts_an_array_of_players_and_null_the_origin() {
    let text = r#"
variables {
    global:
        0: g
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Teleport(All Players(All Teams), Vector(1, 1, 1));
        Teleport(Host Player, Null);
        Set Global Variable(g, Position Of(Host Player));
    }
}
"#;
    let players =
        format!("{HOST}, {{ \"id\": \"p2\", \"position\": {{ \"x\": 5, \"y\": 5, \"z\": 5 }} }}");
    let trace = run_with(text, &players);
    assert_eq!(
        global(&trace),
        &Val::Vector {
            x: 0.0,
            y: 0.0,
            z: 0.0
        }
    );
    // `Null` is the catalog's `zeroAsNull` coercion: the origin.
    // Both players got the first teleport; the host got the second.
    let positions = trace
        .timeline
        .entries
        .iter()
        .flat_map(|entry| entry.changes.iter())
        .filter(|change| matches!(change, Change::World { field, .. } if field == "position"))
        .count();
    assert_eq!(positions, 3);
}

#[test]
fn teleport_on_an_undeclared_player_is_not_modeled() {
    let text = r#"
rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Teleport(Null, Vector(1, 2, 3));
    }
}
"#;
    let trace = run_with(text, HOST);
    assert_not_modeled(&trace, "Null where a player is needed");
}

#[test]
fn apply_impulse_stays_not_modeled() {
    // A velocity change needs physics the empty world does not carry.
    let text = r#"
rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Apply Impulse(Host Player, Vector(0, 1, 0), 5, To World, Cancel Contrary Motion);
    }
}
"#;
    let trace = run_with(text, HOST);
    match &trace.outcome {
        Outcome::NotModeled(stop) => assert!(stop.construct.contains("applyImpulse")),
        other => panic!("expected not_modeled, got {other:?}"),
    }
}

#[test]
fn the_trace_marks_the_run_as_empty_world() {
    let trace = run(&set_global("1"));
    let json = serde_json::to_value(&trace).unwrap();
    assert_eq!(json["geometry"], serde_json::json!("emptyWorld"));
}
