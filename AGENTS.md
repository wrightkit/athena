# AGENTS.md

This repository is part of the **WrightKit** multi-repository workspace. Apply the
workspace-level `AGENTS.md` when available, then follow this file. The working
language of `README.md` is Chinese; agent-facing documents such as this one are in
English.

`runtime` holds **runtime-behavior probes** for Overwatch Workshop rules and the
captures recorded from the real client. It exists so that a later scheduler
simulator can assert against measured behavior instead of community notes.

## Ownership

This repository owns:

- probe programs under `probes/<id>/` and the captures taken from them;
- the capture convention below and the verdict rules in `README.md`.

This repository does **not** own:

- Workshop semantics, the catalog, parsing, or emission (`workshop-rs`);
- the scheduler simulator (a separate workspace crate that depends on `workshop-rs`);
- Wright orchestration (`wright`).

A confirmed behavior may later become catalog metadata in `workshop-rs`. Do not add
it there from this repository, and do not promote a result that has fewer than three
agreeing runs.

## Layout

```text
README.md              Chinese: procedure, probe list, expected values, verdict table
probes/<id>/
  probe.workshop       readable source, the source of truth for edits
  probe.zh-CN.ow       the text pasted into the client; derived from the source
  capture.txt          run 1, verbatim Inspector text
  capture-2.txt        optional, run 2
  capture-3.txt        optional, run 3
  capture.json         client, hypothesis, observed, runs, result, notes
.agents/skills/run-workshop-probe/SKILL.md   the procedure for running one probe
```

## Invariants

- **Expected values are hypotheses until a capture agrees.** The expected values and
  the `match` / `mismatch` / `inconclusive` conditions live in `README.md`
  ("操作与判定"). Do not copy them into other files.
- **Captures are verbatim.** Save the whole Inspector text as pasted, variable block
  included. Never reconstruct, reformat, or fill in a missing line. If a paste looks
  truncated, ask for it again.
- **`result` uses three values only:** `match`, `mismatch`, `inconclusive`. Do not add
  evidence classes or grades. A result with fewer than three agreeing runs stays
  provisional: say so in `notes` and keep `runs` accurate.
- **A mismatch wins.** If the client disagrees with the hypothesis, record the client's
  behavior. Do not edit the probe until it agrees.
- **Run one probe at a time.** Probes share global variable names and would corrupt
  each other.
- **A changed probe invalidates its captures.** `capture.json` records `probe_sha256`,
  the SHA-256 of the exact `probe.zh-CN.ow` that was pasted. If the hash no longer
  matches, the capture is stale: re-run it or mark it stale in `notes`.
- **Report times in milliseconds.** The Inspector prefixes lines with `[mm:ss]`, which
  is useless for timing, and `Custom String` shows numbers with two decimals. Probes
  therefore print `(Total Time Elapsed - t0) * 1000`.
- **The tick is 16 ms.** All timing in probes assumes 0.016 s per tick; P0 and P1 check it.
- **Unsupported is explicit.** A probe that cannot express its hypothesis is reported,
  not silently weakened.

## Editing a probe

1. Edit `probe.workshop`. Keep `probe.zh-CN.ow` equivalent by hand or by regeneration.
2. Run `wright check --kind workshop` on both files. Both must pass.
3. Compare `wright inspect cost --kind workshop probes/<id>/probe.workshop` with
   `wright inspect cost --kind workshop --locale zh-CN probes/<id>/probe.zh-CN.ow`.
   The element counts must be equal.
4. Update the README expected values if behavior of the probe changed, and treat
   existing captures of that probe as stale.

## Running a probe

Follow `.agents/skills/run-workshop-probe/SKILL.md`. Driving the game client is a
human step for now: the agent prepares the file and judges the result, the user pastes
into the client and copies the Inspector back. Do not claim a probe was run unless a
capture exists.

## Verification and delivery

- Verification of a probe change is `wright check` plus the element-count comparison
  above. Verification of a result is the capture itself; there is no other oracle.
- Commit only verified, task-owned files. Keep `.wright/` and `.DS_Store` out.
- This repository has no remote yet. Do not add one or push without being asked.
