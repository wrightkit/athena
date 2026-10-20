//! Every `probes/<id>/capture.json` records the SHA-256 of the pasted
//! `probe.zh-CN.ow`. A probe edit makes its captures stale
//! (docs/capture-convention.md), so a mismatch fails the build.

use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

#[test]
fn capture_probe_sha256_matches_probe_program() {
    let probes = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../probes");
    let mut entries: Vec<_> = fs::read_dir(&probes)
        .unwrap_or_else(|error| panic!("{}: {error}", probes.display()))
        .collect::<Result<_, _>>()
        .unwrap_or_else(|error| panic!("{}: {error}", probes.display()));
    entries.sort_by_key(|entry| entry.file_name());

    let mut checked = 0;
    let mut failures = Vec::new();
    for entry in entries {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        checked += 1;
        let id = entry.file_name().to_string_lossy().into_owned();
        let mut fail = |message: String| failures.push(format!("{id}: {message}"));

        let program = match fs::read(dir.join("probe.zh-CN.ow")) {
            Ok(bytes) => bytes,
            Err(error) => {
                fail(format!("probe.zh-CN.ow: {error}"));
                continue;
            }
        };
        let capture_text = match fs::read_to_string(dir.join("capture.json")) {
            Ok(text) => text,
            Err(error) => {
                fail(format!("capture.json: {error}"));
                continue;
            }
        };
        let recorded = serde_json::from_str::<serde_json::Value>(&capture_text)
            .ok()
            .and_then(|capture| capture["probe_sha256"].as_str().map(str::to_string));
        let Some(recorded) = recorded.filter(|hash| !hash.is_empty()) else {
            fail("capture.json has no probe_sha256".to_string());
            continue;
        };
        let actual = format!("{:x}", Sha256::digest(&program));
        if recorded != actual {
            fail(format!(
                "probe_sha256 {recorded} does not match probe.zh-CN.ow {actual}; the capture is stale"
            ));
        }
    }

    assert!(checked > 0, "no probes under {}", probes.display());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
