//! The `matchPhase` scenario field: the six match-phase reads answer
//! true for their own phase and false for the others, and a run without
//! the field ends `missing_input` (athena#101).

use athena_sim::{MissingInputKind, Outcome, RunError, Val, run_scenario, run_text};

/// The phase reads by en-US spelling, each with the `matchPhase` it
/// answers true for.
const READS: &[(&str, &str)] = &[
    ("Is Waiting For Players", "waitingForPlayers"),
    ("Is Assembling Heroes", "assemblingHeroes"),
    ("Is In Setup", "setup"),
    ("Is Game In Progress", "gameInProgress"),
    ("Is Between Rounds", "betweenRounds"),
    ("Is Match Complete", "matchComplete"),
];

/// One `Ongoing - Global` rule that writes the read's answer to `g` at
/// tick 0.
fn set_global(value: &str) -> String {
    format!(
        r#"
variables {{
    global:
        0: g
}}

rule ("phase") {{
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

fn run(text: &str, phase: &str) -> athena_sim::Trace {
    let scenario = format!(
        r#"{{ "scenarioVersion": 2, "seconds": 1, "players": [], "matchPhase": "{phase}" }}"#
    );
    run_scenario(text, &scenario).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn each_read_is_true_for_its_own_phase_only() {
    for phase in READS.iter().map(|(_, phase)| phase) {
        for (read, own) in READS {
            let trace = run(&set_global(read), phase);
            assert_eq!(trace.outcome, Outcome::Complete, "{read} under {phase}");
            assert_eq!(
                trace.globals["g"],
                Val::Bool(phase == own),
                "{read} under {phase}"
            );
        }
    }
}

#[test]
fn a_phase_read_without_the_field_is_a_missing_input() {
    let scenario = r#"{ "scenarioVersion": 2, "seconds": 1, "players": [] }"#;
    let text = set_global("Is In Setup");
    let trace = run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    let [input] = trace.missing_inputs.as_slice() else {
        panic!("{:?}", trace.missing_inputs);
    };
    assert_eq!(input.input, MissingInputKind::MatchPhase);
    assert_eq!(input.player, None);
    assert_eq!(input.name.as_deref(), Some("matchPhase"));
    assert_eq!(input.tick, Some(0));

    // A run without a scenario reports it the same way.
    let trace = run_text(&text, athena_sim::Options { seconds: 1.0 })
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::MissingInput);
    assert_eq!(trace.missing_inputs[0].input, MissingInputKind::MatchPhase);
}

#[test]
fn an_unknown_or_mistyped_match_phase_is_refused() {
    for match_phase in [r#""overtime""#, r#""SETUP""#, "3", "true"] {
        let json = format!(
            r#"{{ "scenarioVersion": 2, "seconds": 1, "players": [],
                 "matchPhase": {match_phase} }}"#
        );
        assert!(
            matches!(
                run_scenario(&set_global("Is In Setup"), &json),
                Err(RunError::Scenario(_))
            ),
            "matchPhase {match_phase}: expected a scenario refusal"
        );
    }
}
