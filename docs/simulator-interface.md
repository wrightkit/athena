# Simulator interface

The contract between `athena-sim` and its consumers (`wright`, tests). It covers the entry point and the trace. Scheduler behavior is in [architecture.md](architecture.md) and [findings.md](findings.md).

## Entry point

```rust
pub fn run_text(text: &str, options: Options) -> Result<Trace, ParseError>
pub fn run_scenario(text: &str, scenario_json: &str) -> Result<Trace, RunError>
pub fn run_scenario_with_hero_values(
    text: &str,
    scenario_json: &str,
    hero_values: &workshop_rs::hero_values::HeroValues,
) -> Result<Trace, RunError>
```

- `text` is Workshop text in any locale that `workshop-rs` supports. The simulator parses it with the public `workshop-rs` parser. It takes no `Program`, so callers may pin another `workshop-rs` version.
- `Options.seconds` is the simulated time, counted from the first tick. `run_scenario` takes the simulated time from the scenario's `seconds`.
- `run_scenario` builds player world state from the `workshop-rs` hero-values file pinned at `hero_values::CURRENT_VERSION`. `run_scenario_with_hero_values` takes an explicit set instead; tests use it with a file loaded through `hero_values::load`.
- `Err(ParseError)` means the text is not Workshop. Nothing was simulated.
- `Err(RunError::Scenario)` means the scenario is malformed or fails a loader check. Nothing was simulated; see [scenario-format.md](scenario-format.md).
- `Err(RunError::HeroValues)` means the pinned hero-values file failed to load; neither the program nor the scenario is the cause. Nothing was simulated.
- `Ok(Trace)` means the text parsed (and the scenario, when given, is well-formed). Every parsed program gives a trace, including a program that uses behavior the simulator does not model.

The crate compiles for `wasm32-unknown-unknown`.

## Trace

| Field | Meaning |
| --- | --- |
| `outcome` | How the run ended. Check it before using the other fields. |
| `logs` | `Log To Inspector` lines in order, each with the `tick` it was written on. |
| `globals` | Global variables at the end of the run. A declared variable that was never set is `null`. |
| `players` | The player state table: one entry per scenario player, keyed by its `id`. Empty when the run had no scenario. See below. |
| `missingInputs` | The live inputs the program read but the scenario did not supply. Empty unless `outcome.status` is `missing_input`; at most one entry in version 1. Each entry: `input` (`hero`, `team`, `eventPlayer`, `eventValue`), `player` (the player `id` the input belongs to, `null` for `eventPlayer` and `eventValue`), `name` (the event-value name, `null` for the others), `tick`, `construct`. |
| `expectations` | One result per scenario expectation, in file order: `index`, `status` (`pass`, `fail`, `not_evaluated`), `actual` (the observed value, `null` when it does not exist). |

`logs`, `globals` and `players` describe the state when the run ended: after the last tick for `complete`, at the stop for `not_modeled` and `missing_input`.

### `players` rows

Each row repeats what the scenario declared — `hero` (the scenario text, `null` when undeclared) and `team` (`team1`/`team2`, `null` when undeclared) — plus `world`, the catalog-derived state at the end of the run:

| `world` field | Meaning |
| --- | --- |
| `health` | The `health` pool: `{ "current": ..., "max": ... }`. |
| `shield` | The `shield` pool, same shape; `null` when the catalog records none. |
| `armor` | The `armor` pool, same shape; `null` when the catalog records none. |
| `cooldowns` | Cooldown seconds per ability slot, keyed by `slot` or `slot/variant`; only slots the catalog records a value for appear. Read-only in version 1. |
| `ultimate` | `{ "charge": ..., "cost": ... }`: charge points toward the catalog's cost. Read-only in version 1. |

`world` is `null` when the run could not resolve the player's catalog state — a player without a `hero`, or a hero value the file does not have. `Damage` or `Heal` on such a player stops the run as `not_modeled`; the row still reports the declared `hero`/`team`.

### Outcome and "not modeled"

- `complete`: the whole requested time was simulated.
- `not_modeled`: the run met a construct the simulator has no passing capture fixture for, and stopped there. The outcome carries `construct` (a short description, for example `action wait` or `value getPosition`) and `tick` (the tick on which it was met, or `null` if it was found before the first tick, for example an unsupported rule event).
- `missing_input`: the run met a live input the scenario does not supply and stopped at that read. The input is named in `missingInputs`; expectations are all `not_evaluated`.

The simulator never skips an unmodeled construct and never replaces it with a default value. A consumer must treat a trace with `outcome` other than `complete` as partial. The text of `construct` is for people; do not match on it.

### JSON form

`Trace` implements `serde::Serialize`. The JSON shape is part of the contract:

```json
{
  "outcome": { "status": "not_modeled", "construct": "value getPosition", "tick": 0 },
  "logs": [{ "tick": 3, "text": "t0=3.023" }],
  "globals": { "a": { "type": "number", "value": 1.0 }, "b": { "type": "null" } },
  "players": {
    "p1": {
      "hero": "Ana",
      "team": "team1",
      "world": {
        "health": { "current": 150.0, "max": 200.0 },
        "shield": { "current": 0.0, "max": 100.0 },
        "armor": null,
        "cooldowns": { "ability1": 10.0, "ability2/secondary": 12.0 },
        "ultimate": { "charge": 0.0, "cost": 2100.0 }
      }
    }
  },
  "missingInputs": [],
  "expectations": []
}
```

`outcome` is `{"status": "complete"}`, `{"status": "not_modeled", "construct": ..., "tick": ...}` or `{"status": "missing_input"}`. A value is `{"type": ..., "value": ...}` with `type` one of `null`, `bool`, `number`, `text`, `array`, `player`; `null` has no `value`. `player` holds the scenario `id` that declared the player. `players` is a map keyed by the scenario player `id`; a row's `world` is `null` when the player's catalog state could not be resolved.

Adding a field is compatible. Removing or renaming one, or changing a value's meaning, is a breaking change and needs a version bump of this crate.

## Fixtures

`crates/athena-sim/tests/fixtures.rs` reads every probe through `run_text`, requires `outcome` to be `complete`, and compares `logs` and `globals` with the captures.
