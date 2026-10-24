use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use athena_capture::{Outcome, Verdict, foreign_end, ingest, load_expects, watch};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "Record probe captures from the Workshop Inspector log")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Save the finished run in an Inspector log as the probe's next capture.
    Ingest {
        /// The probe directory, e.g. probes/p1-wait-rounding.
        probe: PathBuf,
        /// The Inspector log file holding one run of the probe.
        log: PathBuf,
        /// The directory holding the probe directories, for the
        /// cross-probe guard. The check is skipped if it does not exist.
        #[arg(long, default_value = "probes")]
        probes: PathBuf,
    },
    /// Watch a directory of Inspector logs; ingest each finished run.
    /// Stops on Ctrl-C or stdin EOF.
    Watch {
        /// The directory holding Inspector log files.
        log_dir: PathBuf,
        /// The directory holding the probe directories.
        #[arg(long, default_value = "probes")]
        probes: PathBuf,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Ingest { probe, log, probes } => ingest_command(&probe, &log, &probes),
        Command::Watch { log_dir, probes } => report(watch(&log_dir, &probes)),
    }
}

fn report(outcome: Result<(), athena_capture::Error>) -> ExitCode {
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// A log holding a different probe's end line would save a run poisoned by
/// the other probe's lines; refuse it. The check is best-effort: it needs
/// the probes dir, and a missing or unreadable one just skips it.
fn guard_foreign_run(
    log: &Path,
    text: &str,
    probe: &Path,
    probes_dir: &Path,
) -> Result<(), String> {
    let Ok(expects) = load_expects(probes_dir) else {
        return Ok(());
    };
    match foreign_end(text, probe, &expects) {
        Some(other) => Err(format!(
            "{} holds a run of probe {other}; split it first or use `athena watch`",
            log.display()
        )),
        None => Ok(()),
    }
}

fn ingest_command(probe: &Path, log: &Path, probes_dir: &Path) -> ExitCode {
    let outcome = fs::read_to_string(log)
        .map_err(|error| format!("{}: {error}", log.display()))
        .and_then(|text| {
            guard_foreign_run(log, &text, probe, probes_dir)?;
            let source = log
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            ingest(probe, &text, &source).map_err(|error| error.to_string())
        });
    match outcome {
        Ok(Outcome::Saved(report)) => {
            println!("saved {}", report.capture.display());
            for (index, verdict) in report.verdicts.iter().enumerate() {
                let name = match verdict {
                    Verdict::Match => "match",
                    Verdict::Mismatch => "mismatch",
                };
                println!("run {}: {name}", index + 1);
            }
            println!("result: {}", report.result);
            ExitCode::SUCCESS
        }
        Ok(Outcome::Skipped) => {
            println!("skipped: {} was already ingested", log.display());
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
