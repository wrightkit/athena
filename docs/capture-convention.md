# Capture convention

## Layout

```text
probes/<id>/
  probe.workshop       readable source; the source of truth for edits
  probe.zh-CN.ow       the text pasted into the client; derived from the source
  capture.txt          run 1, verbatim Inspector text
  capture-2.txt        optional, run 2
  capture-3.txt        optional, run 3
  capture.json         client, hypothesis, observed, runs, result, notes, sources
  expect.json          optional, the expected log lines or verdict; lets `athena ingest` judge
  stale/               optional, captures invalidated by a probe change
```

Probe ids are `p<n>-<slug>`. Probes share global variable names, so run one at a time.

## capture.json

```json
{
  "probe": "p1-wait-rounding",
  "client": { "version": "", "date": "", "locale": "zh-CN" },
  "runs": 1,
  "probe_sha256": "SHA-256 of the pasted probe.zh-CN.ow",
  "hypothesis": "what the probe tests",
  "observed": [],
  "result": "match | mismatch | inconclusive",
  "notes": "",
  "sources": []
}
```

- `observed` holds only what the capture shows, in milliseconds. Its field names are per probe.
- `runs` counts saved captures valid for the current `probe_sha256`.
- `notes` states whether the result is provisional, what the probe cannot test, and invalid runs. The text is human-owned except for one trailing `[ingest: …]` marker `athena ingest` maintains; see [Ingesting a log](#ingesting-a-log).
- `sources` lists the Inspector log file names `athena` has already ingested for this probe. It stays empty for captures saved by hand and in files written before the field existed.
- A probe with no capture has `runs: 0`, `observed: []`, `result: inconclusive`.

## Rules

- **Verbatim.** Save the whole Inspector text as pasted, variable block included. Never reconstruct, reformat or fill in a missing line; if a paste looks truncated, get it again.
- **Three values only.** `match`, `mismatch`, `inconclusive`. No evidence classes or grades.
- **Final needs three.** A result with fewer than three agreeing runs is provisional and says so in `notes`. Runs that disagree give `inconclusive` with the differences listed.
- **A mismatch wins.** Record the client's behavior; do not edit the probe until it agrees.
- **A changed probe invalidates its captures.** If `probe_sha256` no longer matches `probe.zh-CN.ow`, the capture is stale: move the invalidated `capture*.txt` files to `stale/` under the probe directory, reset `runs` to 0 and note the previous state in `notes`. Tools only look at the probe root, so `stale/` is a parking lot, not an input.
- **Milliseconds.** Inspector lines are prefixed `[hh:mm:ss]` and `Custom String` shows two decimals, so probes print `(Total Time Elapsed - t0) * 1000`.
- **Unsupported is explicit.** A probe that cannot express its hypothesis is reported, not silently weakened.
- **Inconclusive means unasserted.** The simulator test for an `inconclusive` probe is an explicit unasserted case with a note, never a confirmed behavior.

## Editing a probe

1. Edit `probe.workshop`; keep `probe.zh-CN.ow` equivalent.
2. `wright check --kind workshop` on both files; both must pass.
3. `wright inspect cost --kind workshop probes/<id>/probe.workshop` and the same with `--locale zh-CN` on the `.zh-CN.ow` must report equal element counts.
4. Update the expected values in [procedure.zh-CN.md](procedure.zh-CN.md) if the probe's behavior changed, and treat its existing captures as stale.

## Ingesting a log

`athena ingest <probe-dir> <log-file>` saves a run from a Workshop Inspector log as the next capture and updates `runs`, `result`, `notes` and `sources` in `capture.json`. The live log has log lines only, no variable block, so a probe that `ingest` handles must print every value its verdict needs as a log line.

`expect.json` is the only home of such a probe's expected values:

```json
{ "prefix": "P1", "end": "P1 requested_ms=500 ", "lines": ["P1 requested_ms=1 elapsed_ms=16", "..."] }
```

- `prefix`: every line of the probe starts with it. `end`: the last line of a finished run starts with it. `lines`: the expected message of each line after `[hh:mm:ss] `, in order. Numbers match within 0.0051 (`Custom String` shows two decimals).
- A probe whose verdict is statistical carries a `verdict` object instead of `lines` (P5): `{ "kind": "distinct-samples", "startPrefix": "P5 start", "samplePrefix": "P5 i=", "expectedSamples": 188, "matchIfDistinctAtMost": 112 }`. A valid run is the start line — reporting `spawned=真` at a nonzero `pos` — plus exactly `expectedSamples` sample lines whose `pos` changes; `matchIfDistinctAtMost` distinct `pos` values or fewer is a `match`, more is a `mismatch`, and a run that fails validity is rejected, not judged. The threshold sits halfway between the refresh hypothesis and every-sample-distinct.
- `ingest` rejects the log, and writes nothing, if it has no line starting with `end`, has several, has probe-prefixed lines after the `end` line, if the run fails its `verdict` validity, or if `probe_sha256` is stale or `runs` differs from the saved files. Give it one run.
- The command also refuses a log that holds another probe's `end` line — it would record a run poisoned by that probe's lines. It says which probe the run belongs to; split the file first or use `athena watch`. The check reads `probes/*/expect.json` (`--probes <dir>` overrides) and is skipped when the probes dir is missing.
- `sources` dedups by file name, not content: ingesting a log whose file name is already recorded is skipped — nothing is written and `runs` does not move — while byte-identical logs under different names still count as distinct runs.
- It saves the probe's log lines verbatim as `capture.txt`, `capture-2.txt`, and so on. The run is `match` or `mismatch`.
- `result` follows the rules above: final only with three agreeing runs, otherwise `inconclusive`. `ingest` prints the verdict of each run. It does not edit `client`, `hypothesis`, or `observed`; record the client version and any differences there.
- `notes` is human text plus at most one trailing `[ingest: …]` marker owned by `ingest`: while the result is `inconclusive` the marker reads `[ingest: provisional, <n>/3 runs agree]`, with `<n>` the largest agreeing side; once the result is `match` or `mismatch` only the human text remains. `ingest` replaces the marker, it never stacks it; a probe with `runs: 0` keeps its `notes` untouched.
- An ingested capture holds log lines only — no variable block, so there are no captured variables to verify and the simulator fixture checks its log lines only. A full paste export still gets the variable check.
- P6's verdict reads the counters it prints in the `P6 counts` line; `endAbort` is not in that line and still needs the variable export to confirm it stayed zero.

## Watching a log directory

`athena watch <log-dir>` polls a directory once a second for Inspector log files. It learns the probes' end lines from `probes/*/expect.json` (`--probes <dir>` overrides) and ingests one run per probe per file once the file holds an end line; a file without one is left for the next poll in case it is still being written. A file may hold several runs: the watch splits it into segments at every probe's end line, and each segment belongs to the probe whose `end` closes it — so one file holding a P3a run then a P3b run feeds each probe only its own lines. Each probe's `end` should be unique — the P3 siblings log `P3a done` and `P3b done`; if probes ever share an end line, the watch falls back to telling them apart by which candidate's expected `lines` the segment's run matches, and zero or several matches is ambiguous and skipped. Segments cannot tell probes sharing a `prefix` apart line-by-line: an aborted P3a run (no end line) left in the same file as a complete P3b run contaminates the P3b segment and records a `mismatch` — run the intended probe only, and discard a run that was cut short. Only the first complete run of a probe in a file is recorded — a second run of the same probe appended later is not picked up, and across restarts the `sources` list dedups the same file name again. The watch prints one line per ingest or skip, and a file already in `sources` is reported once as a duplicate. Ctrl-C or stdin EOF stops it.
