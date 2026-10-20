# Simulator interface

The contract between `athena-sim` and its consumers (`wright`, tests). It covers the entry point and the trace. Scheduler behavior is in [architecture.md](architecture.md) and [findings.md](findings.md).

## Entry point

```rust
pub fn run_text(text: &str, options: Options) -> Result<Trace, ParseError>
```

- `text` is Workshop text in any locale that `workshop-rs` supports. The simulator parses it with the public `workshop-rs` parser. It takes no `Program`, so callers may pin another `workshop-rs` version.
- `Options.seconds` is the simulated time, counted from the first tick.
- `Err(ParseError)` means the text is not Workshop. Nothing was simulated.
- `Ok(Trace)` means the text parsed. Every parsed program gives a trace, including a program that uses behavior the simulator does not model.

The crate compiles for `wasm32-unknown-unknown`.

## Trace

| Field | Meaning |
| --- | --- |
| `outcome` | How the run ended. Check it before using the other fields. |
| `logs` | `Log To Inspector` lines in order, each with the `tick` it was written on. |
| `globals` | Global variables at the end of the run. A declared variable that was never set is `null`. |

`logs` and `globals` describe the state when the run ended: after the last tick for `complete`, at the stop for `not_modeled`.

### Outcome and "not modeled"

- `complete`: the whole requested time was simulated.
- `not_modeled`: the run met a construct the simulator has no passing capture fixture for, and stopped there. The outcome carries `construct` (a short description, for example `action wait` or `value getPosition`) and `tick` (the tick on which it was met, or `null` if it was found before the first tick, for example an unsupported rule event).

The simulator never skips an unmodeled construct and never replaces it with a default value. A consumer must treat a trace with `outcome` other than `complete` as partial. The text of `construct` is for people; do not match on it.

### JSON form

`Trace` implements `serde::Serialize`. The JSON shape is part of the contract:

```json
{
  "outcome": { "status": "not_modeled", "construct": "value getPosition", "tick": 0 },
  "logs": [{ "tick": 3, "text": "t0=3.023" }],
  "globals": { "a": { "type": "number", "value": 1.0 }, "b": { "type": "null" } }
}
```

`outcome` is `{"status": "complete"}` or `{"status": "not_modeled", "construct": ..., "tick": ...}`. A value is `{"type": ..., "value": ...}` with `type` one of `null`, `bool`, `number`, `text`, `array`; `null` has no `value`.

Adding a field is compatible. Removing or renaming one, or changing a value's meaning, is a breaking change and needs a version bump of this crate.

## Fixtures

`crates/athena-sim/tests/fixtures.rs` reads every probe through `run_text`, requires `outcome` to be `complete`, and compares `logs` and `globals` with the captures.
