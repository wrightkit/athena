# Scenario format

Status: **design proposal for [athena#8](https://github.com/wrightkit/athena/issues/8)**. The owner has not accepted it yet. Nothing here is implemented.

A scenario is a JSON file. It tells the simulator who is in the match, which events happen and when, and what the author expects to see. Agents and `wright ui` write it. `wright` passes it to `athena` unchanged ([ADR-0033](https://github.com/wrightkit/wright/blob/main/docs/adr/0033-simulate-operation.md), decisions 2 and 5). The machine-readable definition is [scenario.schema.json](scenario.schema.json). The entry point and trace are in [simulator-interface.md](simulator-interface.md).

## Decisions for the owner

Each item below is a choice made in this proposal. Any of them can change.

1. **Seconds are part of the scenario.** `seconds` is required, so a scenario describes a whole run. The alternative is a separate caller option.
2. **Players are a list with a text `id`.** Events and expectations point to a player by `id`. The alternative is to number players.
3. **`hero` is free text in the schema.** `athena` checks it against the hero names of `workshop-rs` when it loads the scenario. The schema stays small and does not need updating with each game patch.
4. **`team` is `team1` or `team2` only.** There is no "all" team, because a player is always on one team.
5. **Player `state` means player variables.** It is a map from the variable name in the program to its start value. Other game state (health, position) is not in version 1, because the simulator does not model it yet.
6. **Event `kind` is the Workshop event name** (for example `playerDealtFinalBlow`). The schema does not list the names.
7. **Times are seconds from the first tick.** Events at the same time run in file order.
8. **Observations are checked by `athena`.** The trace carries one result per expectation. The alternative is that each consumer compares the trace itself. Checking in one place keeps `wright` free of Workshop logic.
9. **Expectations are checked at the end of the run only,** and only for three kinds: a global variable, a player variable, a log line. A rule for "value at time T" can come later.
10. **A missing input stops the run,** exactly like an unmodeled construct. The trace lists the first one that was met. The alternative is to continue and collect all of them. Continuing would need a guess for the missing value, and the simulator never guesses.
11. **An unknown field is an error.** The schema forbids extra fields. Typos in an agent-written file are then caught, and version 1 stays closed.
12. **The schema file lives in `docs/`.** CI skips runs for `docs/**` changes, so a change to the schema alone does not start the schema check. A change to the test or to Rust code does.

## Top-level shape

| Field | Required | Meaning |
| --- | --- | --- |
| `scenarioVersion` | yes | The format version. In this proposal it is the integer `1`. |
| `seconds` | yes | Simulated time, greater than zero, counted from the first tick. |
| `players` | yes | List of players. It may be empty. |
| `events` | no | Events injected into the run. |
| `expect` | no | Observations the author expects at the end of the run. |

### Player

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Text that names the player in this file. It must be unique. |
| `hero` | no | Hero name, for example `ana`. |
| `team` | no | `team1` or `team2`. |
| `state` | no | Map from player-variable name to start value. |

### Event

| Field | Required | Meaning |
| --- | --- | --- |
| `kind` | yes | Workshop event name. |
| `time` | yes | Seconds from the first tick, zero or more. |
| `eventPlayer` | no | The `id` of the event player. |
| `values` | no | Map from event-value name to value, for events that carry values. |

### Values

A value is `null`, `true` or `false`, a number, a text, or an array of values. These are the same kinds as the trace uses.

### Expectation

An expectation has a `kind`:

- `global`: fields `name` and `value`. The global variable has this value at the end of the run.
- `playerVariable`: fields `player` (an `id`), `name` and `value`.
- `log`: fields `tick` and `text`. The trace has a log line with this text on this tick.

## Missing inputs

An **input** is a fact about the match that the program reads and the program does not create. Inputs are: the hero of a player, the team of a player, a player variable that nothing has written yet, the event player, and an event value.

An input is **missing** when the program reads it during the run and the scenario does not supply it. Global variables are not inputs; the program owns them. The simulator never fills in a missing input. It stops the run at that read.

A scenario that breaks the schema is a different case. It is refused and nothing runs ([ADR-0033](https://github.com/wrightkit/wright/blob/main/docs/adr/0033-simulate-operation.md), decision 4). A well-formed scenario that lacks an input gives a trace.

### Trace additions

The trace gains two fields. Adding a field is compatible ([simulator-interface.md](simulator-interface.md)).

- `outcome.status` can be `missing_input`. The run stopped at the first missing input.
- `missingInputs`: a list that is empty unless the status is `missing_input`. Each entry has these fields:

| Field | Meaning |
| --- | --- |
| `input` | One of `hero`, `team`, `playerVariable`, `eventPlayer`, `eventValue`. |
| `player` | The `id` of the player the input belongs to, or `null` when no player applies (an event value). |
| `name` | The variable name or event-value name, or `null` for `hero`, `team` and `eventPlayer`. |
| `tick` | The tick on which the program read it. |
| `construct` | A short description for people, as in `not_modeled`. Do not match on it. |

Example: the program reads `eventPlayer.streak` and the scenario gives no `streak` for `p1`:

```json
{ "input": "playerVariable", "player": "p1", "name": "streak", "tick": 62, "construct": "value eventPlayer.streak" }
```

When `outcome.status` is `missing_input`, `logs` and `globals` are the state at the stop, as for `not_modeled`.

### Expectation results

The trace also gains `expectations`: one entry per expectation, in file order, each `{ "index": n, "status": ..., "actual": ... }`. `status` is `pass` or `fail`, and `actual` is the observed value (`null` if it does not exist). When the outcome is not `complete`, every status is `not_evaluated`. A partial run never reports `pass` or `fail`.

## Version

The file declares its version in `scenarioVersion`. The simulator supports a fixed set of versions. If the value is not in that set, or is not an integer, the simulator refuses the scenario before it reads any other field, and its error names the version found and the versions supported. It does not try to read a newer file. The schema file is named for the current version only: `scenarioVersion` is the constant `1`.

Adding an optional field does not change the version. Removing or renaming a field, or changing what a value means, makes a new version.

## Worked example: `practice-range`

[scenario-examples/practice-range.json](scenario-examples/practice-range.json) drives `practice-range` (`main.opy` in that project). Player `p1` plays Ana on team 1 and starts with `streak` 2. At second 1, `p1` gets a final blow. The `kill` rule makes `streak` 3, so `delta` is 2 and `score` goes from 0 to 2.

```json
{
  "scenarioVersion": 1,
  "seconds": 2,
  "players": [
    { "id": "p1", "hero": "ana", "team": "team1", "state": { "streak": 2 } }
  ],
  "events": [
    { "kind": "playerDealtFinalBlow", "time": 1, "eventPlayer": "p1" }
  ],
  "expect": [
    { "kind": "global", "name": "score", "value": 2 },
    { "kind": "playerVariable", "player": "p1", "name": "streak", "value": 3 }
  ]
}
```

If the `state` line is removed, the `kill` rule reads `eventPlayer.streak` with no value. The trace then has `outcome.status` `missing_input` and one `missingInputs` entry for `streak` of `p1`. The simulator does not assume 0.

## Invalid examples

[scenario-examples/invalid/](scenario-examples/invalid/) holds files that the schema must reject:

- `unknown-version.json`: `scenarioVersion` is 2.
- `missing-required-field.json`: `seconds` is absent.
- `wrong-value-type.json`: an event `time` is text.
- `unknown-team.json`: `team` is not `team1` or `team2`.

`crates/athena-sim/tests/scenario_schema.rs` checks that the valid example passes and every invalid example fails. `cargo test` and CI run it.
