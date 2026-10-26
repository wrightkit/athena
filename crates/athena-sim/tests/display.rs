//! Opaque display values (athena#113): the display-only producers —
//! `Icon String`, `Hero Icon String`, `Ability Icon String`, `Input
//! Binding String`, `Custom Color` — yield a client rendering token the
//! model carries as the producer and its evaluated arguments, never the
//! token's content.

use athena_sim::{HudKind, Options, Outcome, Val, run_text};

fn run(text: &str) -> athena_sim::Trace {
    run_text(text, Options { seconds: 2.0 }).unwrap_or_else(|error| panic!("{error}"))
}

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

fn global(trace: &athena_sim::Trace) -> &Val {
    assert_eq!(trace.outcome, Outcome::Complete);
    &trace.globals["g"]
}

fn text(value: &str) -> Val {
    Val::Text(value.to_string())
}

fn display(producer: &str, args: Vec<Val>) -> Val {
    Val::Display {
        producer: producer.to_string(),
        args,
    }
}

/// Each producer yields an opaque display value: the value id and the
/// evaluated arguments — enum members as their member id strings —
/// never the client's glyph, key name or color.
#[test]
fn each_producer_yields_an_opaque_display_value() {
    for (value, expected) in [
        (
            "Icon String(Bolt)",
            display("iconString", vec![text("BOLT")]),
        ),
        (
            "Hero Icon String(Hero(Ana))",
            display("heroIconString", vec![text("ANA")]),
        ),
        (
            "Ability Icon String(Hero(Ana), Button(Ability 1))",
            display("abilityIconString", vec![text("ANA"), text("ABILITY_1")]),
        ),
        (
            "Input Binding String(Button(Jump))",
            display("inputBindingString", vec![text("JUMP")]),
        ),
        (
            "Custom Color(118, 93, 189, 255)",
            display(
                "customColor",
                vec![
                    Val::Number(118.0),
                    Val::Number(93.0),
                    Val::Number(189.0),
                    Val::Number(255.0),
                ],
            ),
        ),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &expected, "{value}");
    }
}

/// Arguments evaluate left to right, so a display value holds the value
/// a variable read gave, not the source expression.
#[test]
fn the_arguments_are_the_evaluated_values() {
    let trace = run(r#"
variables {
    global:
        0: g
        1: icon
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(icon, Icon String(Warning));
        Set Global Variable(g, Global.icon);
    }
}
"#);
    assert_eq!(
        global(&trace),
        &display("iconString", vec![text("WARNING")])
    );
}

/// A variable write records the display value in `changes`, serialized
/// as producer + arguments.
#[test]
fn a_change_shows_the_display_value_readably() {
    let trace = run(&set_global("Icon String(Warning)"));
    let json = serde_json::to_value(&trace.timeline.entries[0].changes[0]).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "kind": "globalVariable",
            "name": "g",
            "index": null,
            "old": {"type": "null"},
            "new": {
                "type": "display",
                "value": {
                    "producer": "iconString",
                    "args": [{"type": "text", "value": "WARNING"}],
                },
            },
        })
    );
}

/// An array element write holds a display value like any other.
#[test]
fn a_display_value_flows_through_an_array() {
    let trace = run(&set_global(
        "Array(Icon String(Bolt), Custom Color(1, 2, 3, 4))",
    ));
    let Val::Array(items) = global(&trace) else {
        panic!("{:?}", global(&trace));
    };
    assert_eq!(items[0], display("iconString", vec![text("BOLT")]));
    assert_eq!(
        items[1],
        display(
            "customColor",
            vec![
                Val::Number(1.0),
                Val::Number(2.0),
                Val::Number(3.0),
                Val::Number(4.0)
            ]
        )
    );
}

/// `Big Message` records its header: a `Custom String` on an icon is a
/// display value and the element keeps it, arguments evaluated.
#[test]
fn a_display_value_reaches_a_message_action() {
    let trace = run(r#"
rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Big Message(Null, Custom String("{0} up", Icon String(Warning)));
        Small Message(Null, Icon String(Bolt));
    }
}
"#);
    assert_eq!(trace.outcome, Outcome::Complete);
    let [big, small] = trace.hud.as_slice() else {
        panic!("expected two HUD elements, got {:?}", trace.hud);
    };
    assert_eq!(big.kind, HudKind::BigMessage);
    assert_eq!(
        big.args.header,
        Some(display(
            "customString",
            vec![text("{0} up"), display("iconString", vec![text("WARNING")]),],
        ))
    );
    assert_eq!(small.kind, HudKind::SmallMessage);
    assert_eq!(
        small.args.header,
        Some(display("iconString", vec![text("BOLT")]))
    );
}

