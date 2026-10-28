//! `Random *` values drawing from the scenario's `random` source
//! (athena#65): deterministic under one seed, a missing input without a
//! source, recorded as `randomDraw` changes.

use athena_sim::{
    Change, MissingInputKind, Outcome, RunError, ScenarioError, Val, run_scenario, run_text,
};

fn seeded(text: &str, seed: u64) -> athena_sim::Trace {
    let scenario = format!(
        r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], "random": {{ "seed": {seed} }} }}"#
    );
    run_scenario(text, &scenario).unwrap_or_else(|error| panic!("{error}"))
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

#[test]
fn the_same_seed_gives_identical_traces() {
    let text = r#"
variables {
    global:
        0: g
        1: h
}

rule ("probe") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(g, Array(Random Integer(1, 10), Random Real(0, 1),
            Random Value In Array(Array("a", "b", "c"))));
        Set Global Variable(h, Randomized Array(Array(1, 2, 3, 4, 5)));
    }
}
"#;
    assert_eq!(seeded(text, 42), seeded(text, 42));
}

#[test]
fn different_seeds_can_differ() {
    let text = set_global("Random Integer(0, 1000000)");
    let draws: Vec<_> = (0..8)
        .map(|seed| seeded(&text, seed).globals["g"].clone())
        .collect();
    assert!(
        draws.iter().any(|draw| *draw != draws[0]),
        "eight seeds drew {draws:?}"
    );
}

#[test]
fn without_a_source_a_random_value_is_a_missing_input() {
    let scenario = r#"{ "scenarioVersion": 1, "seconds": 1, "players": [] }"#;
    let text = set_global("Random Integer(0, 9)");
    let trace = run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::Random);
    assert_eq!(input.name.as_deref(), Some("random"));
    assert_eq!(input.tick, Some(0));

    // A run without a scenario reports it the same way.
    let trace = run_text(&text, athena_sim::Options { seconds: 1.0 })
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::Random);
}

#[test]
fn random_integer_is_inclusive_inside_the_bounds() {
    for (bounds, lo, hi) in [
        ("Random Integer(7, 7)", 7.0, 7.0),
        ("Random Integer(2, 9)", 2.0, 9.0),
        // A `Min` past `Max` swaps the bounds.
        ("Random Integer(9, 2)", 2.0, 9.0),
        ("Random Integer(-4, -1)", -4.0, -1.0),
    ] {
        for seed in 0..4 {
            let trace = seeded(&set_global(bounds), seed);
            let Val::Number(n) = global(&trace) else {
                panic!("{bounds} seed {seed}: {:?}", trace.globals["g"]);
            };
            let n = n.get();
            assert!(
                n >= lo && n <= hi && n.fract() == 0.0,
                "{bounds} seed {seed}: {n}"
            );
        }
    }
}

#[test]
fn random_real_stays_inside_min_max() {
    for seed in 0..8 {
        let trace = seeded(&set_global("Random Real(2, 5)"), seed);
        let Val::Number(n) = global(&trace) else {
            panic!("seed {seed}: {:?}", trace.globals["g"]);
        };
        assert!(n.get() >= 2.0 && n.get() < 5.0, "seed {seed}: {n:?}");
    }
    // Swapped bounds cover the same interval.
    let trace = seeded(&set_global("Random Real(5, 2)"), 1);
    let Val::Number(n) = global(&trace) else {
        panic!("{:?}", trace.globals["g"]);
    };
    assert!(n.get() >= 2.0 && n.get() < 5.0, "{n:?}");
}

