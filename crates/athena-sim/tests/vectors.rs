//! Vector values and the scenario `players[]` `position`/`facing` fields
//! (athena#74): the catalog's deterministic vector math in the
//! x-left/y-up/z-forward frame, the live `position`/`facing` reads, and
//! the geometry-dependent values that stay `not_modeled`.

use athena_sim::{
    MissingInputKind, Options, Outcome, RunError, ScenarioError, Val, run_scenario, run_text,
};

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
    run_scenario(text, &scenario).unwrap_or_else(|error| panic!("{error}"))
}

fn global(trace: &athena_sim::Trace) -> &Val {
    assert_eq!(trace.outcome, Outcome::Complete);
    &trace.globals["g"]
}

fn vec(x: f64, y: f64, z: f64) -> Val {
    Val::Vector { x, y, z }
}

fn components(value: &Val) -> (f64, f64, f64) {
    match value {
        Val::Vector { x, y, z } => (*x, *y, *z),
        other => panic!("not a vector: {other:?}"),
    }
}

#[track_caller]
fn assert_close(actual: &Val, expected: (f64, f64, f64)) {
    let (x, y, z) = components(actual);
    let (ex, ey, ez) = expected;
    assert!(
        (x - ex).abs() < 1e-9 && (y - ey).abs() < 1e-9 && (z - ez).abs() < 1e-9,
        "{actual:?} != ({ex}, {ey}, {ez})"
    );
}

#[track_caller]
fn assert_not_modeled(trace: &athena_sim::Trace, construct: &str) {
    match &trace.outcome {
        Outcome::NotModeled(stop) => assert_eq!(stop.construct, construct),
        other => panic!("expected not_modeled, got {other:?}"),
    }
}

#[test]
fn vector_builds_and_its_components_read_back() {
    let trace = run(&set_global("Vector(1, 2, 3)"));
    assert_eq!(global(&trace), &vec(1.0, 2.0, 3.0));
    // Booleans coerce to 0/1 as they do in a number context.
    let trace = run(&set_global("Vector(1, True, False)"));
    assert_eq!(global(&trace), &vec(1.0, 1.0, 0.0));

    for (component, expected) in [
        ("X Component Of", 1.0),
        ("Y Component Of", 2.0),
        ("Z Component Of", 3.0),
    ] {
        let trace = run(&set_global(&format!("{component}(Vector(1, 2, 3))")));
        assert_eq!(global(&trace), &Val::Number(expected), "{component}");
    }
}

#[test]
fn vector_towards_is_the_displacement_and_direction_towards_its_unit() {
    let trace = run(&set_global(
        "Vector Towards(Vector(1, 2, 3), Vector(4, 6, 3))",
    ));
    assert_eq!(global(&trace), &vec(3.0, 4.0, 0.0));

    let trace = run(&set_global(
        "Direction Towards(Vector(1, 2, 3), Vector(4, 6, 3))",
    ));
    assert_close(global(&trace), (0.6, 0.8, 0.0));

    // Identical positions have no direction; the client's answer is
    // unmeasured.
    let trace = run(&set_global(
        "Direction Towards(Vector(1, 1, 1), Vector(1, 1, 1))",
    ));
    assert_not_modeled(&trace, "directionTowards between identical positions");
}

#[test]
fn distance_between_is_the_displacement_magnitude() {
    let trace = run(&set_global(
        "Distance Between(Vector(1, 2, 3), Vector(4, 6, 3))",
    ));
    assert_eq!(global(&trace), &Val::Number(5.0));
}

#[test]
fn direction_from_angles_maps_the_frame() {
    // Horizontal 0 is forward (+z), positive turns left (+x); positive
    // vertical looks down (-y).
    for (angles, expected) in [
        ("0, 0", (0.0, 0.0, 1.0)),
        ("90, 0", (1.0, 0.0, 0.0)),
        ("-90, 0", (-1.0, 0.0, 0.0)),
        ("180, 0", (0.0, 0.0, -1.0)),
        ("0, 90", (0.0, -1.0, 0.0)),
        ("0, -90", (0.0, 1.0, 0.0)),
    ] {
        let trace = run(&set_global(&format!("Direction From Angles({angles})")));
        assert_close(global(&trace), expected);
    }
}

