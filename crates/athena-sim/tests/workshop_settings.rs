//! `Workshop Setting *` values evaluate to their declared default (athena#54).

use athena_sim::{Options, Outcome, Val, run_scenario, run_text};

fn run(text: &str) -> athena_sim::Trace {
    run_text(text, Options { seconds: 2.0 }).unwrap_or_else(|error| panic!("{error}"))
}

/// One setting of each kind, read into a global.
const DEFAULTS: &str = r#"
variables {
    global:
        0: real
        1: int
        2: toggle
        3: combo
        4: hero
}

rule ("defaults") {
    event {
        Ongoing - Global;
    }
    actions {
        Set Global Variable(real, Workshop Setting Real(Custom String("cat"), Custom String("real"), 3.5, 1, 4, 0));
        Set Global Variable(int, Workshop Setting Integer(Custom String("cat"), Custom String("int"), 7, 0, 10, 0));
        Set Global Variable(toggle, Workshop Setting Toggle(Custom String("cat"), Custom String("toggle"), True, 1));
        Set Global Variable(combo, Workshop Setting Combo(Custom String("cat"), Custom String("combo"), 2, Array(Custom String("a"), Custom String("b"), Custom String("c")), 0));
        Set Global Variable(hero, Workshop Setting Hero(Custom String("cat"), Custom String("hero"), Hero(Ana), 0));
    }
}
"#;

#[test]
fn each_setting_kind_evaluates_to_its_default() {
    let trace = run(DEFAULTS);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["real"], Val::Number(3.5));
    assert_eq!(trace.globals["int"], Val::Number(7.0));
    assert_eq!(trace.globals["toggle"], Val::Bool(true));
    assert_eq!(trace.globals["combo"], Val::Number(2.0));
    assert_eq!(trace.globals["hero"], Val::Text("ANA".into()));
}

/// A setting read in a rule condition and in an action argument.
const CONDITION_AND_ARGUMENT: &str = r#"
variables {
    global:
        0: fired
}

rule ("gated") {
    event {
        Ongoing - Global;
    }
    conditions {
        Compare(Workshop Setting Toggle(Custom String("cat"), Custom String("enabled"), True, 0), ==, True);
    }
    actions {
        Set Global Variable(fired, 1);
        Log To Inspector(Custom String("bonus {0}", Workshop Setting Integer(Custom String("cat"), Custom String("bonus"), 25, 0, 50, 0)));
    }
}
"#;

#[test]
fn a_setting_reads_inside_a_condition_and_an_action_argument() {
    let trace = run(CONDITION_AND_ARGUMENT);
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["fired"], Val::Number(1.0));
    assert_eq!(trace.logs.len(), 1);
    assert_eq!(trace.logs[0].text, "bonus 25.00");
}

/// The canonical use of a hero setting: gate a rule by the lobby hero choice.
const HERO_GATE: &str = r#"
variables {
    global:
        0: same
}

rule ("gate") {
    event {
        Player Dealt Final Blow;
        All;
        All;
    }
    actions {
        Set Global Variable(same, Compare(Workshop Setting Hero(Custom String("cat"), Custom String("hero"), Hero(Ana), 0), ==, Hero Of(Event Player)));
    }
}
"#;

#[test]
fn a_hero_setting_compares_equal_to_the_played_hero() {
    // The scenario spelling does not matter: `Hero Of` and the setting both
    // yield the catalog member id.
    for hero in ["Ana", "ana", "ANA"] {
        let scenario = format!(
            r#"{{ "scenarioVersion": 1, "seconds": 1,
                 "players": [{{ "id": "p1", "hero": "{hero}" }}],
                 "events": [{{ "kind": "playerDealtFinalBlow", "time": 0, "eventPlayer": "p1" }}] }}"#
        );
        let trace = run_scenario(HERO_GATE, &scenario)
            .unwrap_or_else(|error| panic!("hero {hero}: {error}"));
        assert_eq!(trace.outcome, Outcome::Complete, "hero {hero}");
        assert_eq!(trace.globals["same"], Val::Bool(true), "hero {hero}");
    }
}
