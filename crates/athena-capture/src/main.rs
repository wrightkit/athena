use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use athena_capture::{Verdict, ingest};
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
}

fn main() -> ExitCode {
    let Command::Ingest { probe, log } = Cli::parse().command;
    let outcome = fs::read_to_string(&log)
        .map_err(|error| format!("{}: {error}", log.display()))
        .and_then(|text| ingest(&probe, &text).map_err(|error| error.to_string()));
    match outcome {
        Ok(report) => {
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
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
