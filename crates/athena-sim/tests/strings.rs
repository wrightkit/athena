//! String and text value constructs (athena#57): `Char In String`,
//! `String Split`, `Index Of String Char`, `String Replace`, and the
//! `Custom String` placeholder slots.

use athena_sim::{Options, Outcome, Val, run_text};

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

/// `Char In String` reads the character at the index, coerces a boolean
/// index to 0/1, and yields the empty string for an index outside the
/// string's bounds — negative or past the end — or for the `Empty
/// Array` spelling of an empty string.
#[test]
fn char_in_string_reads_the_character_or_empty_string() {
    for (value, expected) in [
        ("Char In String(Custom String(\"abc\"), 1)", "b"),
        // A boolean index reads as 0/1.
        ("Char In String(Custom String(\"abc\"), False)", "a"),
        ("Char In String(Custom String(\"abc\"), True)", "b"),
        // Characters index as characters, not UTF-8 bytes.
        ("Char In String(Custom String(\"堡垒\"), 1)", "垒"),
        // Out of bounds, negative, and the empty string: `""`.
        ("Char In String(Custom String(\"abc\"), 3)", ""),
        ("Char In String(Custom String(\"abc\"), -1)", ""),
        ("Char In String(Empty Array, False)", ""),
        ("Char In String(Custom String(\"\"), 0)", ""),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &text(expected), "{value}");
    }
}

/// `String Split` splits around the separator; a separator the source
/// does not contain yields the source alone, an empty separator splits
/// into characters, and an empty source yields the empty array.
#[test]
fn string_split_splits_around_the_separator() {
    let trace = run(&set_global(
        "String Split(Custom String(\"9-8-7\"), Custom String(\"-\"))",
    ));
    assert_eq!(
        global(&trace),
        &Val::Array(vec![text("9"), text("8"), text("7")])
    );

    // A separator the source does not contain yields the source alone.
    let trace = run(&set_global(
        "String Split(Custom String(\"abc\"), Custom String(\"-\"))",
    ));
    assert_eq!(global(&trace), &Val::Array(vec![text("abc")]));

    // The `Empty Array` spelling of an empty separator splits into
    // characters.
    let trace = run(&set_global(
        "String Split(Custom String(\"ab\"), Empty Array)",
    ));
    assert_eq!(global(&trace), &Val::Array(vec![text("a"), text("b")]));

    // The empty source yields the empty array.
    let trace = run(&set_global(
        "String Split(Empty Array, Custom String(\"-\"))",
    ));
    assert_eq!(global(&trace), &Val::Array(vec![]));
    let trace = run(&set_global(
        "String Split(Custom String(\"\"), Custom String(\"-\"))",
    ));
    assert_eq!(global(&trace), &Val::Array(vec![]));
}

/// `Index Of String Char` reports the needle's first index in
/// characters, or -1 when the needle — an empty needle included — is
/// absent.
#[test]
fn index_of_string_char_finds_the_first_index_or_minus_one() {
    for (value, expected) in [
        (
            "Index Of String Char(Custom String(\"abcabc\"), Custom String(\"b\"))",
            1.0,
        ),
        // The index counts characters, not UTF-8 bytes.
        (
            "Index Of String Char(Custom String(\"堡垒b\"), Custom String(\"b\"))",
            2.0,
        ),
        (
            "Index Of String Char(Custom String(\"abc\"), Custom String(\"z\"))",
            -1.0,
        ),
        (
            "Index Of String Char(Empty Array, Custom String(\"a\"))",
            -1.0,
        ),
        (
            "Index Of String Char(Custom String(\"abc\"), Empty Array)",
            -1.0,
        ),
    ] {
        let trace = run(&set_global(value));
        assert_eq!(global(&trace), &Val::Number(expected), "{value}");
    }
}

/// `String Replace` replaces every occurrence of the pattern.
#[test]
fn string_replace_replaces_all_occurrences() {
    let trace = run(&set_global(
        "String Replace(Custom String(\"1488 and 1488\"), Custom String(\"1488\"), Custom String(\"x\"))",
    ));
    assert_eq!(global(&trace), &text("x and x"));

    // The empty source and the missing pattern leave nothing to do.
    let trace = run(&set_global(
        "String Replace(Empty Array, Custom String(\"a\"), Custom String(\"b\"))",
    ));
    assert_eq!(global(&trace), &text(""));
}

/// Whether the client still caps a produced string at 511 bytes is
/// unmeasured — the sources conflict — so a produced result past the
/// boundary stops the run: a `String Replace` result, a `Custom
/// String` render and a `String Split` part alike.
#[test]
fn a_produced_string_over_511_bytes_is_not_modeled() {
    for value in [
        // A `Custom String` renders past the boundary.
        format!("Custom String(\"{}\")", "a".repeat(600)),
        // `String Replace` expands past it.
        format!(
            "String Replace(Custom String(\"{}\"), Custom String(\"a\"), Custom String(\"垒\"))",
            "a".repeat(300)
        ),
        // A `String Split` part carries it; the bare-literal source
        // enters unproduced so the part itself is what crosses.
        format!(
            "String Split(\"{}\", Custom String(\"-\"))",
            "a".repeat(600)
        ),
    ] {
        let trace = run(&set_global(&value));
        let Outcome::NotModeled(stop) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
        assert!(stop.construct.contains("511"), "{}", stop.construct);
    }
}

/// An empty `String Replace` pattern names no occurrence; the run
/// stops.
#[test]
fn string_replace_with_an_empty_pattern_is_not_modeled() {
    let trace = run(&set_global(
        "String Replace(Custom String(\"abc\"), Empty Array, Custom String(\"x\"))",
    ));
    let Outcome::NotModeled(stop) = &trace.outcome else {
        panic!("{:?}", trace.outcome);
    };
    assert!(
        stop.construct.contains("empty pattern"),
        "{}",
        stop.construct
    );
}

/// `Custom String` fills only the slots it has arguments for; a
/// placeholder without an argument stays literal, and an enum literal
/// argument renders its catalog member id.
#[test]
fn custom_string_leaves_missing_placeholders_literal() {
    let trace = run(&set_global(
        "Custom String(\"{0} {1}\", Custom String(\"a\"))",
    ));
    assert_eq!(global(&trace), &text("a {1}"));

    let trace = run(&set_global("Custom String(\"{0}\", Button(Reload))"));
    assert_eq!(global(&trace), &text("RELOAD"));
}

/// The display-only strings render client-localized glyphs — icon
/// pictures, ability icons, per-player key names — which no capture or
/// documented placeholder text defines; the run stops.
#[test]
fn display_only_strings_are_not_modeled() {
    for value in [
        "Icon String(Bolt)",
        "Ability Icon String(Hero(Ana), Button(Ability 1))",
        "Input Binding String(Button(Jump))",
        "Hero Icon String(Hero(Ana))",
        "Custom String(\"{0}\", Icon String(Bolt))",
    ] {
        let trace = run(&set_global(value));
        let Outcome::NotModeled(_) = &trace.outcome else {
            panic!("{value}: {:?}", trace.outcome);
        };
    }
}