#[test]
fn angles_from_direction_invert_the_frame() {
    for (direction, horizontal, vertical) in [
        ("Vector(0, 0, 1)", 0.0, 0.0),
        ("Vector(1, 0, 0)", 90.0, 0.0),
        ("Vector(-1, 0, 0)", -90.0, 0.0),
        ("Vector(0, -1, 0)", 0.0, 90.0),
        ("Vector(0, 1, 0)", 0.0, -90.0),
    ] {
        let trace = run(&set_global(&format!(
            "Horizontal Angle From Direction({direction})"
        )));
        let &Val::Number(h) = global(&trace) else {
            panic!("{:?}", trace.globals["g"])
        };
        assert!(
            (h - horizontal).abs() < 1e-9,
            "{direction}: {h} != {horizontal}"
        );
        let trace = run(&set_global(&format!(
            "Vertical Angle From Direction({direction})"
        )));
        let &Val::Number(v) = global(&trace) else {
            panic!("{:?}", trace.globals["g"])
        };
        assert!(
            (v - vertical).abs() < 1e-9,
            "{direction}: {v} != {vertical}"
        );
    }
}

#[test]
fn dot_and_cross_product() {
    let trace = run(&set_global("Dot Product(Vector(1, 2, 3), Vector(4, 5, 6))"));
    assert_eq!(global(&trace), &Val::Number(32.0));
    // x̂ × ŷ = ẑ.
    let trace = run(&set_global(
        "Cross Product(Vector(1, 0, 0), Vector(0, 1, 0))",
    ));
    assert_eq!(global(&trace), &vec(0.0, 0.0, 1.0));
    let trace = run(&set_global(
        "Cross Product(Vector(0, 1, 0), Vector(1, 0, 0))",
    ));
    assert_eq!(global(&trace), &vec(0.0, 0.0, -1.0));
}

#[test]
fn normalize_and_magnitude() {
    let trace = run(&set_global("Normalize(Vector(3, 0, 4))"));
    assert_eq!(global(&trace), &vec(0.6, 0.0, 0.8));
    let trace = run(&set_global("Magnitude Of(Vector(3, 0, 4))"));
    assert_eq!(global(&trace), &Val::Number(5.0));

    // A zero-length vector has no unit direction; unmeasured.
    let trace = run(&set_global("Normalize(Vector(0, 0, 0))"));
    assert_not_modeled(&trace, "normalize of a zero-length vector");
}

#[test]
fn vector_arithmetic_is_componentwise_with_scalar_broadcast() {
    for (value, expected) in [
        ("Add(Vector(1, 2, 3), Vector(4, 5, 6))", vec(5.0, 7.0, 9.0)),
        (
            "Subtract(Vector(1, 2, 3), Vector(4, 5, 6))",
            vec(-3.0, -3.0, -3.0),
        ),
        (
            "Multiply(Vector(1, 2, 3), Vector(4, 5, 6))",
            vec(4.0, 10.0, 18.0),
        ),
        (
            "Divide(Vector(4, 10, 18), Vector(4, 5, 6))",
            vec(1.0, 2.0, 3.0),
        ),
        ("Multiply(Vector(1, 2, 3), 2)", vec(2.0, 4.0, 6.0)),
        ("Multiply(2, Vector(1, 2, 3))", vec(2.0, 4.0, 6.0)),
        ("Add(Vector(1, 2, 3), 1)", vec(2.0, 3.0, 4.0)),
        ("Subtract(10, Vector(1, 2, 3))", vec(9.0, 8.0, 7.0)),
        ("Divide(1, Vector(1, 2, 4))", vec(1.0, 0.5, 0.25)),
        // Purely numeric operands still yield a number.
        ("Add(2, 3)", Val::Number(5.0)),
        ("Divide(10, 4)", Val::Number(2.5)),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &expected, "{value}");
    }
}

/// `Set` then `Modify` a variable: the `Modify` operations named like the
/// value forms run the same vector math.
fn modify_global(initial: &str, op: &str, operand: &str) -> String {
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
        Set Global Variable(g, {initial});
        Modify Global Variable(g, {op}, {operand});
    }}
}}
"#
    )
}

