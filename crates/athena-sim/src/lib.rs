//! Scheduler simulator for Workshop rules.

mod eval;
mod sched;

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

/// A Workshop value as the simulator sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Val {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Val>),
}

/// One `Log To Inspector` line, with the tick it was written on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub tick: u64,
    pub text: String,
}

/// A construct the simulator does not model. The run stops at it; nothing is
/// defaulted or skipped.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotModeled {
    /// What was met, for example `action wait` or `value getPosition`.
    pub construct: String,
    /// The tick on which it was met; `None` when found before the first tick.
    pub tick: Option<u64>,
}

impl NotModeled {
    pub(crate) fn new(construct: impl Into<String>) -> Self {
        Self {
            construct: construct.into(),
            tick: None,
        }
    }

    pub(crate) fn at(mut self, tick: u64) -> Self {
        self.tick = Some(tick);
        self
    }
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    /// The whole requested time was simulated.
    Complete,
    /// The run stopped at a construct the simulator does not model.
    NotModeled(NotModeled),
}

/// What a run produced. `logs` and `globals` describe the state when the run
/// ended: after the last tick for `Complete`, at the stop for `NotModeled`.
/// Consumers must check `outcome` before using either.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trace {
    pub outcome: Outcome,
    pub logs: Vec<LogEntry>,
    pub globals: BTreeMap<String, Val>,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Simulated seconds to run, counted from the first tick.
    pub seconds: f64,
}

/// The text did not parse as Workshop, so there is nothing to simulate.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

/// Parse Workshop text (any supported locale) and run it.
///
/// A program that parses always yields a [`Trace`]. Behavior outside the model
/// is reported in [`Trace::outcome`], never defaulted.
pub fn run_text(text: &str, options: Options) -> Result<Trace, ParseError> {
    let parse = |error: &dyn fmt::Display| ParseError(error.to_string());
    let catalog = workshop_rs::catalog::Catalog::builtin_ref().map_err(|e| parse(&e))?;
    let locale = workshop_rs::detect::resolve_locale(text, catalog, None).map_err(|e| parse(&e))?;
    let program = workshop_rs::parser::parse(text, catalog, &locale).map_err(|e| parse(&e))?;
    Ok(sched::run(&program, options))
}
