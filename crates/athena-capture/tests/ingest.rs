mod common;

use std::fs;
use std::path::PathBuf;

use athena_capture::{Error, Expect, Outcome, Verdict, ingest, judge_captures, result_of};
use common::{fresh_probe, probes, recorded, recorded_capture, saved};

fn probes_with_expectations() -> Vec<PathBuf> {
    let mut dirs: Vec<_> = fs::read_dir(probes())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|dir| dir.join("expect.json").exists())
        .collect();
    dirs.sort();
    dirs
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

/// The AC of the issue: a capture keeps only log lines. Re-judging the
/// saved captures with the variable block stripped must reproduce the
/// recorded result — P0 to P4 line by line, P5 by its verdict rule.
#[test]
fn the_log_lines_of_each_saved_capture_reproduce_the_recorded_result() {
    for id in [
        "p0-timer-resolution",
        "p1-wait-rounding",
        "p2-edge-trigger-and-wait-block",
        "p3-same-tick-rule-order",
        "p3-same-tick-rule-order-driver-last",
        "p5-value-refresh-rate",
    ] {
        let dir = probes().join(id);
        let expect: Expect =
            serde_json::from_str(&fs::read_to_string(dir.join("expect.json")).unwrap()).unwrap();
        let mut verdicts = Vec::new();
        for file in ["capture.txt", "capture-2.txt", "capture-3.txt"] {
            let path = dir.join(file);
            if !path.exists() {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            let log_only: String = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("全局."))
                .map(|line| format!("{line}\n"))
                .collect();
            let run = expect.run(&log_only).unwrap();
            verdicts.push(expect.judge(&run).unwrap());
        }
        let record = recorded(&dir);
        assert_eq!(
            result_of(&verdicts),
            record["result"].as_str().unwrap(),
            "{id}"
        );
    }
}

#[test]
fn three_agreeing_runs_make_the_result_final() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let files = ["capture.txt", "capture-2.txt", "capture-3.txt"];
    for (index, file) in files.iter().enumerate() {
        let report = saved(&dir, &recorded_capture(id, file), file);
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
fn the_same_source_is_skipped_not_counted_twice() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let log = recorded_capture(id, "capture.txt");
    saved(&dir, &log, "run-1.txt");
    assert!(matches!(
        ingest(&dir, &log, "run-1.txt").unwrap(),
        Outcome::Skipped
    ));
    assert_eq!(recorded(&dir)["runs"], 1);
    assert!(!dir.join("capture-2.txt").exists());
    // Byte-identical text under another file name is a distinct run.
    saved(&dir, &log, "run-2.txt");
    assert_eq!(recorded(&dir)["runs"], 2);
    assert!(dir.join("capture-2.txt").exists());
    assert_eq!(
        recorded(&dir)["sources"],
        serde_json::json!(["run-1.txt", "run-2.txt"])
    );
}

#[test]
fn a_capture_json_without_sources_starts_empty() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    // A capture.json written before `sources` existed has no such line;
    // remove it (and the comma it leaves behind) to reproduce one.
    let text = fs::read_to_string(dir.join("capture.json")).unwrap();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| !line.starts_with("  \"sources\": "))
        .map(str::to_string)
        .collect();
    let close = lines.iter().rposition(|line| line.trim() == "}").unwrap();
    lines[close - 1] = lines[close - 1].trim_end_matches(',').to_string();
    fs::write(dir.join("capture.json"), lines.join("\n") + "\n").unwrap();

    saved(&dir, &recorded_capture(id, "capture.txt"), "run-1.txt");
    let record = recorded(&dir);
    assert_eq!(record["sources"], serde_json::json!(["run-1.txt"]));
    assert!(matches!(
        ingest(&dir, &recorded_capture(id, "capture.txt"), "run-1.txt").unwrap(),
        Outcome::Skipped
    ));
}