#[test]
fn modify_variable_arithmetic_is_the_same_vector_math() {
    for (initial, op, operand, expected) in [
        (
            "Vector(1, 2, 3)",
            "Add",
            "Vector(10, 20, 30)",
            vec(11.0, 22.0, 33.0),
        ),
        (
            "Vector(1, 2, 3)",
            "Subtract",
            "Vector(10, 20, 30)",
            vec(-9.0, -18.0, -27.0),
        ),
        ("Vector(1, 2, 3)", "Multiply", "2", vec(2.0, 4.0, 6.0)),
        (
            "Vector(4, 10, 18)",
            "Divide",
            "Vector(4, 5, 6)",
            vec(1.0, 2.0, 3.0),
        ),
        ("2", "Raise To Power", "8", Val::Number(256.0)),
    ] {
        let trace = run(&modify_global(initial, op, operand));
        assert_eq!(global(&trace), &expected, "{op} on {initial}");
    }

    // `Modulo` stays scalar: a vector operand is not a number.
    let trace = run(&modify_global("Vector(1, 2, 3)", "Modulo", "2"));
    assert_not_modeled(&trace, "arithmetic on Vector { x: 1.0, y: 2.0, z: 3.0 }");
}

#[test]
fn modify_at_index_runs_the_vector_arithmetic_on_the_element() {
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
        Set Global Variable(g, Array(Vector(1, 2, 3), 0));
        Modify Global Variable At Index(g, 0, Add, Vector(10, 20, 30));
    }
}
"#;
    let trace = run(text);
    assert_eq!(
        global(&trace),
        &Val::Array(vec![vec(11.0, 22.0, 33.0), Val::Number(0.0)])
    );
}

#[test]
fn modify_variable_reads_a_player_operand_position() {
    let text = r#"
variables {
    global:
        0: g
    player:
        0: v
}

rule ("probe") {
    event {
        Ongoing - Each Player;
    }
    actions {
        Set Global Variable(g, Vector(1, 2, 3));
        Modify Global Variable(g, Add, Event Player);
        Set Player Variable(Event Player, v, Vector(1, 2, 3));
        Modify Player Variable(Event Player, v, Add, Vector(10, 20, 30));
        Set Global Variable(g, (Event Player).v);
    }
}
"#;
    let trace = run_with(
        text,
        r#"{ "id": "p1", "position": { "x": 10, "y": 20, "z": 30 } }"#,
    );
    assert_eq!(global(&trace), &vec(11.0, 22.0, 33.0));

    // A `Player` operand without a scenario `position` is a missing input.
    let trace = run_with(
        &modify_global("Vector(1, 2, 3)", "Add", "Host Player"),
        &format!("{HOST} }}"),
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Position);
}

#[test]
fn vector_divide_by_zero_stops_instead_of_serializing_null() {
    let trace = run(&set_global("Divide(Vector(1, 2, 3), 0)"));
    assert_not_modeled(&trace, "divide produced a non-finite vector");
}

#[test]
fn angle_between_vectors_reports_degrees() {
    for (value, expected) in [
        (
            "Angle Between Vectors(Vector(1, 0, 0), Vector(0, 1, 0))",
            90.0,
        ),
        (
            "Angle Between Vectors(Vector(1, 0, 0), Vector(2, 0, 0))",
            0.0,
        ),
        (
            "Angle Between Vectors(Vector(1, 0, 0), Vector(-1, 0, 0))",
            180.0,
        ),
    ] {
        let trace = run(&set_global(value));
        let &Val::Number(angle) = global(&trace) else {
            panic!("{:?}", trace.globals["g"])
        };
        assert!(
            (angle - expected).abs() < 1e-9,
            "{value}: {angle} != {expected}"
        );
    }
    // A zero-length vector has no direction.
    let trace = run(&set_global(
        "Angle Between Vectors(Vector(0, 0, 0), Vector(1, 0, 0))",
    ));
    assert_not_modeled(&trace, "angleBetweenVectors on a zero-length vector");
}

