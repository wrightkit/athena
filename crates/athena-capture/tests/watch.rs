mod common;

use std::fs;
use std::path::PathBuf;

use athena_capture::{Event, ScanState, foreign_end, load_expects, scan_once};
use common::{P5_START, copy_probe, p5_log, recorded, recorded_capture, unique_dir};

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
    let mut state = ScanState::default();

    assert!(scan_once(&logs, &expects, &mut state).is_empty());
    let capture = recorded_capture("p1-wait-rounding", "capture.txt");
    fs::write(logs.join("run-1.txt"), &capture).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut state)),
        ["ingested:p1-wait-rounding"]
    );
    assert_eq!(recorded(&probe)["runs"], 1);

    // A rescan with the same state stays quiet; a restarted watch reports
    // the dedup skip once.
    assert!(scan_once(&logs, &expects, &mut state).is_empty());
    let mut restarted = ScanState::default();
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
    let mut state = ScanState::default();

    let full = recorded_capture("p1-wait-rounding", "capture.txt");
    let partial: String = full
        .lines()
        .take_while(|line| !line.contains("requested_ms=500"))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write(logs.join("run.txt"), &partial).unwrap();
    assert!(scan_once(&logs, &expects, &mut state).is_empty());
    assert_eq!(recorded(&probe)["runs"], 0);

    // The file keeps being written; once the end line lands it ingests.
    fs::write(logs.join("run.txt"), &full).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut state)),
        ["ingested:p1-wait-rounding"]
    );
}

#[test]
fn a_file_with_two_runs_of_one_probe_records_the_first() {
    let (probes, logs) = tree(&["p1-wait-rounding"]);
    let probe = probes.join("p1-wait-rounding");
    let expects = load_expects(&probes).unwrap();
    let mut state = ScanState::default();
    let full = recorded_capture("p1-wait-rounding", "capture.txt");
    fs::write(logs.join("run.txt"), format!("{full}{full}")).unwrap();

    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut state)),
        ["ingested:p1-wait-rounding"]
    );
    assert_eq!(recorded(&probe)["runs"], 1);
    // The second segment stays untouched and a rescan stays quiet.
    assert!(scan_once(&logs, &expects, &mut state).is_empty());
    assert_eq!(recorded(&probe)["runs"], 1);
}

#[test]
fn a_skipped_segment_does_not_burn_a_later_valid_run() {
    let (probes, logs) = tree(&["p5-value-refresh-rate"]);
    let probe = probes.join("p5-value-refresh-rate");
    let expects = load_expects(&probes).unwrap();
    let mut state = ScanState::default();

    // The first segment is a run the verdict rejects (the player never
    // spawned); the valid run in the same file is still recorded.
    let bad = p5_log("P5 start spawned=假 pos=(1.00, 2.00, 3.00)", 188, 38);
    let good = p5_log(P5_START, 188, 38);
    fs::write(logs.join("runs.txt"), format!("{bad}{good}")).unwrap();
    let events = scan_once(&logs, &expects, &mut state);
    match &events[..] {
        [
            Event::Skipped { reason, .. },
            Event::Ingested { probe: p, .. },
        ] => {
            assert!(reason.contains("spawned"), "{reason}");
            assert_eq!(p, "p5-value-refresh-rate");
        }
        other => panic!(
            "expected Skipped then Ingested, got {}",
            kinds(other).join(", ")
        ),
    }
    assert_eq!(recorded(&probe)["runs"], 1);
    assert_eq!(recorded(&probe)["sources"], serde_json::json!(["runs.txt"]));
    assert!(scan_once(&logs, &expects, &mut state).is_empty());
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
    let mut state = ScanState::default();

    // The stale captures re-tagged with each probe's new end token.
    let original = recorded_capture("p3-same-tick-rule-order", "stale/capture.txt")
        .replace("P3 done", "P3a done");
    let driver_last = recorded_capture("p3-same-tick-rule-order-driver-last", "stale/capture.txt")
        .replace("P3 done", "P3b done");
    fs::write(logs.join("a.txt"), original).unwrap();
    fs::write(logs.join("b.txt"), driver_last).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut state)),
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
fn a_log_holding_two_sibling_runs_attributes_each_segment() {
    let (probes, logs) = tree(&[
        "p3-same-tick-rule-order",
        "p3-same-tick-rule-order-driver-last",
    ]);
    let expects = load_expects(&probes).unwrap();
    let mut state = ScanState::default();

    // One file, two complete runs back to back: same `P3` prefix, so each
    // segment must keep only the lines of its own run.
    let original = recorded_capture("p3-same-tick-rule-order", "stale/capture.txt")
        .replace("P3 done", "P3a done");
    let driver_last = recorded_capture("p3-same-tick-rule-order-driver-last", "stale/capture.txt")
        .replace("P3 done", "P3b done");
    fs::write(logs.join("mixed.txt"), format!("{original}{driver_last}")).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut state)),
        [
            "ingested:p3-same-tick-rule-order",
            "ingested:p3-same-tick-rule-order-driver-last"
        ]
    );
    for id in [
        "p3-same-tick-rule-order",
        "p3-same-tick-rule-order-driver-last",
    ] {
        let dir = probes.join(id);
        assert_eq!(recorded(&dir)["runs"], 1);
        let capture = fs::read_to_string(dir.join("capture.txt")).unwrap();
        assert_eq!(capture.lines().count(), 7, "{id}");
        assert!(capture.trim_end().ends_with("done"), "{id}");
    }
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
    let mut state = ScanState::default();

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
        kinds(&scan_once(&logs, &expects, &mut state)),
        [
            "ingested:p3-same-tick-rule-order",
            "ingested:p3-same-tick-rule-order-driver-last"
        ]
    );

    // A P3 run matching neither expectation has no unique owner.
    let wrong = recorded_capture("p3-same-tick-rule-order", "stale/capture.txt")
        .replace("t_ms=0", "t_ms=999");
    fs::write(logs.join("c.txt"), wrong).unwrap();
    let events = scan_once(&logs, &expects, &mut state);
    match &events[..] {
        [Event::Skipped { reason, .. }] => {
            assert!(reason.contains("no unique owner"), "{reason}")
        }
        other => panic!("expected one Skipped, got {}", kinds(other).join(", ")),
    }
    // The segment was reported but no (file, probe) slot was spent, so a
    // valid segment appended to the same file still ingests.
    let valid = recorded_capture("p3-same-tick-rule-order", "stale/capture.txt");
    fs::write(logs.join("c.txt"), valid).unwrap();
    assert_eq!(
        kinds(&scan_once(&logs, &expects, &mut state)),
        ["ingested:p3-same-tick-rule-order"]
    );
}

#[test]
fn a_log_holding_another_probes_end_line_is_foreign() {
    let (probes, _) = tree(&["p1-wait-rounding", "p3-same-tick-rule-order"]);
    let expects = load_expects(&probes).unwrap();
    let text = recorded_capture("p3-same-tick-rule-order", "stale/capture.txt")
        .replace("P3 done", "P3a done");

    assert_eq!(
        foreign_end(&text, &probes.join("p1-wait-rounding"), &expects).as_deref(),
        Some("p3-same-tick-rule-order")
    );
    // The probe's own end line is not foreign.
    assert!(foreign_end(&text, &probes.join("p3-same-tick-rule-order"), &expects).is_none());
}