#[test]
fn random_value_in_array_picks_an_element_or_null() {
    for seed in 0..4 {
        let trace = seeded(
            &set_global(r#"Random Value In Array(Array("a", "b", "c"))"#),
            seed,
        );
        match global(&trace) {
            Val::Text(text) => assert!(["a", "b", "c"].contains(&text.as_str()), "{text}"),
            other => panic!("seed {seed}: {other:?}"),
        }
    }

    // An empty array — `Null` reads as one too — yields `Null`.
    for value in [
        "Random Value In Array(Empty Array)",
        "Random Value In Array(Null)",
    ] {
        let trace = seeded(&set_global(value), 1);
        assert_eq!(global(&trace), &Val::Null, "{value}");
    }
}

#[test]
fn randomized_array_keeps_the_elements() {
    let text = set_global("Randomized Array(Array(1, 2, 3, 4, 5))");
    for seed in 0..4 {
        let trace = seeded(&text, seed);
        let Val::Array(items) = global(&trace) else {
            panic!("seed {seed}: {:?}", trace.globals["g"]);
        };
        let mut sorted: Vec<f64> = items
            .iter()
            .map(|item| match item {
                Val::Number(n) => n.get(),
                other => panic!("seed {seed}: {other:?}"),
            })
            .collect();
        sorted.sort_by(f64::total_cmp);
        assert_eq!(sorted, [1.0, 2.0, 3.0, 4.0, 5.0], "seed {seed}");
    }
    // Empty and one-element arrays come back unchanged.
    for (value, expected) in [
        ("Randomized Array(Empty Array)", Val::Array(vec![])),
        ("Randomized Array(Null)", Val::Array(vec![])),
        (
            "Randomized Array(Array(7))",
            Val::Array(vec![Val::number(7.0)]),
        ),
    ] {
        let trace = seeded(&set_global(value), 1);
        assert_eq!(global(&trace), &expected, "{value}");
    }
}

#[test]
fn a_draw_is_recorded_on_the_tick_changes() {
    let trace = seeded(&set_global("Random Integer(0, 9)"), 7);
    let draws: Vec<_> = trace
        .timeline
        .entries
        .iter()
        .flat_map(|entry| entry.changes.iter())
        .filter_map(|change| match change {
            Change::RandomDraw {
                rule,
                construct,
                value,
            } => Some((*rule, construct.as_str(), value.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        draws,
        vec![(0, "randomInteger", trace.globals["g"].clone())],
        "{:?}",
        trace.timeline
    );
}

/// A `Wait Until` in a `Call Subroutine` frame keeps the subroutine's
/// rule index on the draws its per-tick re-checks record.
#[test]
fn a_wait_until_recheck_records_the_subroutines_rule() {
    let text = r#"
subroutines {
    0: sub
}

rule ("main") {
    event {
        Ongoing - Global;
    }
    actions {
        Call Subroutine(sub);
    }
}

rule ("Subroutine sub") {
    event {
        Subroutine;
        sub;
    }
    actions {
        Wait Until(Random Integer(0, 0) > 0, 0.2);
    }
}
"#;
    let trace = seeded(text, 1);
    // One draw on tick 0 for the first check, one per re-check tick.
    let rules: Vec<usize> = trace
        .timeline
        .entries
        .iter()
        .flat_map(|entry| entry.changes.iter())
        .filter_map(|change| match change {
            Change::RandomDraw { rule, .. } => Some(*rule),
            _ => None,
        })
        .collect();
    assert!(rules.len() > 1, "expected re-checks: {:?}", trace.timeline);
    assert!(rules.iter().all(|rule| *rule == 1), "{rules:?}");
}

/// Finite bounds can still span a range `Max - Min` cannot hold: the run
/// stops `not_modeled`, as for a non-finite bound.
#[test]
fn a_non_finite_range_is_not_modeled() {
    for value in [
        "Random Integer(Subtract(0, Raise To Power(10, 308)), Raise To Power(10, 308))",
        "Random Real(Subtract(0, Raise To Power(10, 308)), Raise To Power(10, 308))",
    ] {
        let trace = seeded(&set_global(value), 1);
        assert!(
            matches!(trace.outcome, Outcome::NotModeled(_)),
            "{value}: {:?}",
            trace.outcome
        );
    }
}

/// `seed` is an unsigned 64-bit integer: `u64::MAX` loads, `null` is not
/// "absent" and `2^64` is past the range.
#[test]
fn the_seed_is_an_unsigned_64_bit_integer() {
    let text = set_global("Random Integer(0, 9)");
    assert_eq!(seeded(&text, u64::MAX).outcome, Outcome::Complete);
    for random in ["null", r#"{ "seed": 18446744073709551616 }"#] {
        let scenario = format!(
            r#"{{ "scenarioVersion": 1, "seconds": 1, "players": [], "random": {random} }}"#
        );
        match run_scenario(&text, &scenario) {
            Err(RunError::Scenario(ScenarioError::Format(_))) => {}
            other => panic!("random {random}: {other:?}"),
        }
    }
}