#[test]
fn vectors_compare_by_component() {
    for (value, expected) in [
        ("Compare(Vector(1, 2, 3), ==, Vector(1, 2, 3))", true),
        ("Compare(Vector(1, 2, 3), ==, Vector(1, 2, 4))", false),
        ("Compare(Vector(1, 2, 3), !=, Vector(0, 0, 0))", true),
        // A vector is never a number, an array or a text.
        ("Compare(Vector(1, 2, 3), ==, 0)", false),
        ("Compare(Vector(1, 2, 3), ==, Array(1, 2, 3))", false),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Bool(expected), "{value}");
    }
}

const HOST: &str = r#"{ "id": "p1", "host": true"#;

#[test]
fn position_of_reads_the_scenario_field() {
    let trace = run_with(
        &set_global("Position Of(Host Player)"),
        r#"{ "id": "p1", "host": true, "position": { "x": 1, "y": 2, "z": 3 } }"#,
    );
    assert_eq!(global(&trace), &vec(1.0, 2.0, 3.0));

    // A `Player` operand of a position value reads the same field.
    let trace = run_with(
        &set_global("Distance Between(Host Player, Vector(4, 6, 3))"),
        r#"{ "id": "p1", "host": true, "position": { "x": 1, "y": 2, "z": 3 } }"#,
    );
    assert_eq!(global(&trace), &Val::Number(5.0));
    let trace = run_with(
        &set_global("Add(Host Player, Vector(1, 1, 1))"),
        r#"{ "id": "p1", "host": true, "position": { "x": 1, "y": 2, "z": 3 } }"#,
    );
    assert_eq!(global(&trace), &vec(2.0, 3.0, 4.0));
}

#[test]
fn a_missing_position_is_a_missing_input_naming_the_field() {
    let trace = run_with(
        &set_global("Position Of(Host Player)"),
        &format!("{HOST} }}"),
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Position);
    assert_eq!(input.player.as_deref(), Some("p1"));
    assert_eq!(input.name.as_deref(), Some("position"));
    assert_eq!(input.tick, Some(0));

    // The same for a `Player` operand of an arithmetic value.
    let trace = run_with(&set_global("Add(Host Player, 1)"), &format!("{HOST} }}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Position);
}

#[test]
fn facing_direction_of_reproduces_the_scenario_angles() {
    let trace = run_with(
        &set_global("Facing Direction Of(Host Player)"),
        r#"{ "id": "p1", "host": true, "facing": { "horizontal": 90, "vertical": 0 } }"#,
    );
    assert_close(global(&trace), (1.0, 0.0, 0.0));

    let trace = run_with(
        &set_global("Facing Direction Of(Host Player)"),
        &format!("{HOST} }}"),
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Facing);
    assert_eq!(input.player.as_deref(), Some("p1"));
    assert_eq!(input.name.as_deref(), Some("facing"));
}

#[test]
fn local_and_world_vector_of_rotate_and_translate() {
    // Level facing: rotation alone is the identity, translation subtracts
    // the position.
    let player = r#"{ "id": "p1", "host": true, "position": { "x": 5, "y": 6, "z": 7 }, "facing": { "horizontal": 0, "vertical": 0 } }"#;
    let trace = run_with(
        &set_global("Local Vector Of(Vector(1, 2, 3), Host Player, Rotation)"),
        player,
    );
    assert_close(global(&trace), (1.0, 2.0, 3.0));
    let trace = run_with(
        &set_global("Local Vector Of(Vector(5, 6, 7), Host Player, Rotation And Translation)"),
        player,
    );
    assert_close(global(&trace), (0.0, 0.0, 0.0));

    // Facing left (+x): the player's forward is world +x, so world +z is
    // its right — local -x — and a local forward is world +x back.
    let player = r#"{ "id": "p1", "host": true, "position": { "x": 5, "y": 6, "z": 7 }, "facing": { "horizontal": 90, "vertical": 0 } }"#;
    let trace = run_with(
        &set_global("Local Vector Of(Vector(0, 0, 1), Host Player, Rotation)"),
        player,
    );
    assert_close(global(&trace), (-1.0, 0.0, 0.0));
    let trace = run_with(
        &set_global("World Vector Of(Vector(0, 0, 1), Host Player, Rotation)"),
        player,
    );
    assert_close(global(&trace), (1.0, 0.0, 0.0));
    let trace = run_with(
        &set_global("World Vector Of(Vector(1, 1, 0), Host Player, Rotation And Translation)"),
        player,
    );
    assert_close(global(&trace), (5.0, 7.0, 6.0));

    // `ROTATION` needs `facing`; `ROTATION_AND_TRANSLATION` needs both.
    let trace = run_with(
        &set_global("Local Vector Of(Vector(0, 0, 1), Host Player, Rotation)"),
        &format!("{HOST} }}"),
    );
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Facing);
    let trace = run_with(
        &set_global("World Vector Of(Vector(0, 0, 1), Host Player, Rotation And Translation)"),
        r#"{ "id": "p1", "host": true, "facing": { "horizontal": 0, "vertical": 0 } }"#,
    );
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Position);
}

