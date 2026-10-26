use std::fs;
use std::path::PathBuf;

use athena_sim::{
    Options, Outcome, Trace, Val, run_scenario, run_scenario_with_hero_values, run_text,
};

const LOG_TOLERANCE: f64 = 0.0051;
const VARIABLE_TOLERANCE: f64 = 0.0006;
const CAPTURES: [&str; 3] = ["capture.txt", "capture-2.txt", "capture-3.txt"];

fn probe_file(id: &str, file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../probes")
        .join(id)
        .join(file)
}

fn read(id: &str, file: &str) -> String {
    let path = probe_file(id, file);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

struct Capture {
    variables: Vec<(String, String)>,
    logs: Vec<String>,
}

fn parse_capture(text: &str) -> Capture {
    let mut variables = Vec::new();
    let mut logs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("全局.") {
            let (name, value) = rest.trim_end_matches(';').split_once(" = ").unwrap();
            variables.push((name.trim().to_string(), value.trim().to_string()));
        } else if line.starts_with("[00:") {
            let (_, message) = line.split_once("] ").unwrap();
            logs.push(message.to_string());
        }
    }
    Capture { variables, logs }
}

#[derive(Debug, PartialEq)]
enum Token {
    Text(String),
    Number(f64),
}

fn tokens(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut text_run = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if !text_run.is_empty() {
                out.push(Token::Text(std::mem::take(&mut text_run)));
            }
            let number: String = chars[start..i].iter().collect();
            out.push(Token::Number(number.parse().unwrap()));
        } else {
            text_run.push(chars[i]);
            i += 1;
        }
    }
    if !text_run.is_empty() {
        out.push(Token::Text(text_run));
    }
    out
}

fn same_log(simulated: &str, captured: &str) -> bool {
    let (a, b) = (tokens(simulated), tokens(captured));
    a.len() == b.len()
        && a.iter().zip(&b).all(|pair| match pair {
            (Token::Text(x), Token::Text(y)) => x == y,
            (Token::Number(x), Token::Number(y)) => (x - y).abs() <= LOG_TOLERANCE,
            _ => false,
        })
}

/// A pasted Inspector export carries the 变量/动作 blocks; a capture saved
/// by `athena ingest` holds log lines only.
fn has_variable_block(text: &str) -> bool {
    text.lines()
        .any(|line| matches!(line.trim(), "变量" | "动作" | "全局:"))
}

fn is_zero_like(value: &Val) -> bool {
    matches!(value, Val::Null | Val::Bool(false)) || matches!(value, Val::Number(n) if *n == 0.0)
}

/// Variables in the export must agree; any variable the export omits must be
/// zero-like, because the client leaves out variables whose value is zero.
fn compare_variables(trace: &Trace, captured: &Capture) -> Result<(), String> {
    for (name, expected) in &captured.variables {
        let actual = trace
            .globals
            .get(name)
            .ok_or_else(|| format!("variable {name} missing from simulation"))?;
        let ok = match (expected.as_str(), actual) {
            ("真", Val::Bool(value)) => *value,
            ("假", Val::Bool(value)) => !*value,
            ("假", other) => is_zero_like(other),
            (number, Val::Number(value)) => number
                .parse::<f64>()
                .is_ok_and(|n| (n - value).abs() <= VARIABLE_TOLERANCE),
            _ => false,
        };
        if !ok {
            return Err(format!(
                "variable {name}: capture {expected}, simulated {actual:?}"
            ));
        }
    }
    for (name, value) in &trace.globals {
        let exported = captured.variables.iter().any(|(n, _)| n == name);
        if !exported && !is_zero_like(value) {
            return Err(format!(
                "variable {name} = {value:?} is absent from the capture"
            ));
        }
    }
    Ok(())
}

fn compare_logs(trace: &Trace, captured: &Capture) -> Result<(), String> {
    let simulated: Vec<&str> = trace.logs.iter().map(|entry| entry.text.as_str()).collect();
    if simulated.len() != captured.logs.len() {
        return Err(format!(
            "{} simulated log lines, {} captured\nsimulated: {simulated:#?}\ncaptured: {:#?}",
            simulated.len(),
            captured.logs.len(),
            captured.logs
        ));
    }
    for (index, (sim, cap)) in simulated.iter().zip(&captured.logs).enumerate() {
        if !same_log(sim, cap) {
            return Err(format!(
                "log line {index}: simulated {sim:?}, captured {cap:?}"
            ));
        }
    }
    Ok(())
}

const RUN_SECONDS: f64 = 10.0;

