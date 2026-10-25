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

/// What `ingest` did with a log.
#[derive(Debug)]
pub enum Outcome {
    /// The log's source name is already in `sources`; nothing was written
    /// and `runs` was not touched. Distinct runs can share byte-identical
    /// log text, so dedup keys on the source name, not the content.
    Skipped,
    /// The run was judged and written as the probe's next capture.
    Saved(Report),
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
pub fn result_of(verdicts: &[Verdict]) -> &'static str {
    match verdicts {
        [first, _, _, ..] if verdicts.iter().all(|v| v == first) => match first {
            Verdict::Match => "match",
            Verdict::Mismatch => "mismatch",
        },
        _ => "inconclusive",
    }
}

fn verdicts(dir: &Path, expect: &Expect) -> Result<Vec<Verdict>, Error> {
    judge_all(dir, expect, usize::MAX)
}

fn judge_all(dir: &Path, expect: &Expect, count: usize) -> Result<Vec<Verdict>, Error> {
    saved_captures(dir)
        .iter()
        .take(count)
        .map(|path| {
            let text = read(path)?;
            let run = expect.run(&text).map_err(|error| Error::Format {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
            expect.judge(&run)
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
/// update `runs`, `result`, `notes` and `sources` in `capture.json`.
/// Nothing is written unless the whole ingest is accepted. A crash between
/// the two writes leaves a stray file in the next slot while `runs` still
/// counts the earlier captures; that stray is overwritten on the next
/// ingest rather than counted as a run.
///
/// `source` names the log this run came from — the watched or given file
/// name. A source already recorded in `sources` is skipped; a `capture.json`
/// written before `sources` existed has an empty list.
pub fn ingest(dir: &Path, log: &str, source: &str) -> Result<Outcome, Error> {
    let expect: Expect = read_json(&dir.join("expect.json"))?;
    let record_path = dir.join("capture.json");
    let record_text = read(&record_path)?;
    let record: Value = serde_json::from_str(&record_text).map_err(|error| Error::Format {
        path: record_path.display().to_string(),
        message: error.to_string(),
    })?;

    let mut sources: Vec<String> = record["sources"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if sources.iter().any(|name| name == source) {
        return Ok(Outcome::Skipped);
    }

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
    // One file past `runs` is the crash leftover described above; more is
    // a corrupted directory.
    if saved != runs && saved != runs + 1 {
        return Err(Error::RunCount {
            recorded: runs,
            found: saved,
        });
    }

    let run = expect.run(log)?;
    // Only the recorded captures are judged; the possible stray is never
    // read as a run.
    let mut verdicts = judge_all(dir, &expect, runs as usize)?;
    verdicts.push(expect.judge(&run)?);
    let result = result_of(&verdicts);
    sources.push(source.to_string());

    let capture = capture_path(dir, runs as usize + 1);
    let text: String = run.iter().map(|line| format!("{}\n", line.raw)).collect();
    let notes = updated_notes(&record, result, &verdicts);
    let updated = set_top_level(&record_text, "runs", &verdicts.len().to_string())
        .and_then(|text| set_top_level(&text, "result", &format!("\"{result}\"")))
        .and_then(|text| set_top_level(&text, "notes", &json_string(&notes)))
        .and_then(|text| upsert_top_level(&text, "sources", &json_string(&sources)))
        .ok_or_else(|| Error::Format {
            path: record_path.display().to_string(),
            message: "expected the two-space capture.json shape with \"runs\", \"result\", \"notes\" and \"sources\""
                .to_string(),
        })?;
    fs::write(&capture, text).map_err(io(&capture))?;
    fs::write(&record_path, updated).map_err(io(&record_path))?;

    Ok(Outcome::Saved(Report {
        capture,
        verdicts,
        result,
    }))
}

fn json_string(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("strings serialize")
}

/// `notes` keeps its human text; `ingest` manages only a trailing
/// `[ingest: …]` marker so a provisional result says so until three runs
/// agree.
fn updated_notes(record: &Value, result: &str, verdicts: &[Verdict]) -> String {
    let human = strip_marker(record["notes"].as_str().unwrap_or_default());
    if result != "inconclusive" {
        return human.to_string();
    }
    let matching = verdicts.iter().filter(|v| **v == Verdict::Match).count();
    let agreeing = matching.max(verdicts.len() - matching);
    let marker = format!("[ingest: provisional, {agreeing}/3 runs agree]");
    if human.is_empty() {
        marker
    } else {
        format!("{human} {marker}")
    }
}

/// The note without its trailing `[ingest: …]` marker, if it has one.
fn strip_marker(notes: &str) -> &str {
    let end = notes.trim_end();
    let Some(at) = end.rfind("[ingest: ") else {
        return notes;
    };
    let tail = &end[at..];
    if tail.ends_with(']') && !tail[..tail.len() - 1].contains(']') {
        end[..at].trim_end()
    } else {
        notes
    }
}

/// Replace the value of the one two-space-indented `"key": value` line,
/// keeping the rest of the file, its key order and its formatting. The
/// convention's `capture.json` keeps that exact two-space shape; a file
/// that drifts (other indentation, two keys at that indent) fails instead
/// of being normalized silently. A value that spans several lines, as in a
/// pretty-printed array, is replaced whole: its lines run up to the next
/// two-space key or the closing brace.
fn set_top_level(text: &str, key: &str, value: &str) -> Option<String> {
    let prefix = format!("  \"{key}\": ");
    let is_key = |line: &str| line.starts_with("  \"") || line == "}";
    let source: Vec<&str> = text.lines().collect();
    let mut lines: Vec<String> = Vec::new();
    let mut found = 0;
    let mut i = 0;
    while i < source.len() {
        if source[i].starts_with(&prefix) {
            found += 1;
            let mut last = i;
            while source.get(last + 1).is_some_and(|line| !is_key(line)) {
                last += 1;
            }
            let comma = if source[last].ends_with(',') { "," } else { "" };
            lines.push(format!("{prefix}{value}{comma}"));
            i = last + 1;
        } else {
            lines.push(source[i].to_string());
            i += 1;
        }
    }
    (found == 1).then(|| lines.join("\n") + "\n")
}

/// Like `set_top_level`, but a key the file does not have yet is added as
/// the last field instead of failing. A duplicated key still fails.
fn upsert_top_level(text: &str, key: &str, value: &str) -> Option<String> {
    if text
        .lines()
        .any(|line| line.starts_with(&format!("  \"{key}\": ")))
    {
        return set_top_level(text, key, value);
    }
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let close = lines.iter().rposition(|line| line.trim() == "}")?;
    let prev = close.checked_sub(1)?;
    if !lines[prev].ends_with(',') {
        lines[prev].push(',');
    }
    lines.insert(close, format!("  \"{key}\": {value}"));
    Some(lines.join("\n") + "\n")
}