/// A HUD color argument takes `Custom Color`: the element keeps the
/// display value where a member id like `WHITE` would sit, and an
/// ordinary `Color` member still records its member id — Bastion's
/// `Custom Color` HUD pattern (athena#113).
#[test]
fn custom_color_reaches_a_hud_color_argument() {
    let trace = run(r#"
rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Create HUD Text(Null, "h", Null, Null, Top, 0, Custom Color(118, 93, 189, 255), Color(White), Color(White), None, Default Visibility);
    }
}
"#);
    assert_eq!(trace.outcome, Outcome::Complete);
    let [hud] = trace.hud.as_slice() else {
        panic!("expected one HUD element, got {:?}", trace.hud);
    };
    assert_eq!(
        hud.args.header_color,
        Some(display(
            "customColor",
            vec![
                Val::Number(118.0),
                Val::Number(93.0),
                Val::Number(189.0),
                Val::Number(255.0),
            ],
        ))
    );
    assert_eq!(hud.args.text_color, Some(text("WHITE")));
    // In the trace JSON the member color stays a bare member id while
    // the `Custom Color` is a structured display value.
    let json = serde_json::to_value(hud).unwrap();
    assert_eq!(json["textColor"], "WHITE");
    assert_eq!(
        json["headerColor"],
        serde_json::json!({
            "type": "display",
            "value": {
                "producer": "customColor",
                "args": [
                    {"type": "number", "value": 118.0},
                    {"type": "number", "value": 93.0},
                    {"type": "number", "value": 189.0},
                    {"type": "number", "value": 255.0},
                ],
            },
        })
    );
}

/// Two display values with the same producer and the same evaluated
/// arguments answer true.
#[test]
fn identical_display_values_compare_equal() {
    for value in [
        "Compare(Icon String(Bolt), ==, Icon String(Bolt))",
        "Compare(Custom Color(1, 2, 3, 4), ==, Custom Color(1, 2, 3, 4))",
        // The arguments compare as values: `Button(Jump)` and the
        // member's own id string differ, so the producers' arguments
        // must too.
        "Compare(Input Binding String(Button(Jump)), ==, Input Binding String(Button(Jump)))",
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Bool(true), "{value}");
    }
    // `!=` answers false on the same structural match.
    let trace = run(&set_global(
        "Compare(Icon String(Bolt), !=, Icon String(Bolt))",
    ));
    assert_eq!(global(&trace), &Val::Bool(false));
}

/// Any other comparison — against a literal string, or two structurally
/// different tokens — needs content the model does not carry, so the
/// run stops.
#[test]
fn any_other_comparison_on_a_display_value_is_not_modeled() {
    for value in [
        // A display value against a literal string.
        "Compare(Icon String(Bolt), ==, \"Bolt\")",
        "Compare(\"Bolt\", ==, Icon String(Bolt))",
        "Compare(Icon String(Bolt), !=, Icon String(Warning))",
        // Two producers that differ.
        "Compare(Icon String(Bolt), ==, Hero Icon String(Hero(Ana)))",
        // Same producer, different arguments.
        "Compare(Icon String(Bolt), ==, Icon String(Warning))",
        // A `Custom String` output in a comparison.
        "Compare(Custom String(\"{0}\", Icon String(Bolt)), ==, Custom String(\"{0}\", Icon String(Bolt)))",
    ] {
        let trace = run(&set_global(value));
        let Outcome::NotModeled(_) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
    }
}

/// An ordinary value still compares the way it did before.
#[test]
fn ordinary_values_still_compare() {
    for (value, expected) in [
        ("Compare(\"a\", ==, \"a\")", true),
        ("Compare(\"a\", ==, \"b\")", false),
        ("Compare(Color(White), ==, Color(White))", true),
        ("Compare(Color(White), ==, Color(Red))", false),
        ("Compare(1, ==, \"a\")", false),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Bool(expected), "{value}");
    }
}

/// Reading a display value as a number stops — an arithmetic operand,
/// an array index, a `Wait` duration all take `number()`.
#[test]
fn a_numeric_read_on_a_display_value_is_not_modeled() {
    for value in [
        "Add(Icon String(Bolt), 1)",
        "Value In Array(Array(1, 2), Icon String(Bolt))",
    ] {
        let trace = run(&set_global(value));
        let Outcome::NotModeled(_) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
    }
}

/// A `Sorted Array` on display values stops — the order of opaque
/// tokens is not defined.
#[test]
fn sorting_display_values_is_not_modeled() {
    let trace = run(&set_global(
        "Sorted Array(Array(Icon String(Bolt), Icon String(Warning)), Current Array Element)",
    ));
    let Outcome::NotModeled(_) = &trace.outcome else {
        panic!("{:?}", trace.outcome);
    };
}
