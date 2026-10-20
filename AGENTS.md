# AGENTS.md

This repository is part of the **WrightKit** multi-repository workspace. Apply the
workspace-level `AGENTS.md` when available, then follow this file. `README.md` and
`docs/` are English except `docs/procedure.zh-CN.md`, the Chinese probe procedure.

`athena` holds **runtime-behavior probes** for Overwatch Workshop rules and the
captures recorded from the real client, so that a later scheduler simulator can
assert against measured behavior instead of community notes.

## Docs routing

Start at [`docs/README.md`](docs/README.md). Load only the document the task needs:

- goal and scope: `docs/goal.md`
- ownership, dependency direction, simulator scope: `docs/architecture.md`
- why this is a standalone project, and what is still open: `docs/placement.md`
- capture layout, `capture.json` shape, result rules, editing a probe: `docs/capture-convention.md`
- running a probe, expected values, verdict conditions: `docs/procedure.zh-CN.md`

Per-probe status lives in `probes/<id>/capture.json`; do not copy it into prose.
When a change affects a durable contract or procedure, update the owning document
in the same change.

## Ownership

This repository owns the probe programs, the captures, and the capture convention and
verdict rules in `docs/`. It does **not** own Workshop semantics, the catalog, parsing
or emission (`workshop-rs`), or Wright orchestration (`wright`). The scheduler
simulator is the crate `crates/athena-sim`, depending on `workshop-rs` (`docs/placement.md`).
It is built in layers; behavior without a passing capture fixture must return `Unsupported`.
Do not add confirmed behavior to `workshop-rs` from here, and do not promote a result
with fewer than three agreeing runs.

## Invariants

The capture rules (verbatim captures, three result values, a mismatch wins, stale
`probe_sha256`, milliseconds) are in `docs/capture-convention.md`. In addition:

- Expected values are hypotheses until a capture agrees; they live only in
  `docs/procedure.zh-CN.md`. Do not copy them elsewhere.
- Run one probe at a time. Probes share global variable names.
- The tick is 16 ms; P0 and P1 check it.
- Do not claim a probe was run unless a capture exists.

## Running a probe

Follow the workspace skill `.agents/skills/wrightkit-run-workshop-probe/SKILL.md`.
Driving the game client is a human step: the agent prepares the file and judges the
result; the user pastes into the client and copies the Inspector back.

## Verification and delivery

- A probe change is verified by `wright check --kind workshop` on both files plus the
  element-count comparison in `docs/capture-convention.md`. A result is verified by the
  capture itself; there is no other oracle.
- `cargo test` also verifies every `capture.json`'s `probe_sha256` against
  `probe.zh-CN.ow` (`crates/athena-sim/tests/probe_hashes.rs`); CI runs it on PRs and `main`.
- Commit only verified, task-owned files. Keep `.wright/` and `.DS_Store` out.
- Do not push without being asked.
