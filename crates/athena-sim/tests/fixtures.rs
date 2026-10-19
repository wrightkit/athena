//! Replays every probe whose capture matched the hypothesis and compares the
//! simulator with the verbatim Inspector text saved from the client.

use std::fs;
use std::path::PathBuf;

use athena_sim::{Options, SimError, Trace, Val, run_text};

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

/// Split a verbatim Inspector export into its variable block and log lines.
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
    let english = run_text(&read(id, "probe.workshop"), options)
        .unwrap_or_else(|error| panic!("{id} probe.workshop: {error}"));
    let chinese = run_text(&read(id, "probe.zh-CN.ow"), options)
        .unwrap_or_else(|error| panic!("{id} probe.zh-CN.ow: {error}"));
    assert_eq!(
        english, chinese,
        "{id}: the two probe spellings must simulate identically"
    );
    for file in CAPTURES {
        let captured = parse_capture(&read(id, file));
        if let Err(message) =
            compare_logs(&english, &captured).and(compare_variables(&english, &captured))
        {
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
#[ignore = "needs Wait Until (layer 3)"]
fn p4_wait_until_zero_tick() {
    check_probe("p4-wait-until-zero-tick");
}

#[test]
#[ignore = "needs Wait Until and Wait Behavior (layer 3)"]
fn p6_wait_behavior() {
    check_probe("p6-wait-behavior");
}

#[test]
fn p5_needs_a_player_world_and_is_reported_unsupported() {
    let text = read("p5-value-refresh-rate", "probe.workshop");
    let error = run_text(
        &text,
        Options {
            seconds: RUN_SECONDS,
        },
    )
    .unwrap_err();
    assert!(matches!(error, SimError::Unsupported(_)), "{error}");
}