#[test]
fn eye_position_is_a_scenario_state_read() {
    let player = r#"{ "id": "p1", "host": true, "position": { "x": 0, "y": 0, "z": 0 }, "facing": { "horizontal": 0, "vertical": 0 } }"#;
    // `Eye Position` reads the scenario `state` map (athena#89); a
    // version-1 player has no `state`, so the read is a missing input
    // naming `state.eyePosition`.
    let trace = run_with(&set_global("Eye Position(Host Player)"), player);
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let missing = &trace.missing_inputs[0];
    assert_eq!(missing.input, MissingInputKind::State);
    assert_eq!(missing.player.as_deref(), Some("p1"));
    assert_eq!(missing.name.as_deref(), Some("state.eyePosition"));
}

#[test]
fn a_vector_serializes_in_the_documented_shape() {
    let trace = run(&set_global("Vector(1, 2.5, -3)"));
    let json = serde_json::to_value(&trace).unwrap();
    assert_eq!(
        json["globals"]["g"],
        serde_json::json!({ "type": "vector", "value": { "x": 1.0, "y": 2.5, "z": -3.0 } })
    );
}

/// `__ambiguous_enum` — the parser's sentinel for a bare member
/// spelling that names a candidate in more than one enum domain —
/// resolves to its `Vector` member's directional constant in a vector
/// or number operand position (athena#80). `Forward`/`Backward` name
/// only a `Vector` member, so they parse to the enum literal directly
/// and the same constants apply.
#[test]
fn ambiguous_vector_spellings_resolve_in_operand_positions() {
    let trace = run(&set_global("Add(Left, Right)"));
    assert_eq!(global(&trace), &vec(0.0, 0.0, 0.0));

    for (spelling, expected) in [
        ("Left", (1.0, 0.0, 0.0)),
        ("Right", (-1.0, 0.0, 0.0)),
        ("Up", (0.0, 1.0, 0.0)),
        ("Down", (0.0, -1.0, 0.0)),
        ("Forward", (0.0, 0.0, 1.0)),
        ("Backward", (0.0, 0.0, -1.0)),
    ] {
        let trace = run(&set_global(&format!("Add({spelling}, Vector(0, 0, 0))")));
        assert_close(global(&trace), expected);
    }
}

/// The same resolution inside the operands of a `Compare`, reached
/// through `If` and `Or`.
#[test]
fn ambiguous_vector_spellings_resolve_inside_compare() {
    let text = r#"
variables {
    global:
        0: g
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    conditions {
        Or(Compare(Vector(0, 0, 0), ==, Add(Left, Right)), False);
    }
    actions {
        If(Compare(Vector(0, 0, 0), ==, Add(Left, Right)));
            Set Global Variable(g, 1);
        End;
    }
}
"#;
    let trace = run(text);
    assert_eq!(global(&trace), &Val::Number(1.0));
}

