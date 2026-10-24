use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::expect::{Expect, Verdict};
use crate::ingest::{Outcome, ingest};
use crate::log::lines;
use crate::{Error, io};

/// How often `watch` looks at the log directory again.
const POLL: Duration = Duration::from_secs(1);

/// One ingest or skip `watch` reports, one line each.
#[derive(Debug)]
pub enum Event {
    /// The log's run was saved as the probe's next capture.
    Ingested {
        probe: String,
        log: PathBuf,
        result: &'static str,
    },
    /// The log's file name was already in the probe's `sources`.
    Duplicate { probe: String, log: PathBuf },
    /// The log names a probe's end line but no run could be recorded.
    Skipped { log: PathBuf, reason: String },
}

impl Event {
    /// The line `watch` prints for the event.
    pub fn line(&self) -> String {
        match self {
            Event::Ingested { probe, log, result } => {
                format!("{probe}: ingested {} (result {result})", name(log))
            }
            Event::Duplicate { probe, log } => {
                format!("{probe}: {} already ingested, skipped", name(log))
            }
            Event::Skipped { log, reason } => format!("{}: skipped ({reason})", name(log)),
        }
    }
}

fn name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Every `probes/*/expect.json` under `probes_dir`, sorted by probe dir.
pub fn load_expects(probes_dir: &Path) -> Result<Vec<(PathBuf, Expect)>, Error> {
    let mut expects = Vec::new();
    for entry in fs::read_dir(probes_dir).map_err(io(probes_dir))? {
        let dir = entry.map_err(io(probes_dir))?.path();
        let file = dir.join("expect.json");
        if !(dir.is_dir() && file.exists()) {
            continue;
        }
        let text = fs::read_to_string(&file).map_err(io(&file))?;
        let expect: Expect = serde_json::from_str(&text).map_err(|error| Error::Format {
            path: file.display().to_string(),
            message: error.to_string(),
        })?;
        expects.push((dir, expect));
    }
    expects.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(expects)
}

/// One poll of `log_dir`: for every file that newly contains a probe's
/// `end` line, ingest one run per probe. `attempted` remembers the
/// (file, probe) pairs already handled so a rescan stays quiet; a file
/// without an end line is left for the next poll in case it is still
/// being written.
pub fn scan_once(
    log_dir: &Path,
    expects: &[(PathBuf, Expect)],
    attempted: &mut HashSet<(PathBuf, PathBuf)>,
) -> Vec<Event> {
    let mut events = Vec::new();
    let Ok(entries) = fs::read_dir(log_dir) else {
        return events;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    for path in paths {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let messages: Vec<&str> = lines(&text).iter().map(|line| line.message).collect();
        // Candidate probes are the ones whose end line the file holds;
        // several probes can share one end line (the P3 pair).
        let mut by_end: BTreeMap<&str, Vec<&(PathBuf, Expect)>> = BTreeMap::new();
        for candidate in expects {
            if messages
                .iter()
                .any(|message| message.starts_with(&candidate.1.end))
            {
                by_end
                    .entry(candidate.1.end.as_str())
                    .or_default()
                    .push(candidate);
            }
        }
        for candidates in by_end.into_values() {
            let todo: Vec<_> = candidates
                .into_iter()
                .filter(|(dir, _)| !attempted.contains(&(path.clone(), dir.clone())))
                .collect();
            if todo.is_empty() {
                continue;
            }
            let source = name(&path);
            if let [(dir, _)] = todo[..] {
                events.push(try_ingest(dir, &path, &text, &source));
                attempted.insert((path.clone(), dir.clone()));
                continue;
            }
            // Shared end line: only the probe whose expected lines match
            // owns the run. Zero or several matches is ambiguous; the file
            // is skipped rather than recorded under the wrong probe.
            let matching: Vec<_> = todo
                .iter()
                .filter(|(_, expect)| {
                    expect
                        .run(&text)
                        .and_then(|run| expect.judge(&run))
                        .is_ok_and(|verdict| verdict == Verdict::Match)
                })
                .collect();
            for (dir, _) in &todo {
                attempted.insert((path.clone(), (*dir).clone()));
            }
            if let [(dir, _)] = matching[..] {
                events.push(try_ingest(dir, &path, &text, &source));
            } else {
                let names: Vec<String> = todo.iter().map(|(dir, _)| name(dir)).collect();
                events.push(Event::Skipped {
                    log: path.clone(),
                    reason: format!("no unique owner among {}", names.join(", ")),
                });
            }
        }
    }
    events
}

fn try_ingest(dir: &Path, path: &Path, text: &str, source: &str) -> Event {
    match ingest(dir, text, source) {
        Ok(Outcome::Saved(report)) => Event::Ingested {
            probe: name(dir),
            log: path.to_path_buf(),
            result: report.result,
        },
        Ok(Outcome::Skipped) => Event::Duplicate {
            probe: name(dir),
            log: path.to_path_buf(),
        },
        Err(error) => Event::Skipped {
            log: path.to_path_buf(),
            reason: error.to_string(),
        },
    }
}

/// Poll `log_dir` for Inspector logs and ingest each finished run into the
/// probes under `probes_dir`. Stops on stdin EOF (Ctrl-D) or Ctrl-C.
pub fn watch(log_dir: &Path, probes_dir: &Path) -> Result<(), Error> {
    let expects = load_expects(probes_dir)?;
    let mut attempted = HashSet::new();
    // stdin reaching EOF stops the loop; a channel carries the signal so
    // the poll interval can be a timed wait.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for _ in std::io::stdin().lock().lines() {}
        let _ = tx.send(());
    });
    println!("watching {} — Ctrl-C or stdin EOF stops", log_dir.display());
    loop {
        for event in scan_once(log_dir, &expects, &mut attempted) {
            println!("{}", event.line());
        }
        match rx.recv_timeout(POLL) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            _ => break,
        }
    }
    Ok(())
}