#[test]
fn a_provisional_result_is_marked_in_notes_until_three_runs_agree() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let human = recorded(&dir)["notes"].as_str().unwrap().to_string();
    let log = recorded_capture(id, "capture.txt");

    saved(&dir, &log, "run-1.txt");
    assert_eq!(
        recorded(&dir)["notes"].as_str().unwrap(),
        format!("{human} [ingest: provisional, 1/3 runs agree]")
    );
    saved(&dir, &log, "run-2.txt");
    assert_eq!(
        recorded(&dir)["notes"].as_str().unwrap(),
        format!("{human} [ingest: provisional, 2/3 runs agree]")
    );
    saved(&dir, &log, "run-3.txt");
    assert_eq!(recorded(&dir)["result"], "match");
    // Final: the marker goes, only the human text stays.
    assert_eq!(recorded(&dir)["notes"].as_str().unwrap(), human);
}

#[test]
fn a_final_mismatch_clears_the_marker_too() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let human = recorded(&dir)["notes"].as_str().unwrap().to_string();
    let wrong = recorded_capture(id, "capture.txt").replace("elapsed_ms=112", "elapsed_ms=96");
    for n in 1..=3 {
        saved(&dir, &wrong, &format!("run-{n}.txt"));
    }
    assert_eq!(recorded(&dir)["result"], "mismatch");
    assert_eq!(recorded(&dir)["notes"].as_str().unwrap(), human);
}

#[test]
fn a_saved_capture_keeps_the_log_lines_verbatim() {
    let id = "p0-timer-resolution";
    let dir = fresh_probe(id);
    saved(&dir, &recorded_capture(id, "capture.txt"), "capture.txt");
    let saved_text = fs::read_to_string(dir.join("capture.txt")).unwrap();
    let logged: Vec<&str> = saved_text.lines().collect();
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
    for n in 1..=3 {
        saved(&dir, &wrong, &format!("run-{n}.txt"));
    }
    assert_eq!(recorded(&dir)["result"], "mismatch");
}

#[test]
fn runs_that_disagree_stay_inconclusive() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let right = recorded_capture(id, "capture.txt");
    let wrong = right.replace("elapsed_ms=112", "elapsed_ms=96");
    for (n, log) in [&right, &wrong, &right].iter().enumerate() {
        saved(&dir, log, &format!("run-{}.txt", n + 1));
    }
    assert_eq!(recorded(&dir)["result"], "inconclusive");
    assert_eq!(recorded(&dir)["runs"], 3);
    // Three runs that disagree are still provisional; the marker counts
    // the largest agreeing side.
    let record = recorded(&dir);
    let notes = record["notes"].as_str().unwrap();
    assert_eq!(notes.matches("[ingest: ").count(), 1, "{notes}");
    assert!(
        notes.ends_with("[ingest: provisional, 2/3 runs agree]"),
        "{notes}"
    );
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
        ingest(&dir, &truncated, "a.txt"),
        Err(Error::Incomplete(_))
    ));
    assert!(matches!(
        ingest(&dir, &format!("{full}{full}"), "b.txt"),
        Err(Error::SeveralRuns(2))
    ));
    assert!(matches!(
        ingest(&dir, "", "c.txt"),
        Err(Error::Incomplete(_))
    ));

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
        ingest(&dir, &recorded_capture(id, "capture.txt"), "x.txt"),
        Err(Error::Stale { .. })
    ));
}

#[test]
fn ingest_edits_only_its_managed_keys_in_capture_json() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let before = fs::read_to_string(dir.join("capture.json")).unwrap();
    saved(&dir, &recorded_capture(id, "capture.txt"), "run-1.txt");
    let after = fs::read_to_string(dir.join("capture.json")).unwrap();
    let kept = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| {
                !l.starts_with("  \"runs\": ")
                    && !l.starts_with("  \"result\": ")
                    && !l.starts_with("  \"notes\": ")
                    && !l.starts_with("  \"sources\": ")
            })
            .map(str::to_string)
            .collect()
    };
    assert_eq!(kept(&before), kept(&after));
    assert!(after.contains("  \"runs\": 1,"));
}

