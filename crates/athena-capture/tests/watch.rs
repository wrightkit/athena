mod common;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use athena_capture::{Event, load_expects, scan_once};
use common::{copy_probe, recorded, recorded_capture, unique_dir};

/// A temp `probes/` tree with fresh copies of `ids`, and a `logs/` dir.
fn tree(ids: &[&str]) -> (PathBuf, PathBuf) {
    let root = unique_dir("watch");
    let probes = root.join("probes");
    let logs = root.join("logs");
    fs::create_dir_all(&logs).unwrap();
    for id in ids {
        copy_probe(id, &probes.join(id));
    }
    (probes, logs)
}

fn kinds(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .map(|event| match event {
            Event::Ingested { probe, .. } => format!("ingested:{probe}"),
            Event::Duplicate { probe, .. } => format!("duplicate:{probe}"),
            Event::Skipped { reason, .. } => format!("skipped:{reason}"),
        })
        .collect()
}

#[test]
fn a_finished_log_is_ingested_once_per_probe() {
    let (probes, logs) = tree(&["p1-wait-rounding"]);
    let probe = probes.join("p1-wait-rounding");
    let expects = load_expects(&probes).unwrap();
    let mut attempted = HashSet::new();

    assert!(scan_once(&logs, &expects, &mut attempted).is_empty());
    let capture = recorded_capture("p1-wait-rounding", "capture.txt");
    fs::write(logs.join("run-1.txt"), &capture).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut attempted)),
        ["ingested:p1-wait-rounding"]
    );
    assert_eq!(recorded(&probe)["runs"], 1);

    // A rescan with the same state stays quiet; a restarted watch reports
    // the dedup skip once.
    assert!(scan_once(&logs, &expects, &mut attempted).is_empty());
    let mut restarted = HashSet::new();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut restarted)),
        ["duplicate:p1-wait-rounding"]
    );

    // The same text under a new file name is another run.
    fs::write(logs.join("run-2.txt"), &capture).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut restarted)),
        ["ingested:p1-wait-rounding"]
    );
    assert_eq!(recorded(&probe)["runs"], 2);
}

#[test]
fn a_log_without_an_end_line_waits_for_the_next_scan() {
    let (probes, logs) = tree(&["p1-wait-rounding"]);
    let probe = probes.join("p1-wait-rounding");
    let expects = load_expects(&probes).unwrap();
    let mut attempted = HashSet::new();

    let full = recorded_capture("p1-wait-rounding", "capture.txt");
    let partial: String = full
        .lines()
        .take_while(|line| !line.contains("requested_ms=500"))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write(logs.join("run.txt"), &partial).unwrap();
    assert!(scan_once(&logs, &expects, &mut attempted).is_empty());
    assert_eq!(recorded(&probe)["runs"], 0);

    // The file keeps being written; once the end line lands it ingests.
    fs::write(logs.join("run.txt"), &full).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut attempted)),
        ["ingested:p1-wait-rounding"]
    );
}

#[test]
fn a_log_that_fails_to_parse_is_skipped_once() {
    let (probes, logs) = tree(&["p1-wait-rounding"]);
    let expects = load_expects(&probes).unwrap();
    let mut attempted = HashSet::new();
    let full = recorded_capture("p1-wait-rounding", "capture.txt");
    fs::write(logs.join("run.txt"), format!("{full}{full}")).unwrap();

    let events = scan_once(&logs, &expects, &mut attempted);
    match &events[..] {
        [Event::Skipped { reason, .. }] => assert!(reason.contains("2 runs"), "{reason}"),
        other => panic!("expected one Skipped, got {}", kinds(other).join(", ")),
    }
    assert!(scan_once(&logs, &expects, &mut attempted).is_empty());
    assert_eq!(recorded(&probes.join("p1-wait-rounding"))["runs"], 0);
}

/// Rewrite a copied probe's `end` (and its last expected line) to `token`.
fn retag_end(probe_dir: &std::path::Path, token: &str) {
    let file = probe_dir.join("expect.json");
    let text = fs::read_to_string(&file).unwrap();
    let end = text
        .lines()
        .find_map(|line| {
            line.trim()
                .trim_end_matches(',')
                .strip_prefix("\"end\": \"")
                .and_then(|rest| rest.strip_suffix('"'))
        })
        .unwrap()
        .to_string();
    fs::write(
        file,
        text.replace(&format!("\"{end}\""), &format!("\"{token}\"")),
    )
    .unwrap();
}

#[test]
fn the_p3_siblings_resolve_by_their_unique_end_lines() {
    let (probes, logs) = tree(&[
        "p3-same-tick-rule-order",
        "p3-same-tick-rule-order-driver-last",
    ]);
    let expects = load_expects(&probes).unwrap();
    assert_eq!(expects.len(), 2);
    assert_ne!(expects[0].1.end, expects[1].1.end);
    let mut attempted = HashSet::new();

    // The stale captures re-tagged with each probe's new end token.
    let original = recorded_capture("p3-same-tick-rule-order", "stale/capture.txt")
        .replace("P3 done", "P3a done");
    let driver_last = recorded_capture("p3-same-tick-rule-order-driver-last", "stale/capture.txt")
        .replace("P3 done", "P3b done");
    fs::write(logs.join("a.txt"), original).unwrap();
    fs::write(logs.join("b.txt"), driver_last).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut attempted)),
        [
            "ingested:p3-same-tick-rule-order",
            "ingested:p3-same-tick-rule-order-driver-last"
        ]
    );
    assert_eq!(recorded(&probes.join("p3-same-tick-rule-order"))["runs"], 1);
    assert_eq!(
        recorded(&probes.join("p3-same-tick-rule-order-driver-last"))["runs"],
        1
    );
}

#[test]
fn probes_sharing_an_end_line_are_attributed_by_their_expected_lines() {
    let (probes, logs) = tree(&[
        "p3-same-tick-rule-order",
        "p3-same-tick-rule-order-driver-last",
    ]);
    // The fallback path: force the two copies back to a shared `P3 done`.
    for id in [
        "p3-same-tick-rule-order",
        "p3-same-tick-rule-order-driver-last",
    ] {
        retag_end(&probes.join(id), "P3 done");
    }
    let expects = load_expects(&probes).unwrap();
    let mut attempted = HashSet::new();

    fs::write(
        logs.join("a.txt"),
        recorded_capture("p3-same-tick-rule-order", "stale/capture.txt"),
    )
    .unwrap();
    fs::write(
        logs.join("b.txt"),
        recorded_capture("p3-same-tick-rule-order-driver-last", "stale/capture.txt"),
    )
    .unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut attempted)),
        [
            "ingested:p3-same-tick-rule-order",
            "ingested:p3-same-tick-rule-order-driver-last"
        ]
    );

    // A P3 run matching neither expectation has no unique owner.
    let wrong = recorded_capture("p3-same-tick-rule-order", "stale/capture.txt")
        .replace("t_ms=0", "t_ms=999");
    fs::write(logs.join("c.txt"), wrong).unwrap();
    let events = scan_once(&logs, &expects, &mut attempted);
    match &events[..] {
        [Event::Skipped { reason, .. }] => {
            assert!(reason.contains("no unique owner"), "{reason}")
        }
        other => panic!("expected one Skipped, got {}", kinds(other).join(", ")),
    }
}
