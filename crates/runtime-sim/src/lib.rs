//! Scheduler simulator for Workshop rules.

mod eval;
mod sched;

use std::collections::BTreeMap;
use std::fmt;

/// A Workshop value as the simulator sees it.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Val>),
}

/// One `Log To Inspector` line, with the tick it was written on.
#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub tick: u64,
    pub text: String,
}

/// What a run produced: the Inspector log and the final global variables.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    pub logs: Vec<LogEntry>,
    pub globals: BTreeMap<String, Val>,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Simulated seconds to run, counted from the first tick.
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SimError {
    /// The text did not parse as Workshop.
    Parse(String),
    /// The program uses something the simulator does not model.
    Unsupported(String),
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "parse error: {message}"),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for SimError {}

/// Parse Workshop text (any supported locale) and run it.
pub fn run_text(text: &str, options: Options) -> Result<Trace, SimError> {
    let parse = |error: &dyn fmt::Display| SimError::Parse(error.to_string());
    let catalog = workshop_rs::catalog::Catalog::builtin_ref().map_err(|e| parse(&e))?;
    let locale = workshop_rs::detect::resolve_locale(text, catalog, None).map_err(|e| parse(&e))?;
    let program = workshop_rs::parser::parse(text, catalog, &locale).map_err(|e| parse(&e))?;
    sched::run(&program, options)
}
