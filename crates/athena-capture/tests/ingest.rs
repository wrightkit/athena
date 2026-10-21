use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use athena_capture::{Error, ingest, judge_captures};

fn probes() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../probes")
}

fn probes_with_expectations() -> Vec<PathBuf> {
    let mut dirs: Vec<_> = fs::read_dir(probes())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|dir| dir.join("expect.json").exists())
        .collect();
    dirs.sort();
    dirs
}

/// A copy of a probe without its captures, with `runs` reset to 0.
fn fresh_probe(id: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{id}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
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
    dir
}

fn recorded(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(dir.join("capture.json")).unwrap()).unwrap()
}

fn recorded_capture(id: &str, file: &str) -> String {
    fs::read_to_string(probes().join(id).join(file)).unwrap()
}

#[test]
fn the_saved_captures_of_every_expecting_probe_give_the_recorded_result() {
    let dirs = probes_with_expectations();
    assert!(!dirs.is_empty());
    for dir in dirs {
        let (verdicts, result) = judge_captures(&dir).unwrap();
        let record = recorded(&dir);
        assert_eq!(
            result,
            record["result"].as_str().unwrap(),
            "{}",
            dir.display()
        );
        assert_eq!(
            verdicts.len() as u64,
            record["runs"].as_u64().unwrap(),
            "{}",
            dir.display()
        );
    }
}

#[test]
fn three_agreeing_runs_make_the_result_final() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let files = ["capture.txt", "capture-2.txt", "capture-3.txt"];
    for (index, file) in files.iter().enumerate() {
        let report = ingest(&dir, &recorded_capture(id, file)).unwrap();
        let expected = if index == 2 { "match" } else { "inconclusive" };
        assert_eq!(report.result, expected);
        assert_eq!(recorded(&dir)["runs"], index as u64 + 1);
        assert_eq!(recorded(&dir)["result"], expected);
    }
    for file in files {
        assert!(dir.join(file).exists(), "{file}");
    }
}

#[test]
fn a_saved_capture_keeps_the_log_lines_verbatim() {
    let id = "p0-timer-resolution";
    let dir = fresh_probe(id);
    ingest(&dir, &recorded_capture(id, "capture.txt")).unwrap();
    let saved = fs::read_to_string(dir.join("capture.txt")).unwrap();
    let logged: Vec<&str> = saved.lines().collect();
    assert_eq!(logged.len(), 8);
    assert!(logged[0].starts_with("[00:00:03] P0 i=0 "));
    for line in logged {
        assert!(
            recorded_capture(id, "capture.txt")
                .lines()
                .any(|l| l == line)
        );
    }
}

#[test]
fn a_run_that_differs_from_the_expectation_is_a_mismatch() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let wrong = recorded_capture(id, "capture.txt").replace(
        "requested_ms=100 elapsed_ms=112",
        "requested_ms=100 elapsed_ms=96",
    );
    for _ in 0..3 {
        ingest(&dir, &wrong).unwrap();
    }
    assert_eq!(recorded(&dir)["result"], "mismatch");
}

#[test]
fn runs_that_disagree_stay_inconclusive() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let right = recorded_capture(id, "capture.txt");
    let wrong = right.replace("elapsed_ms=112", "elapsed_ms=96");
    for log in [&right, &wrong, &right] {
        ingest(&dir, log).unwrap();
    }
    assert_eq!(recorded(&dir)["result"], "inconclusive");
    assert_eq!(recorded(&dir)["runs"], 3);
}

#[test]
fn a_rejected_log_changes_nothing() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let full = recorded_capture(id, "capture.txt");
    let before = fs::read_to_string(dir.join("capture.json")).unwrap();

    let truncated: String = full.lines().take(8).map(|l| format!("{l}\n")).collect();
    let truncated = truncated.replace("requested_ms=500", "requested_ms=100");
    assert!(matches!(
        ingest(&dir, &truncated),
        Err(Error::Incomplete(_))
    ));
    assert!(matches!(
        ingest(&dir, &format!("{full}{full}")),
        Err(Error::SeveralRuns(2))
    ));
    assert!(matches!(ingest(&dir, ""), Err(Error::Incomplete(_))));

    assert_eq!(
        fs::read_to_string(dir.join("capture.json")).unwrap(),
        before
    );
    assert!(!dir.join("capture.txt").exists());
}

#[test]
fn a_changed_probe_makes_ingest_refuse() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let mut program = fs::read(dir.join("probe.zh-CN.ow")).unwrap();
    program.push(b'\n');
    fs::write(dir.join("probe.zh-CN.ow"), program).unwrap();
    assert!(matches!(
        ingest(&dir, &recorded_capture(id, "capture.txt")),
        Err(Error::Stale { .. })
    ));
}

#[test]
fn ingest_edits_only_runs_and_result_in_capture_json() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let before = fs::read_to_string(dir.join("capture.json")).unwrap();
    ingest(&dir, &recorded_capture(id, "capture.txt")).unwrap();
    let after = fs::read_to_string(dir.join("capture.json")).unwrap();
    let kept = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.starts_with("  \"runs\": ") && !l.starts_with("  \"result\": "))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(kept(&before), kept(&after));
    assert!(after.contains("  \"runs\": 1,"));
}