/// An ambiguous spelling in a position the catalog does not type as a
/// vector or number operand stays `not_modeled` and names the spelling.
#[test]
fn an_ambiguous_spelling_outside_an_operand_position_is_not_modeled() {
    // `All Players`' argument is a `Team`.
    let trace = run(&set_global("Count Of(All Players(Left))"));
    assert_not_modeled(
        &trace,
        "value __ambiguous_enum `Left` in `array` argument of `allPlayers`",
    );

    // `Evaluate Once`'s operand is a value call's argument too, though
    // eval handles it ahead of the call dispatch.
    let trace = run(&set_global("Evaluate Once(Left)"));
    assert_not_modeled(
        &trace,
        "value __ambiguous_enum `Left` in `value` argument of `evaluateOnce`",
    );

    // A sentinel as an action's argument names only the spelling.
    let trace = run(&set_global("Left"));
    assert_not_modeled(&trace, "value __ambiguous_enum `Left`");
}

#[test]
fn a_partial_position_or_facing_is_malformed() {
    let text = set_global("1");
    for player in [
        r#"{ "id": "p1", "position": null }"#,
        r#"{ "id": "p1", "position": { "x": 1, "y": 2 } }"#,
        r#"{ "id": "p1", "position": [1, 2] }"#,
        r#"{ "id": "p1", "facing": null }"#,
        r#"{ "id": "p1", "facing": { "horizontal": 90 } }"#,
        r#"{ "id": "p1", "facing": { "horizontal": 90, "vertical": 0, "roll": 0 } }"#,
    ] {
        let scenario =
            format!(r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [{player}] }}"#);
        match run_scenario(&text, &scenario) {
            Err(RunError::Scenario(ScenarioError::Format(_))) => {}
            other => panic!("{player}: {other:?}"),
        }
    }
}

/// `Vertical Facing Angle Of` reads the same scenario `facing` input
/// `Facing Direction Of` does — the vertical component (athena#105).
#[test]
fn vertical_facing_angle_of_reads_the_scenario_facing() {
    let trace = run_with(
        &set_global("Vertical Facing Angle Of(Host Player)"),
        r#"{ "id": "p1", "host": true, "facing": { "horizontal": 90, "vertical": 45 } }"#,
    );
    assert_eq!(global(&trace), &Val::Number(45.0));

    // A player without `facing` is the same missing input
    // `Facing Direction Of` reports.
    let trace = run_with(
        &set_global("Vertical Facing Angle Of(Host Player)"),
        &format!("{HOST} }}"),
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Facing);
    assert_eq!(input.player.as_deref(), Some("p1"));
}

/// `Players Within Radius` yields the scenario players on the team
/// whose `position` is within the radius, in roster order — the
/// catalog notes dead players are picked up too — and `losCheck`
/// evaluates but holds in the empty world (athena#105).
#[test]
fn players_within_radius_filters_by_team_and_distance() {
    let players = concat!(
        r#"{ "id": "p1", "team": "team1", "host": true, "position": { "x": 0, "y": 0, "z": 0 } },"#,
        r#"{ "id": "p2", "team": "team2", "position": { "x": 5, "y": 0, "z": 10 } },"#,
        r#"{ "id": "p3", "team": "team1", "position": { "x": 3, "y": 0, "z": 0 } }"#
    );

    // All teams, radius 4 from the origin: p1 and p3, roster order.
    let trace = run_with(
        &set_global("Players Within Radius(Vector(0, 0, 0), 4, All Teams, Off)"),
        players,
    );
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::Player("p1".into()), Val::Player("p3".into())])
    );

    // Team 2 alone: p2 is over 11 m out — an empty array.
    let trace = run_with(
        &set_global("Players Within Radius(Vector(0, 0, 0), 4, Team 2, Off)"),
        players,
    );
    assert_eq!(global(&trace), &Val::Array(vec![]));

    // A player as the center reads that player's `position`.
    let trace = run_with(
        &set_global("Players Within Radius(Host Player, 4, All Teams, Off)"),
        players,
    );
    assert_eq!(
        global(&trace),
        &Val::Array(vec![Val::Player("p1".into()), Val::Player("p3".into())])
    );

    // A team player without `position` is the same missing input a
    // `Position Of` read reports.
    let trace = run_with(
        &set_global("Players Within Radius(Vector(0, 0, 0), 4, All Teams, Off)"),
        r#"{ "id": "p1", "team": "team1" }"#,
    );
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Position);
    assert_eq!(trace.missing_inputs[0].player.as_deref(), Some("p1"));
}
