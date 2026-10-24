#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use athena_capture::{Outcome, Report, ingest};

pub fn probes() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../probes")
}

pub fn unique_dir(tag: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Copy `expect.json`, `probe.zh-CN.ow` and `capture.json` of probe `id`
/// into `dir`, without the captures and with `runs` reset to 0.
pub fn copy_probe(id: &str, dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    for file in ["probe.zh-CN.ow", "expect.json", "capture.json"] {
        fs::copy(probes().join(id).join(file), dir.join(file)).unwrap();
    }
    let record = fs::read_to_string(dir.join("capture.json")).unwrap();
    let reset: Vec<String> = record
        .lines()
        .map(|line| match line {
            l if l.starts_with("  \"runs\": ") => "  \"runs\": 0,".to_string(),
            l if l.starts_with("  \"result\": ") => "  \"result\": \"inconclusive\",".to_string(),
            l => l.to_string(),
        })
        .collect();
    fs::write(dir.join("capture.json"), reset.join("\n") + "\n").unwrap();
}

/// A fresh copy of probe `id` in a unique temp directory.
pub fn fresh_probe(id: &str) -> PathBuf {
    let dir = unique_dir(id);
    copy_probe(id, &dir);
    dir
}

/// Ingest expecting a saved run.
pub fn saved(dir: &Path, log: &str, source: &str) -> Report {
    match ingest(dir, log, source).unwrap() {
        Outcome::Saved(report) => report,
        Outcome::Skipped => panic!("{source} was unexpectedly skipped"),
    }
}

pub fn recorded(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(dir.join("capture.json")).unwrap()).unwrap()
}

pub fn recorded_capture(id: &str, file: &str) -> String {
    fs::read_to_string(probes().join(id).join(file)).unwrap()
}