#[test]
fn an_end_line_followed_by_more_probe_lines_is_end_not_last() {
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    let full = recorded_capture(id, "capture.txt");
    // One end line, then a stray probe line after it: not several runs.
    let extra = format!("{full}[00:00:09] P1 stray");
    assert!(matches!(
        ingest(&dir, &extra, "x.txt"),
        Err(Error::EndNotLast { extra: 1 })
    ));
    assert!(!dir.join("capture.txt").exists());
}

#[test]
fn a_stray_next_slot_file_is_overwritten_not_counted_as_a_run() {
    // A crash between the capture write and the capture.json write leaves
    // a stray file the record does not count; the next ingest overwrites
    // it instead of failing `RunCount`.
    let id = "p1-wait-rounding";
    let dir = fresh_probe(id);
    fs::write(dir.join("capture.txt"), "stray\n").unwrap();
    saved(&dir, &recorded_capture(id, "capture.txt"), "run-1.txt");
    assert_eq!(recorded(&dir)["runs"], 1);
    assert!(
        fs::read_to_string(dir.join("capture.txt"))
            .unwrap()
            .starts_with("[00:00:03] P1 requested_ms=1 ")
    );
}

/// A P5 log: the `start` line plus `samples` `P5 i=` lines cycling through
/// `distinct` positions (`distinct` of 1 is a stuck position).
fn p5_log(start: &str, samples: usize, distinct: usize) -> String {
    let mut log = format!("[00:00:33] {start}\n");
    for i in 0..samples {
        let pos = i % distinct.max(1);
        log += &format!(
            "[00:00:33] P5 i={i} pos=({pos}.00, 0.00, 0.00) everyframe=(0.00, 0.00, 0.00)\n"
        );
    }
    log
}

const P5_START: &str = "P5 start spawned=真 pos=(1.00, 2.00, 3.00)";

#[test]
fn p5_verdict_counts_the_distinct_positions() {
    let id = "p5-value-refresh-rate";
    // About 38 distinct positions, the refresh-rate hypothesis: match.
    let dir = fresh_probe(id);
    let report = saved(&dir, &p5_log(P5_START, 188, 38), "run.txt");
    assert_eq!(report.verdicts, [Verdict::Match]);
    // Every sample distinct, the recorded behavior: mismatch.
    let dir = fresh_probe(id);
    let report = saved(&dir, &p5_log(P5_START, 188, 188), "run.txt");
    assert_eq!(report.verdicts, [Verdict::Mismatch]);
}

#[test]
fn invalid_p5_runs_are_rejected_and_write_nothing() {
    let id = "p5-value-refresh-rate";
    let dir = fresh_probe(id);
    let before = fs::read_to_string(dir.join("capture.json")).unwrap();
    // A dropped middle sample: the end line is still there, but the run
    // no longer holds exactly 188 samples.
    let dropped = p5_log(P5_START, 188, 38)
        .lines()
        .filter(|line| !line.contains("P5 i=5 "))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    for (n, log) in [
        p5_log("P5 start spawned=假 pos=(1.00, 2.00, 3.00)", 188, 38),
        p5_log("P5 start spawned=真 pos=(0.00, 0.00, 0.00)", 188, 38),
        dropped,
        p5_log(P5_START, 188, 1),
    ]
    .iter()
    .enumerate()
    {
        assert!(
            matches!(
                ingest(&dir, log, &format!("run-{n}.txt")),
                Err(Error::Invalid(_))
            ),
            "case {n}"
        );
    }
    // A run that never reaches the end line is a rejection too.
    assert!(matches!(
        ingest(&dir, &p5_log(P5_START, 187, 38), "run-4.txt"),
        Err(Error::Incomplete(_))
    ));
    assert_eq!(
        fs::read_to_string(dir.join("capture.json")).unwrap(),
        before
    );
    assert!(!dir.join("capture.txt").exists());
}
