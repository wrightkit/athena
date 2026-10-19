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
- **Milliseconds.** Inspector lines are prefixed `[mm:ss]` and `Custom String` shows two decimals, so probes print `(Total Time Elapsed - t0) * 1000`.
- **Unsupported is explicit.** A probe that cannot express its hypothesis is reported, not silently weakened.
- **Inconclusive means unasserted.** The simulator test for an `inconclusive` probe is an explicit unasserted case with a note, never a confirmed behavior.

## Editing a probe

1. Edit `probe.workshop`; keep `probe.zh-CN.ow` equivalent.
2. `wright check --kind workshop` on both files; both must pass.
3. `wright inspect cost --kind workshop probes/<id>/probe.workshop` and the same with `--locale zh-CN` on the `.zh-CN.ow` must report equal element counts.
4. Update the expected values in [procedure.zh-CN.md](procedure.zh-CN.md) if the probe's behavior changed, and treat its existing captures as stale.
