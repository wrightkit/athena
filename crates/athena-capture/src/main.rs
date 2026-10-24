use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use athena_capture::{Outcome, Verdict, ingest_file, watch};
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
        Command::Ingest { probe, log } => ingest(&probe, &log),
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

fn ingest(probe: &Path, log: &Path) -> ExitCode {
    match ingest_file(probe, log).map_err(|error| error.to_string()) {
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