fn check_probe(id: &str) {
    let options = Options {
        seconds: RUN_SECONDS,
    };
    // A probe that needs a real player carries a `scenario.json`; without
    // one `run_text` runs the bare program.
    let scenario = probe_file(id, "scenario.json")
        .exists()
        .then(|| read(id, "scenario.json"));
    let simulate = |file: &str| {
        let text = read(id, file);
        let trace = match &scenario {
            Some(scenario) => {
                run_scenario(&text, scenario).unwrap_or_else(|error| panic!("{id} {file}: {error}"))
            }
            None => run_text(&text, options).unwrap_or_else(|error| panic!("{id} {file}: {error}")),
        };
        assert_eq!(trace.outcome, Outcome::Complete, "{id} {file}");
        trace
    };
    let english = simulate("probe.workshop");
    let chinese = simulate("probe.zh-CN.ow");
    assert_eq!(
        english, chinese,
        "{id}: the two probe spellings must simulate identically"
    );
    for file in CAPTURES {
        // Compare only the captures ingested so far; a probe between its
        // first and third run has fewer than three files.
        if !probe_file(id, file).exists() {
            continue;
        }
        let text = read(id, file);
        let captured = parse_capture(&text);
        let result = compare_logs(&english, &captured).and_then(|()| {
            // A capture saved by `athena ingest` holds log lines only — no
            // variable block — so there are no variables to verify against.
            if has_variable_block(&text) {
                compare_variables(&english, &captured)
            } else {
                Ok(())
            }
        });
        if let Err(message) = result {
            panic!("{id} vs {file}: {message}");
        }
    }
}

#[test]
fn p0_timer_resolution() {
    check_probe("p0-timer-resolution");
}

#[test]
fn p1_wait_rounding() {
    check_probe("p1-wait-rounding");
}

#[test]
fn p2_edge_trigger_and_wait_block() {
    check_probe("p2-edge-trigger-and-wait-block");
}

#[test]
fn p3_same_tick_rule_order() {
    check_probe("p3-same-tick-rule-order");
}

#[test]
fn p3_same_tick_rule_order_driver_last() {
    check_probe("p3-same-tick-rule-order-driver-last");
}

#[test]
fn p4_wait_until_zero_tick() {
    check_probe("p4-wait-until-zero-tick");
}

#[test]
fn p6_wait_behavior() {
    check_probe("p6-wait-behavior");
}

#[test]
#[ignore = "no captures yet (`runs` is 0)"]
fn p7_hero_settings() {
    check_probe("p7-hero-settings");
}

/// P7 is `inconclusive`: it has no captures. This test does not assert
/// client behavior; it checks the simulator's modeled player state produces
/// the hypothesis lines in `expect.json`, and that a bare `run_text` — no
/// scenario, so no player — reports the probe `not_modeled`. The values are
/// test-loaded so the check does not wait on a workshop-rs release.
#[test]
fn p7_hero_settings_models_the_expected_lines() {
    // `ana` as the pinned file records her: a 250 health pool, a 14-second
    // `ability1` cooldown and a 2300-point ultimate.
    let values = workshop_rs::hero_values::load(
        r#"{
            "format": "workshop-rs/hero-values-v1",
            "balanceVersion": "test-only",
            "heroes": [
                { "id": "ana", "values": [
                    { "kind": "health", "pool": "health", "amount": 250 },
                    { "kind": "cooldown", "slot": "ability1", "seconds": 14 },
                    { "kind": "ultimateCost", "points": 2300 }
                ] }
            ]
        }"#,
    )
    .expect("the test hero values load");
    let expected: serde_json::Value =
        serde_json::from_str(&read("p7-hero-settings", "expect.json")).unwrap();
    let scenario = read("p7-hero-settings", "scenario.json");
    let trace = run_scenario_with_hero_values(
        &read("p7-hero-settings", "probe.workshop"),
        &scenario,
        &values,
    )
    .unwrap_or_else(|error| panic!("p7-hero-settings: {error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    for (log, line) in trace.logs.iter().zip(expected["lines"].as_array().unwrap()) {
        assert!(
            same_log(&log.text, line.as_str().unwrap()),
            "simulated {:?}, expected {line}",
            log.text,
        );
    }
    assert_eq!(
        trace.logs.len(),
        expected["lines"].as_array().unwrap().len()
    );

    let trace = run_text(
        &read("p7-hero-settings", "probe.workshop"),
        Options {
            seconds: RUN_SECONDS,
        },
    )
    .unwrap();
    let Outcome::NotModeled(_) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
}

#[test]
fn p5_needs_a_player_world_and_is_reported_not_modeled() {
    let text = read("p5-value-refresh-rate", "probe.workshop");
    let trace = run_text(
        &text,
        Options {
            seconds: RUN_SECONDS,
        },
    )
    .unwrap();
    let Outcome::NotModeled(not_modeled) = &trace.outcome else {
        panic!("expected NotModeled, got {:?}", trace.outcome);
    };
    assert!(!not_modeled.construct.is_empty());
}

#[test]
fn trace_json_is_the_documented_shape() {
    let text = read("p5-value-refresh-rate", "probe.workshop");
    let trace = run_text(
        &text,
        Options {
            seconds: RUN_SECONDS,
        },
    )
    .unwrap();
    let json = serde_json::to_value(&trace).unwrap();
    assert_eq!(json["outcome"]["status"], "not_modeled");
    assert!(json["outcome"]["construct"].is_string());
    assert!(json["logs"].is_array());
    assert!(json["globals"].is_object());
}
