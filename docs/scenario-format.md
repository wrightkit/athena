# Scenario format

Status: **accepted design, implemented in `athena-sim`** ([athena#8](https://github.com/wrightkit/athena/issues/8), merged as #16). The simulator reads a scenario file as described here; where the run reaches a construct the simulator does not model, the trace says so (`outcome.status` `not_modeled`).

A scenario is a JSON file. It tells the simulator who is in the match, which events happen and when, and what the author expects to see. Agents and `wright ui` write it. `wright` passes it to `athena` unchanged ([ADR-0033](https://github.com/wrightkit/wright/blob/main/docs/adr/0033-simulate-operation.md), decisions 2 and 5). The machine-readable definition is [scenario.schema.json](scenario.schema.json). The entry point and trace are in [simulator-interface.md](simulator-interface.md).

## Decisions for the owner

Each item below is a choice made in this proposal. Any of them can change.

1. **Seconds are part of the scenario.** `seconds` is required, so a scenario describes a whole run. The alternative is a separate caller option.
2. **Players are a list with a text `id`.** Events and expectations point to a player by `id`. The alternative is to number players.
3. **`hero` is free text in the schema.** `athena` checks it against the hero names of `workshop-rs` when it loads the scenario. The schema stays small and does not need updating with each game patch.
4. **`team` is `team1` or `team2` only.** There is no "all" team, because a player is always on one team.
5. **Player `state` means player variables.** It is a map from the variable name in the program to its start value. A variable not listed starts as `null` (decision 13). Other game state (health, position) is not in version 1, because the simulator does not model it yet.
6. **Event `kind` is the Workshop event name** (for example `playerDealtFinalBlow`). The schema does not list the names.
7. **Times are seconds from the first tick.** Events at the same time run in file order.
8. **Observations are checked by `athena`.** The trace carries one result per expectation. The alternative is that each consumer compares the trace itself. Checking in one place keeps `wright` free of Workshop logic.
9. **Expectations are checked at the end of the run only,** and only for three kinds: a global variable, a player variable, a log line. A rule for "value at time T" can come later.
10. **A missing live input stops the run.** It stops at the first one met, as for an unmodeled construct, and the trace lists it. The alternative is to continue and collect all of them. Continuing would need a guess for the missing value, and the simulator never guesses.
11. **An unknown field is an error.** The schema forbids extra fields. Typos in an agent-written file are then caught, and version 1 stays closed.
12. **The schema file lives in `docs/`.** CI skips runs for `docs/**` changes, so a change to the schema alone does not start the schema check. A change to the test or to Rust code does.
13. **A scenario is a mock: what it does not declare takes the default value, which is `null`.** This is the owner's rule. The repository does not establish it as game behavior. No probe capture and no `workshop-rs` document records what an undeclared variable holds in the real client. The value `null` is what `athena` already uses for a global variable that was never set (`simulator-interface.md`, and `Null` reads as `0` in a number context, `architecture.md`). It is therefore a **modeled** choice, not a **measured** one. A probe is needed before it counts as measured.

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
| `id` | yes | Text that names the player in this file. It must be unique; the loader checks this (see Loader checks). |
| `hero` | no | Hero name, for example `ana`. |
| `team` | no | `team1` or `team2`. |
| `state` | no | Map from player-variable name to start value. A variable not listed starts as `null`. |

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

## Loader checks

JSON Schema cannot express some rules. The `athena` loader must check them and refuse the scenario when one fails. The schema and its test do not cover them.

- Player `id` values are unique.
- Every `eventPlayer` and every expectation `player` is the `id` of a player in the file.
- Every `hero` is a hero name known to `workshop-rs` (decision 3): the canonical `Hero` member id of the catalog (for example `JUNKER_QUEEN`) or its en-US spelling (`Junker Queen`), matched case-insensitively.
- `scenarioVersion` is checked first (see Version).

## Defaults and missing inputs

A scenario is a mock. Every fact it does not declare is handled by one of two rules. Which rule applies depends on the kind of fact.

**Group 1: the game gives a default. The scenario may omit it, and the run goes on with `null`.**

- A global variable. The program owns it.
- A player variable that the scenario does not list in a player's `state`.

The value is `null`, as in decision 13. No report is made. The trace shows the effect: `globals` holds `null` for a global that was never set.

**Group 2: the game supplies it from live play, and it has no default. This is a missing input.**

- The `hero` of a player, when the program reads it and the player has no `hero`.
- The `team` of a player, when the program reads it and the player has no `team`.
- The event player, when the program reads it and the event has no `eventPlayer`.
- An event value, when the program reads it and the event has no such entry in `values`.

An input of group 2 is **missing** when the program reads it during the run and the scenario does not supply it. The simulator never fills it in. It stops the run at that read, and the trace names the input.

Modeled reads in version 1: the event player (`Event Player`), a player's hero (`Hero Of`) and team (`Team Of`), a player variable, and the scalar event values `eventDamage`, `eventHealing`, `eventWasCriticalHit`, `eventWasEnvironment` and `eventWasHealthPack` — each looked up in `values` by its catalog name. Other event-scoped values (`attacker`, `victim`, `hostPlayer`, `eventAbility`, `eventDirection`) are not modeled and stop the run as `not_modeled`. An injected event fires every rule whose event kind matches `kind`; team and player filters other than `All` are not modeled, and a rule that declares them stops the load as `not_modeled`.

A malformed scenario is a different case. It is refused and nothing runs ([ADR-0033](https://github.com/wrightkit/wright/blob/main/docs/adr/0033-simulate-operation.md), decision 4). The schema and the loader checks (below) define "malformed". A well-formed scenario that lacks a group 2 input gives a trace.

### Trace additions

The trace gains two fields. Adding a field is compatible ([simulator-interface.md](simulator-interface.md)).

- `outcome.status` can be `missing_input`. The run stopped at the first missing input.
- `missingInputs`: a list that is empty unless the status is `missing_input`. The run stops at the first missing input, so the list has at most one entry in version 1. Each entry has these fields:

| Field | Meaning |
| --- | --- |
| `input` | One of `hero`, `team`, `eventPlayer`, `eventValue`. |
| `player` | The `id` of the player the input belongs to, or `null` when no player applies (`eventPlayer`, `eventValue`). |
| `name` | The event-value name for `eventValue`; `null` for the others. |
| `tick` | The tick on which the program read it. |
| `construct` | A short description for people, as in `not_modeled`. Do not match on it. |

Example: the `kill` rule reads the event player and the event has no `eventPlayer`:

```json
{ "input": "eventPlayer", "player": null, "name": null, "tick": 62, "construct": "value eventPlayer" }
```

When `outcome.status` is `missing_input`, `logs` and `globals` are the state at the stop, as for `not_modeled`.

### Expectation results

The trace also gains `expectations`: one entry per expectation, in file order, each `{ "index": n, "status": ..., "actual": ... }`. `status` is `pass` or `fail`, and `actual` is the observed value (`null` if it does not exist). When the outcome is not `complete`, every status is `not_evaluated`. A partial run never reports `pass` or `fail`.

## Version

The file declares its version in `scenarioVersion`. The simulator supports a fixed set of versions. If the value is not in that set, or is not an integer, the simulator refuses the scenario before it reads any other field, and its error names the version found and the versions supported. It does not try to read a newer file. The schema file is named for the current version only: `scenarioVersion` is the constant `1`.

Adding an optional field does not change the version. Removing or renaming a field, or changing what a value means, makes a new version.

## Worked example: `practice-range`

[scenario-examples/valid/practice-range.json](scenario-examples/valid/practice-range.json) drives `practice-range` (`main.opy` in that project). Player `p1` plays Ana on team 1 and starts with `streak` 2. At second 1, `p1` gets a final blow. The `kill` rule makes `streak` 3, so `delta` is 2 and `score` goes from 0 to 2.

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

Two more valid files in [scenario-examples/valid/](scenario-examples/valid/) show the two rules:

- `practice-range-default-variable.json` leaves out `state`. `streak` starts as `null`, which reads as 0, so the intended result is `streak` 1, `delta` 1 and `score` 1.
- `practice-range-missing-event-player.json` leaves out `eventPlayer`. The `kill` rule reads the event player, so the intended result is `outcome.status` `missing_input` and one `missingInputs` entry of kind `eventPlayer`.

The `init` rule's `Create HUD Text` is not modeled (athena#10), so the full `practice-range` program still stops at tick 0 with `outcome.status` `not_modeled` and every expectation reports `not_evaluated`. On the `kill`-rule path — event injection, `Event Player`, player variables, `If`/`Else`, `Call Subroutine` — the simulator models the constructs, and `practice-range.json` yields `streak` 3, `delta` 2 and `score` 2 with both expectations `pass`. `practice-range-default-variable.json` yields `streak` 1, `delta` 1 and `score` 1, and `practice-range-missing-event-player.json` yields `outcome.status` `missing_input` with one `eventPlayer` entry. These are exercised by `crates/athena-sim/tests/scenario.rs`.

## Invalid examples

[scenario-examples/invalid/](scenario-examples/invalid/) holds files that the schema must reject:

- `unknown-version.json`: `scenarioVersion` is 2.
- `missing-required-field.json`: `seconds` is absent.
- `wrong-value-type.json`: an event `time` is text.
- `unknown-team.json`: `team` is not `team1` or `team2`.

`crates/athena-sim/tests/scenario_schema.rs` checks that every file in `valid/` passes and every invalid example fails. `cargo test` and CI run it.
