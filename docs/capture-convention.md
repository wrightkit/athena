# Capture convention

## Layout

```text
probes/<id>/
  probe.workshop       readable source; the source of truth for edits
  probe.zh-CN.ow       the text pasted into the client; derived from the source
  capture.txt          run 1, verbatim Inspector text
  capture-2.txt        optional, run 2
  capture-3.txt        optional, run 3
  capture.json         client, hypothesis, observed, runs, result, notes
  expect.json          optional, the expected log lines; lets `athena ingest` judge
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
  "notes": ""
}
```

- `observed` holds only what the capture shows, in milliseconds. Its field names are per probe.
- `runs` counts saved captures valid for the current `probe_sha256`.
- `notes` states whether the result is provisional, what the probe cannot test, and invalid runs.
- A probe with no capture has `runs: 0`, `observed: []`, `result: inconclusive`.

## Rules

- **Verbatim.** Save the whole Inspector text as pasted, variable block included. Never reconstruct, reformat or fill in a missing line; if a paste looks truncated, get it again.
- **Three values only.** `match`, `mismatch`, `inconclusive`. No evidence classes or grades.
- **Final needs three.** A result with fewer than three agreeing runs is provisional and says so in `notes`. Runs that disagree give `inconclusive` with the differences listed.
- **A mismatch wins.** Record the client's behavior; do not edit the probe until it agrees.
- **A changed probe invalidates its captures.** If `probe_sha256` no longer matches `probe.zh-CN.ow`, the capture is stale: re-run it or mark it stale in `notes`.
- **Milliseconds.** Inspector lines are prefixed `[hh:mm:ss]` and `Custom String` shows two decimals, so probes print `(Total Time Elapsed - t0) * 1000`.
- **Unsupported is explicit.** A probe that cannot express its hypothesis is reported, not silently weakened.
- **Inconclusive means unasserted.** The simulator test for an `inconclusive` probe is an explicit unasserted case with a note, never a confirmed behavior.

## Editing a probe

1. Edit `probe.workshop`; keep `probe.zh-CN.ow` equivalent.
2. `wright check --kind workshop` on both files; both must pass.
3. `wright inspect cost --kind workshop probes/<id>/probe.workshop` and the same with `--locale zh-CN` on the `.zh-CN.ow` must report equal element counts.
4. Update the expected values in [procedure.zh-CN.md](procedure.zh-CN.md) if the probe's behavior changed, and treat its existing captures as stale.

## Ingesting a log

`athena ingest <probe-dir> <log-file>` saves a run from a Workshop Inspector log as the next capture and updates `runs` and `result` in `capture.json`. The live log has log lines only, no variable block, so a probe that `ingest` handles must print every value its verdict needs as a log line.

`expect.json` is the only home of such a probe's expected values:

```json
{ "prefix": "P1", "end": "P1 requested_ms=500 ", "lines": ["P1 requested_ms=1 elapsed_ms=16", "..."] }
```

- `prefix`: every line of the probe starts with it. `end`: the last line of a finished run starts with it. `lines`: the expected message of each line after `[hh:mm:ss] `, in order. Numbers match within 0.0051 (`Custom String` shows two decimals).
- `ingest` rejects the log, and writes nothing, if it has no line starting with `end`, has several, or if `probe_sha256` is stale or `runs` differs from the saved files. Give it one run; a re-used log file counts as a new run.
- It saves the probe's log lines verbatim as `capture.txt`, `capture-2.txt`, and so on. The run is `match` or `mismatch` against `lines`.
- `result` follows the rules above: final only with three agreeing runs, otherwise `inconclusive`. `ingest` prints the verdict of each run. It does not edit `client`, `hypothesis`, `observed`, or `notes`; record the client version and any differences there.
- A probe whose verdict is statistical (P5) or needs the variable block (P6) has no `expect.json` yet and is judged by hand.
