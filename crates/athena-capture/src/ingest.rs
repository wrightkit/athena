use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::expect::{Expect, Verdict};
use crate::{Error, io};

/// What an `ingest` saved and concluded.
#[derive(Debug)]
pub struct Report {
    pub capture: PathBuf,
    /// The verdict of every saved capture, in file order.
    pub verdicts: Vec<Verdict>,
    /// The `result` now recorded in `capture.json`.
    pub result: &'static str,
}

fn capture_path(dir: &Path, n: usize) -> PathBuf {
    match n {
        1 => dir.join("capture.txt"),
        n => dir.join(format!("capture-{n}.txt")),
    }
}

fn saved_captures(dir: &Path) -> Vec<PathBuf> {
    (1..)
        .map(|n| capture_path(dir, n))
        .take_while(|path| path.exists())
        .collect()
}

fn read(path: &Path) -> Result<String, Error> {
    fs::read_to_string(path).map_err(io(path))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Error> {
    serde_json::from_str(&read(path)?).map_err(|error| Error::Format {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

/// A result is final only when at least three runs agree.
fn result_of(verdicts: &[Verdict]) -> &'static str {
    match verdicts {
        [first, _, _, ..] if verdicts.iter().all(|v| v == first) => match first {
            Verdict::Match => "match",
            Verdict::Mismatch => "mismatch",
        },
        _ => "inconclusive",
    }
}

fn verdicts(dir: &Path, expect: &Expect) -> Result<Vec<Verdict>, Error> {
    saved_captures(dir)
        .iter()
        .map(|path| {
            let text = read(path)?;
            let run = expect.run(&text).map_err(|error| Error::Format {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
            Ok(expect.judge(&run))
        })
        .collect()
}

/// Judge the saved captures of the probe in `dir`.
pub fn judge_captures(dir: &Path) -> Result<(Vec<Verdict>, &'static str), Error> {
    let expect: Expect = read_json(&dir.join("expect.json"))?;
    let verdicts = verdicts(dir, &expect)?;
    let result = result_of(&verdicts);
    Ok((verdicts, result))
}

/// Save the single finished run in `log` as the probe's next capture and
/// update `runs` and `result` in `capture.json`. Nothing is written unless
/// the whole ingest is accepted.
pub fn ingest(dir: &Path, log: &str) -> Result<Report, Error> {
    let expect: Expect = read_json(&dir.join("expect.json"))?;
    let record_path = dir.join("capture.json");
    let record_text = read(&record_path)?;
    let record: Value = read_json(&record_path)?;

    let program = fs::read(dir.join("probe.zh-CN.ow")).map_err(io(&dir.join("probe.zh-CN.ow")))?;
    let actual = format!("{:x}", Sha256::digest(&program));
    let recorded = record["probe_sha256"].as_str().unwrap_or_default();
    if recorded != actual {
        return Err(Error::Stale {
            recorded: recorded.to_string(),
            actual,
        });
    }

    let saved = saved_captures(dir).len() as u64;
    let runs = record["runs"].as_u64().unwrap_or_default();
    if runs != saved {
        return Err(Error::RunCount {
            recorded: runs,
            found: saved,
        });
    }

    let run = expect.run(log)?;
    let mut verdicts = verdicts(dir, &expect)?;
    verdicts.push(expect.judge(&run));
    let result = result_of(&verdicts);

    let capture = capture_path(dir, verdicts.len());
    let text: String = run.iter().map(|line| format!("{}\n", line.raw)).collect();
    let updated = set_top_level(&record_text, "runs", &verdicts.len().to_string())
        .and_then(|text| set_top_level(&text, "result", &format!("\"{result}\"")))
        .ok_or_else(|| Error::Format {
            path: record_path.display().to_string(),
            message: "expected one top-level \"runs\" and one \"result\" line".to_string(),
        })?;
    fs::write(&capture, text).map_err(io(&capture))?;
    fs::write(&record_path, updated).map_err(io(&record_path))?;

    Ok(Report {
        capture,
        verdicts,
        result,
    })
}

/// Replace the value of the one two-space-indented `"key": value` line,
/// keeping the rest of the file, its key order and its formatting.
fn set_top_level(text: &str, key: &str, value: &str) -> Option<String> {
    let prefix = format!("  \"{key}\": ");
    let mut found = 0;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if line.starts_with(&prefix) {
                found += 1;
                let comma = if line.ends_with(',') { "," } else { "" };
                format!("{prefix}{value}{comma}")
            } else {
                line.to_string()
            }
        })
        .collect();
    (found == 1).then(|| lines.join("\n") + "\n")
}
