# athena

Runtime-behavior probes for Overwatch Workshop rules, and the captures recorded from the real client.

A probe is a small hand-written Workshop program. A human pastes it into the client, the whole Inspector text is saved verbatim, and a JSON record states the hypothesis, the observed values and a result of `match`, `mismatch` or `inconclusive`. The captures are the test fixtures for the scheduler simulator.

## Status

Each probe's measured result and run count are in `probes/<id>/capture.json`.

The scheduler simulator (`crates/athena-sim`) is a released library that depends on `workshop-rs` and builds for WebAssembly. It runs Workshop text, optionally with a JSON scenario, and returns a timeline trace. Wright consumes it by git tag. Its fixtures are the probe captures (`cargo test`). Placement is in [docs/placement.md](docs/placement.md).

## Run a probe

Use the workspace skill `wrightkit-run-workshop-probe` (`.agents/skills/wrightkit-run-workshop-probe/SKILL.md`). The agent prepares the probe and judges the result; you paste into the client and copy the Inspector text back. The steps and expected values (Chinese) are in [docs/procedure.zh-CN.md](docs/procedure.zh-CN.md).

To record a run from the Workshop Inspector log, run `athena ingest probes/<id> <log-file>`, or point `athena watch <log-dir>` at a directory of Inspector logs to ingest each finished run as it lands. From a checkout, `cargo run -p athena-capture --` runs the `athena` binary ([capture convention](docs/capture-convention.md#ingesting-a-log)).

## Documentation

See [docs/README.md](docs/README.md): [goal](docs/goal.md), [architecture](docs/architecture.md), [simulator interface](docs/simulator-interface.md), [scenario format](docs/scenario-format.md), [trace format](docs/trace-format.md), [capture convention](docs/capture-convention.md), [findings](docs/findings.md), [releases](docs/releases.md).
