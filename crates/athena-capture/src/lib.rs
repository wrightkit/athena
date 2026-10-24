//! Records probe captures from the Workshop Inspector log and judges them
//! against the probe's `expect.json` (`docs/capture-convention.md`).

mod expect;
mod ingest;
mod log;
mod watch;

pub use expect::{Expect, Verdict, VerdictSpec};
pub use ingest::{Outcome, Report, ingest, judge_captures, result_of};
pub use watch::{Event, ScanState, foreign_end, load_expects, scan_once, watch};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    Format { path: String, message: String },
    #[error("the log has no complete run: no line starting with {0:?} ends it")]
    Incomplete(String),
    #[error("the run's end line is not last: {extra} probe lines follow it")]
    EndNotLast { extra: usize },
    #[error("the log holds {0} runs; give it one run")]
    SeveralRuns(usize),
    #[error("the run is invalid: {0}")]
    Invalid(String),
    #[error(
        "capture.json records probe_sha256 {recorded}, but probe.zh-CN.ow is {actual}; the saved captures are stale"
    )]
    Stale { recorded: String, actual: String },
    #[error("capture.json records {recorded} runs but {found} capture files exist")]
    RunCount { recorded: u64, found: u64 },
}

pub(crate) fn io(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.display().to_string(),
        source,
    }
}
