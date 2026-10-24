use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
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

/// Scan bookkeeping shared across polls.
#[derive(Default)]
pub struct ScanState {
    /// (file, probe) pairs that already recorded a run; a probe gets at
    /// most one run per file, so a later same-probe segment is left alone.
    attempted: HashSet<(PathBuf, PathBuf)>,
    /// (file, segment hash) already handled. Suppression keys on content:
    /// a file that is rewritten — or grows a new segment — is processed
    /// again, while an unchanged segment is never re-evaluated.
    reported: HashSet<(PathBuf, u64)>,
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

/// The name of a probe other than `probe` whose `end` token `text` holds,
/// if any. Probes are told apart by directory name — the convention names
/// a probe directory after its id.
pub fn foreign_end(text: &str, probe: &Path, expects: &[(PathBuf, Expect)]) -> Option<String> {
    let target = name(probe);
    let messages: Vec<&str> = lines(text).iter().map(|line| line.message).collect();
    expects
        .iter()
        .filter(|(dir, _)| name(dir) != target)
        .find(|(_, expect)| {
            messages
                .iter()
                .any(|message| message.starts_with(&expect.end))
        })
        .map(|(dir, _)| name(dir))
}

/// The probe whose expected `lines` the segment's run matches, when
/// several probes share the end line that closed it.
fn owner_of<'a>(
    expects: &'a [(PathBuf, Expect)],
    closers: &[usize],
    run_text: &str,
) -> Option<&'a (PathBuf, Expect)> {
    let matching: Vec<&(PathBuf, Expect)> = closers
        .iter()
        .map(|&index| &expects[index])
        .filter(|(_, expect)| {
            expect
                .run(run_text)
                .and_then(|run| expect.judge(&run))
                .is_ok_and(|verdict| verdict == Verdict::Match)
        })
        .collect();
    match matching[..] {
        [only] => Some(only),
        _ => None,
    }
}

/// One poll of `log_dir`. Each file is split into segments at every known
/// `end` line — a segment is owned by the probe whose `end` closes it —
/// and at most one run per probe is recorded from a file. Lines after the
/// last end line are a run still in progress and are left for the next
/// poll. `state` keeps the scan quiet and consistent across polls.
pub fn scan_once(
    log_dir: &Path,
    expects: &[(PathBuf, Expect)],
    state: &mut ScanState,
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
        let log_lines = lines(&text);
        // Close a segment at every line that is some probe's end. The run
        // inside a segment is judged on its own lines only, so same-prefix
        // probes cannot contaminate each other.
        let mut segments = Vec::new();
        let mut start = 0;
        for (index, line) in log_lines.iter().enumerate() {
            let closers: Vec<usize> = expects
                .iter()
                .enumerate()
                .filter(|(_, (_, expect))| line.message.starts_with(&expect.end))
                .map(|(index, _)| index)
                .collect();
            if !closers.is_empty() {
                segments.push((start, index, closers));
                start = index + 1;
            }
        }
        for (start, end, closers) in segments {
            let run_text: String = log_lines[start..=end]
                .iter()
                .map(|line| format!("{}\n", line.raw))
                .collect();
            // A segment already handled is not reported again; a file that
            // is rewritten gets fresh content and is processed again.
            let mut hash = DefaultHasher::new();
            run_text.hash(&mut hash);
            if !state.reported.insert((path.clone(), hash.finish())) {
                continue;
            }
            let owner = match closers[..] {
                [only] => Some(&expects[only]),
                _ => owner_of(expects, &closers, &run_text),
            };
            let Some((dir, _)) = owner else {
                let names: Vec<String> = closers
                    .iter()
                    .map(|&index| name(&expects[index].0))
                    .collect();
                events.push(Event::Skipped {
                    log: path.clone(),
                    reason: format!("no unique owner among {}", names.join(", ")),
                });
                continue;
            };
            if state.attempted.contains(&(path.clone(), dir.clone())) {
                continue;
            }
            let event = try_ingest(dir, &path, &run_text, &name(&path));
            // Only a recorded run spends the file's one slot for this
            // probe; a skipped segment can still be followed by a valid
            // later segment.
            if !matches!(event, Event::Skipped { .. }) {
                state.attempted.insert((path.clone(), dir.clone()));
            }
            events.push(event);
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
    // Fail fast on a missing or unreadable log dir instead of polling it
    // forever.
    fs::read_dir(log_dir).map_err(io(log_dir))?;
    let mut state = ScanState::default();
    // stdin reaching EOF stops the loop; a channel carries the signal so
    // the poll interval can be a timed wait.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for _ in std::io::stdin().lock().lines() {}
        let _ = tx.send(());
    });
    println!("watching {} — Ctrl-C or stdin EOF stops", log_dir.display());
    loop {
        for event in scan_once(log_dir, &expects, &mut state) {
            println!("{}", event.line());
        }
        match rx.recv_timeout(POLL) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            _ => break,
        }
    }
    Ok(())
}
