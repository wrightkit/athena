# athena

Runtime-behavior probes for Overwatch Workshop rules, and the captures recorded from the real client.

A probe is a small hand-written Workshop program. A human pastes it into the client, the whole Inspector text is saved verbatim, and a JSON record states the hypothesis, the observed values and a result of `match`, `mismatch` or `inconclusive`. The captures are meant to become test fixtures for a later scheduler simulator.

## Status

Probes P0 to P6 exist; measured results and run counts are in `probes/<id>/capture.json`. The scheduler simulator (`crates/athena-sim`) is being built in layers; its fixtures are the probe captures (`cargo test`). It is a standalone project depending on `workshop-rs`: [docs/placement.md](docs/placement.md).

## Run a probe

Use the workspace skill `wrightkit-run-workshop-probe` (`.agents/skills/wrightkit-run-workshop-probe/SKILL.md`). The agent prepares the probe and judges the result; you paste into the client and copy the Inspector text back. The steps and expected values (Chinese) are in [docs/procedure.zh-CN.md](docs/procedure.zh-CN.md).

To record a run from the Workshop Inspector log, run `cargo run -p athena-capture -- ingest probes/<id> <log-file>` ([capture convention](docs/capture-convention.md#ingesting-a-log)).

## Documentation

See [docs/README.md](docs/README.md): [goal](docs/goal.md), [architecture](docs/architecture.md), [placement](docs/placement.md), [capture convention](docs/capture-convention.md), [findings](docs/findings.md).
