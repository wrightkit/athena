# Scenario format

Status: **accepted design, implemented in `athena-sim`** ([athena#8](https://github.com/wrightkit/athena/issues/8), merged as #16). The simulator reads a scenario file as described here; where the run reaches a construct the simulator does not model, the trace says so (`outcome.status` `not_modeled`).

A scenario is a JSON file. It tells the simulator who is in the match, which events happen and when, and what the author expects to see. Agents and `wright ui` write it. `wright` passes it to `athena` unchanged ([ADR-0033](https://github.com/wrightkit/wright/blob/main/docs/adr/0033-simulate-operation.md), decisions 2 and 5). The machine-readable definition is [scenario.schema.json](scenario.schema.json). The entry point and trace are in [simulator-interface.md](simulator-interface.md).

## Decisions for the owner

Each item below is a choice made in this proposal. Any of them can change.

1. **Seconds are part of the scenario.** `seconds` is required, so a scenario describes a whole run. The alternative is a separate caller option.
2. **Players are a list with a text `id`.** Events and expectations point to a player by `id`. The alternative is to number players.
3. **`hero` is free text in the schema.** `athena` checks it against the hero names of `workshop-rs` when it loads the scenario. The schema stays small and does not need updating with each game patch.
4. **`team` is `team1` or `team2` only.** There is no "all" team, because a player is always on one team.
5. **Player `state` means player variables in version 1.** It is a map from the variable name in the program to its start value. A variable not listed starts as `null` (decision 13). Version 2 renames the field to `vars` and gives `state` the player-state meaning of decision 19. World state (health pools, cooldowns, ultimate charge) starts from the `workshop-rs` hero-values catalog unless a `state` key supplies it (athena#9). Position is not modeled.
6. **Event `kind` is the Workshop event name** (for example `playerDealtFinalBlow`). The schema does not list the names.
7. **Times are seconds from the first tick.** Events at the same time run in file order.
8. **Observations are checked by `athena`.** The trace carries one result per expectation. The alternative is that each consumer compares the trace itself. Checking in one place keeps `wright` free of Workshop logic.
9. **Expectations are checked at the end of the run only,** and only for three kinds: a global variable, a player variable, a log line. A rule for "value at time T" can come later.
10. **A missing live input stops the run.** It stops at the first one met, as for an unmodeled construct, and the trace lists it. The alternative is to continue and collect all of them. Continuing would need a guess for the missing value, and the simulator never guesses.
11. **An unknown field is an error.** The schema forbids extra fields. Typos in an agent-written file are then caught, and version 1 stays closed.
12. **The schema file lives in `docs/`.** CI skips runs for `docs/**` changes, so a change to the schema alone does not start the schema check. A change to the test or to Rust code does.
13. **A scenario is a mock: what it does not declare takes the default value, which is `null`.** This is the owner's rule. The repository does not establish it as game behavior. No probe capture and no `workshop-rs` document records what an undeclared variable holds in the real client. The value `null` is what `athena` already uses for a global variable that was never set (`simulator-interface.md`, and `Null` reads as `0` in a number context, `architecture.md`). It is therefore a **modeled** choice, not a **measured** one. A probe is needed before it counts as measured.
14. **Randomness is a seed, supplied by the scenario.** The client's generator is unmeasured, and the simulator must stay deterministic, so `random` carries a `seed` that drives a splitmix64 stream the `Random *` values consume in evaluation order (athena#65). The mapping from draws to values is **modeled, not measured** — no client capture records the distribution. A `Random *` value in a run without `random` is a group 2 missing input. The alternative — an explicit sequence of draws — can come later; a `random` object leaves room for it.
15. **Server load is a constant, supplied by the scenario.** Server load is a client-measured quantity, and the simulator must not guess it, so `serverLoad` carries the three numbers `Server Load`, `Server Load Average` and `Server Load Peak` read — `{ "average": n, "peak": n, "current": n }`, constant for the run (athena#67). The constant is **modeled, not measured**: real load varies per tick. A `Server Load *` value in a run without `serverLoad` is a group 2 missing input; a partial object is malformed, because a field that exists only sometimes is easier to miswrite than to read.
16. **The map is a constant, supplied by the scenario.** The lobby chooses the map outside the Workshop script, and the simulator must not guess it, so `map` carries the name `Current Map` reads — a `Map` member id of the `workshop-rs` catalog, in the same form `hero` takes (athena#71). The constant is **modeled, not measured**: a mid-run map change is not modeled. A `Current Map` value in a run without `map` is a group 2 missing input. Other map-derived values stay `not_modeled` unless they read the same field and the catalog.
17. **Position and facing are per-player inputs, supplied by the scenario.** The game moves and turns players continuously, and the simulator must not guess them, so a player may carry `position` — `{ "x": n, "y": n, "z": n }` in the Workshop frame (x left, y up, z forward) — and `facing` — `{ "horizontal": n, "vertical": n }` in degrees, horizontal 0 forward increasing left, vertical 0 level increasing downward (athena#74). Both are **modeled, not measured**: `facing` holds for the whole run; `position` holds until a `Teleport` writes it — nothing else moves a player. Supply each field whole or not at all — a partial object is malformed. A `position`/`facing` read with no field is a group 2 missing input. Reads that need map geometry — raycasts, line of sight, walkable positions, `Is On Ground` — answer for an empty world ([simulator-interface.md](simulator-interface.md#geometry-empty-world)); `Eye Position` reads the version-2 `state` map (decision 19).
18. **Objective positions are a list of points, supplied by the scenario.** The map and mode place the objectives outside the Workshop script, and the simulator must not guess them, so `objectives` carries a list of positions `{ "x": n, "y": n, "z": n }` in the Workshop frame, entry `i` the position `Objective Position(i)` returns (athena#82). The argument is the catalog's "index of the objective to consider, starting at 0 and counting up", read under the number coercion — the boolean form `False`/`True` names indexes 0/1. The list is **modeled, not measured**: it is constant for the run, and objective movement (payload progress, point capture) is not modeled. An `Objective Position` read with no `objectives` field, or an index that names no entry — out of range, negative, fractional or non-finite — is a group 2 missing input naming `objectives[i]`. Other objective-based values (`Objective Index`, `Is Objective Complete`, …) stay `not_modeled`. An entry that is not a whole position object is malformed, as for `position`.
19. **Player state is a keyed map, supplied by the scenario (version 2).** The game tracks per-player state the program reads continuously — health, velocity, whether the player is moving — and the simulator must not guess it, so version 2 adds `state`, an object keyed by the accepted ids the schema lists (athena#89). Each declared value is **scenario-supplied, not simulated** — `Damage` does not move `health` — and no read derives it, except the two derivations the catalog states exactly: `Speed Of` is the length of `velocity` (it reads no key of its own), and `Is Moving` is *not* derived — a scenario supplies it directly. The state-write actions of athena#116 (`Set Ultimate Charge`, `Set Player Health`, `Set Max Health`, `Kill`/`Respawn`/`Resurrect`) write a cell over the map: a write defines a key the scenario never supplied, a read after a write returns the written value, and a read before any write of a key the map does not list is a group 2 missing input naming `state.<key>` — the input contract is unchanged, since the simulator still never fills a key in. A player that declares `state` opts into it fully: an unknown key or a wrong JSON type is malformed. A player without `state` keeps the established models — `Health`/`Max Health` read the pools `Damage`/`Heal` move, `Is In Spawn Room` is false in the empty world — and every other listed read is a missing input. Reads that take extra arguments (`Has Status`, `Ability Cooldown`, `Player Stat`) are not player-state reads and stay `not_modeled`. `state` also decides whether an authored hero setting the simulator does not apply fails a player's world resolution or is skipped ([architecture.md](architecture.md), athena#91): skipped only when the map declares every key the setting can move — `ultCharge` for `ultGen%`, `passiveUltGen%`, `combatUltGen%` and `enableSpawningWithUlt`, `health`, `maxHealth` and `normalizedHealth` for `passiveHealthRegen` and `enableRolePassive` — and failed otherwise, with the message naming the keys that would clear it. A skipped setting means the scenario-supplied value was used, not that the setting was simulated.
20. **A player's lobby slot is a scenario input.** The lobby seats each player outside the Workshop script, and roster order is not a measured slot, so a player may carry `slot` — an integer from 0 to 11, unique among the players (athena#99). `Slot Of` reads it and a `Slot N` rule-event filter matches it; the field is **scenario-supplied, not simulated**, constant for the run. A `Slot Of` read on a player with no `slot` is a group 2 missing input naming `slot`; a player without `slot` does not match a `Slot N` filter, as an undeclared `team` or `hero` does not match its filter. A `slot` outside the range or a non-integer is malformed, and two players sharing a slot is a loader error.
21. **The match phase is a scenario input.** The lobby's phase machine runs outside the Workshop script and the simulator does not run a match, so `matchPhase` carries the phase the phase reads report — one of `waitingForPlayers`, `assemblingHeroes`, `setup`, `gameInProgress`, `betweenRounds` or `matchComplete`, the six phases the catalog's `Is Waiting For Players`, `Is Assembling Heroes`, `Is In Setup`, `Is Game In Progress`, `Is Between Rounds` and `Is Match Complete` describe (athena#101). Each catalog description names exactly one phase, so a read answers true exactly for its own phase and false for the others; the field is **scenario-supplied, not simulated**, constant for the run. A phase read with no `matchPhase` is a group 2 missing input naming `matchPhase`; an unknown value is refused. Overtime is no catalog phase, so the field cannot express it.
22. **The game mode and the local player are scenario inputs.** The lobby chooses the game mode outside the Workshop script and the client binds `Local Player` to the end user's player, so `gameMode` carries the mode `Current Game Mode` reads — a `Gamemode` member id of the `workshop-rs` catalog (for example `SKIRMISH`) or its en-US spelling (`Skirmish`), matched case-insensitively, the same form `map` takes (athena#105) — and a player may carry `local`, which marks the one player `Local Player` returns, as `host` marks `Host Player`. Both are **scenario-supplied, not simulated**, constant for the run; a mid-run mode change is not modeled. `Current Game Mode` yields the member id as text — the form a `Game Mode` literal yields, so the two compare equal. A `Current Game Mode` read with no `gameMode` is a group 2 missing input naming `gameMode`; a `Local Player` read in a scenario with no `local` player is a group 2 missing input naming `local`; an unknown `gameMode` or two `local` players is refused.

## Top-level shape

| Field | Required | Meaning |
| --- | --- | --- |
| `scenarioVersion` | yes | The format version: the integer `1` or `2`. |
| `seconds` | yes | Simulated time, greater than zero, counted from the first tick. |
| `players` | yes | List of players. It may be empty. |
| `events` | no | Events injected into the run. |
| `expect` | no | Observations the author expects at the end of the run. |
| `random` | no | The source `Random *` values draw from: `{ "seed": n }`, `n` an unsigned 64-bit integer. Write `n` as an integer literal: the loader rejects a decimal point or exponent, so `1.0` and `1e3` fail although the schema admits them. |
| `serverLoad` | no | The numbers `Server Load`, `Server Load Average` and `Server Load Peak` read: `{ "average": n, "peak": n, "current": n }`, constant for the run. |
| `map` | no | The map `Current Map` reads: a `Map` member id of the `workshop-rs` catalog (for example `KINGS_ROW`) or its en-US spelling (`King's Row`), matched case-insensitively — the same form `hero` takes. |
| `matchPhase` | no | The match phase the phase reads answer: `waitingForPlayers`, `assemblingHeroes`, `setup`, `gameInProgress`, `betweenRounds` or `matchComplete` — the six phases the catalog's phase reads define. |
| `gameMode` | no | The game mode `Current Game Mode` reads: a `Gamemode` member id of the `workshop-rs` catalog (for example `SKIRMISH`) or its en-US spelling (`Skirmish`), matched case-insensitively — the same form `map` takes. |
| `objectives` | no | The positions `Objective Position` reads: a list of `{ "x": n, "y": n, "z": n }` in the same frame as `position`, entry `i` indexed by the evaluated argument — a number, or a boolean under the number coercion (`False`/`True` as 0/1). A read with no field, or an index naming no entry, is a missing input. |

### Player

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Text that names the player in this file. It must be unique; the loader checks this (see Loader checks). |
| `hero` | no | Hero name, for example `ana`. |
| `team` | no | `team1` or `team2`. |
| `host` | no | `true` marks the player `Host Player` returns. At most one player has it. Default `false`. |
| `local` | no | `true` marks the player `Local Player` returns. At most one player has it. Default `false`. |
| `state` | no | Version 1: map from player-variable name to start value (a variable not listed starts as `null`). Version 2: the player-state map of decision 19 — the values the no-extra-argument player reads return, keyed by the accepted ids. |
| `vars` | no | Version 2 only: map from player-variable name to start value — the version-1 `state` renamed. Malformed in version 1. |
| `buttons` | no | Map from button slot (`jump`, `ability1`, …) to the held state `Is Button Held` reads. A slot not listed is a missing input when the program reads it. |
| `position` | no | The player's position `{ "x": n, "y": n, "z": n }` (x left, y up, z forward), read by `Position Of`, the position operands, the geometry reads and a `Player` destination of `Teleport` — a `Player` start or end of a raycast or `Is In Line of Sight` resolves two meters above it — and written by `Teleport`. A read with no field is a missing input. |
| `facing` | no | The player's facing `{ "horizontal": n, "vertical": n }` in degrees, read by `Facing Direction Of` and `Local`/`World Vector Of`. A read with no field is a missing input. |
| `slot` | no | The player's lobby slot: an integer from 0 to 11, read by `Slot Of` and matched by a `Slot N` rule-event filter — a player without one does not match such a filter. A `Slot Of` read with no field is a missing input; two players may not share a slot (see Loader checks). Write it as an integer literal, as for `random.seed`. |

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
- Player `slot` values are unique.
- Every `eventPlayer` and every expectation `player` is the `id` of a player in the file.
- Every `hero` is a hero name known to `workshop-rs` (decision 3): the canonical `Hero` member id of the catalog (for example `JUNKER_QUEEN`) or its en-US spelling (`Junker Queen`), matched case-insensitively.
- `map`, when present, is a map name known to `workshop-rs`: the canonical `Map` member id of the catalog (for example `KINGS_ROW`) or its en-US spelling (`King's Row`), matched case-insensitively.
- `gameMode`, when present, is a game-mode name known to `workshop-rs`: the canonical `Gamemode` member id of the catalog (for example `SKIRMISH`) or its en-US spelling (`Skirmish`), matched case-insensitively.
- At most one player has `"host": true`.
- At most one player has `"local": true`.
- `scenarioVersion` is checked first (see Version).

## Defaults and missing inputs

A scenario is a mock. Every fact it does not declare is handled by one of two rules. Which rule applies depends on the kind of fact.

**Group 1: the game gives a default. The scenario may omit it, and the run goes on with `null`.**

- A global variable. The program owns it.
- A player variable that the scenario does not list in a player's `state`.

The value is `null`, as in decision 13. No report is made. The trace shows the effect: `globals` holds `null` for a global that was never set.

**Group 2: the game supplies it from live play, and it has no default. This is a missing input.**

- The `hero` of a player, when the program reads it and the player has no `hero` — `Hero Of`, or a player `Is Hero Being Played` or `Players On Hero` reads on the team.
- The `team` of a player, when the program reads it and the player has no `team`.
- The event player, when the program reads it and the event has no `eventPlayer`.
- An event value, when the program reads it and the event has no such entry in `values`.
- The held state of a button, when `Is Button Held` reads it and the player's `buttons` does not list the slot.
- The random source, when a `Random *` value reads it and the scenario has no `random`.
- The server load, when a `Server Load`, `Server Load Average` or `Server Load Peak` value reads it and the scenario has no `serverLoad`.
- The map, when a `Current Map` value reads it and the scenario has no `map`.
- The match phase, when a phase read (`Is Waiting For Players`, `Is Assembling Heroes`, `Is In Setup`, `Is Game In Progress`, `Is Between Rounds` or `Is Match Complete`) evaluates and the scenario has no `matchPhase`.
- The game mode, when `Current Game Mode` evaluates and the scenario has no `gameMode`.
- The local player, when `Local Player` evaluates and no player is marked `local`.
- The `position` of a player, when a position read (`Position Of`, a `Player` operand of `Distance Between`/`Vector Towards`/`Direction Towards`/`Add`/`Subtract`/`Multiply`/`Divide`, a `Players Within Radius` player, a `Local`/`World Vector Of` translation, a `Player` start or end of `Ray Cast Hit Position`/`Ray Cast Hit Player`/`Ray Cast Hit Normal`/`Is In Line of Sight`, a `Player` argument of `Nearest Walkable Position`, a `Player` destination of `Teleport`) names a player with no `position`.
- The `facing` of a player, when a facing read (`Facing Direction Of`, `Vertical Facing Angle Of`, `Local`/`World Vector Of`) names a player with no `facing`.
- The `slot` of a player, when `Slot Of` reads it and the player has no `slot`.
- The `objectives` entry for an objective index, when `Objective Position` reads it and the scenario has no `objectives` or no entry at the evaluated index.
- A `state.<key>` entry, when a player-state read of decision 19 consults it and the player has no `state` or the map does not list the key and no action wrote it — unless an established model answers it (`Health`/`Max Health` from the pools, `Is In Spawn Room` from the empty world).

An input of group 2 is **missing** when the program reads it during the run and the scenario does not supply it. The simulator never fills it in. It stops the run at that read, and the trace names the input.

Modeled reads in version 1: the event player (`Event Player`), a player's hero (`Hero Of`), team (`Team Of`) and lobby slot (`Slot Of`, read from `slot`), a player variable, a button's held state (`Is Button Held`, looked up in the player's `buttons` by the evaluated slot), the server load (`Server Load`, `Server Load Average` and `Server Load Peak`, read from `serverLoad`), the map (`Current Map`, read from `map`, yielding the catalog `Map` member id as text — the form a `Map` literal yields, so the two compare equal), the match phase (`Is Waiting For Players`, `Is Assembling Heroes`, `Is In Setup`, `Is Game In Progress`, `Is Between Rounds` and `Is Match Complete`, each read from `matchPhase` and true exactly for the declared phase), the game mode (`Current Game Mode`, read from `gameMode`, yielding the catalog `Gamemode` member id as text — the form a `Game Mode` literal yields, so the two compare equal), and the scalar event values `eventDamage`, `eventHealing`, `eventWasCriticalHit`, `eventWasEnvironment` and `eventWasHealthPack` — each looked up in `values` by its catalog name. `Host Player` returns the player marked `host`; without one it stops the run as `not_modeled`. `Local Player` returns the player marked `local`; without one the read is a group 2 missing input naming `local`. `Has Spawned` of a player with a `hero` is true from the first tick; `Is Alive` answers the `isDead` read where one answers — a declared `state.isDead` or a cell an action wrote — and the same hero model otherwise (modeled, not measured, [architecture.md](architecture.md#choices-no-capture-covers-yet)). For a player without a `hero`, `Has Spawned` — and `Is Alive` when `isDead` cannot answer — stops the run as `not_modeled`. `Is Hero Being Played(hero, team)` is true when at least one scenario player on `team` — `All Teams` selects every player — has `hero` as its declared `hero` (modeled, not measured; the hero-selection screen is outside the model); a player on the team without a `hero` is a group 2 missing input, and a `hero` or `team` argument that is not a literal catalog member after evaluation is `not_modeled`. `Players On Hero(hero, team)` yields the scenario players on `team` that have `hero` as their declared `hero`, in roster order, under the same declared-`hero` read: a player on the team without a `hero` is a group 2 missing input, and a `hero` or `team` argument that is not a literal catalog member after evaluation is `not_modeled`. Other event-scoped values (`attacker`, `victim`, `eventAbility`, `eventDirection`) are not modeled and stop the run as `not_modeled`. An injected event fires every rule whose event kind matches `kind` and whose `team`/`player` filters the `eventPlayer` satisfies — `All` always matches, `Team 1`/`Team 2` match the declared `team`, a hero name matches the declared `hero`; an event without an `eventPlayer` satisfies only `All`/`All`, and evaluating a filtered rule against it reads the event player, a group 2 missing input. A `Slot N` filter matches the declared `slot` — a player without one does not match — and a filter no evaluation reaches never fires and is not an error. An `Ongoing - Each Player` rule becomes one instance per matching player with `Event Player` bound to it ([simulator-interface.md](simulator-interface.md#player-scoped-rules)). A player's `position` is read by `Position Of`, by the `Player` operands of the vector values and by the `Player` position arguments of the geometry reads (`Ray Cast *`, `Is In Line of Sight`, `Nearest Walkable Position`) and by a `Player` destination of `Teleport`; a player's `facing` is read by `Facing Direction Of`, `Vertical Facing Angle Of` (the vertical component) and `Local`/`World Vector Of` — each a group 2 input named `position`/`facing`. `Players Within Radius(center, radius, team, losCheck)` yields the scenario players on `team` whose `position` lies within `radius` of `center`, in roster order — the catalog notes dead players are picked up, so no life-state filter applies — and `losCheck` is evaluated but holds in the empty world. An objective position is read by `Objective Position(i)` from `objectives[i]` — a group 2 input named `objectives[i]`, `i` the evaluated index.

In version 2, a player that declares `state` answers the no-extra-argument player-state reads from it (decision 19): `Health`, `Max Health`, `Normalized Health`, `Velocity Of`, `Speed Of` (the length of `velocity`), `Eye Position`, `Is Dead`, `Is Using Ultimate`, `Is Meleeing`, `Is Firing Secondary`, `Is In Alternate Form`, `Is Communicating Any Emote`, `Is In Spawn Room`, `Is Dummy Bot`, `Ultimate Charge Percent` and `Is Moving`, and `Is Using Ability 1`, `Is Using Ability 2`, `Is Firing Primary` (the `isFiringPrimaryFire` key) and `Is Duplicating` (the `isDuplicatingAHero` key) — each scenario-supplied, not simulated.

A malformed scenario is a different case. It is refused and nothing runs ([ADR-0033](https://github.com/wrightkit/wright/blob/main/docs/adr/0033-simulate-operation.md), decision 4). The schema and the loader checks (below) define "malformed". A well-formed scenario that lacks a group 2 input gives a trace.

### Trace additions

The trace gains three fields. Adding a field is compatible ([simulator-interface.md](simulator-interface.md)).

- `geometry` is `emptyWorld` — the run answered spatial queries for an empty world, so geometry-dependent results are modeled, not measured ([simulator-interface.md](simulator-interface.md#geometry-empty-world)).
- `outcome.status` can be `missing_input`. The run stopped at the first missing input.
- `missingInputs`: a list that is empty unless the status is `missing_input`. The run stops at the first missing input, so the list has at most one entry in version 1. Each entry has these fields:

| Field | Meaning |
| --- | --- |
| `input` | One of `hero`, `team`, `eventPlayer`, `eventValue`, `button`, `random`, `serverLoad`, `map`, `matchPhase`, `gameMode`, `local`, `position`, `facing`, `objectives`, `state`, `slot`. |
| `player` | The `id` of the player the input belongs to, or `null` when no player applies (`eventPlayer`, `eventValue`, `random`, `serverLoad`, `map`, `matchPhase`, `gameMode`, `local`, `objectives`). |
| `name` | The event-value name for `eventValue`, the button slot for `button`, the field `random` for `random`, `serverLoad` for `serverLoad`, `map` for `map`, `matchPhase` for `matchPhase`, `gameMode` for `gameMode`, `local` for `local`, `position`/`facing`/`slot` for the player-field inputs, `objectives[i]` (the evaluated index) for `objectives`, `state.<key>` for `state`; `null` for the others. |
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

The file declares its version in `scenarioVersion`. The simulator supports a fixed set of versions — `1` and `2`. If the value is not in that set, or is not an integer, the simulator refuses the scenario before it reads any other field, and its error names the version found and the versions supported. It does not try to read a newer file. The schema file covers both versions: a player's shape depends on the `scenarioVersion` the file declares.

Adding an optional field does not change the version. Removing or renaming a field, or changing what a value means, makes a new version — so version 2 renames the version-1 player `state` (the variables map) to `vars` and gives `state` the player-state meaning of decision 19. A version-1 file that names `vars` is malformed.

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

On the `kill`-rule path — event injection, `Event Player`, player variables, `If`/`Else`, `Call Subroutine` — the simulator models the constructs, and `practice-range.json` yields `streak` 3, `delta` 2 and `score` 2 with both expectations `pass`. `practice-range-default-variable.json` yields `streak` 1, `delta` 1 and `score` 1, and `practice-range-missing-event-player.json` yields `outcome.status` `missing_input` with one `eventPlayer` entry. These are exercised by `crates/athena-sim/tests/scenario.rs`.

The `init` rule's `Create HUD Text` is modeled (athena#10): the full `practice-range` program now completes, and its `hud` trace lists one `hudText` element with recipient `p1` and the header evaluated at creation. The shape of the `hud` field is defined in [simulator-interface.md](simulator-interface.md#hud-elements).

## Invalid examples

[scenario-examples/invalid/](scenario-examples/invalid/) holds files that the schema must reject:

- `unknown-version.json`: `scenarioVersion` is 3.
- `missing-required-field.json`: `seconds` is absent.
- `wrong-value-type.json`: an event `time` is text.
- `unknown-team.json`: `team` is not `team1` or `team2`.
- `random-null.json`: `random` is `null` rather than an object.
- `seed-too-large.json`: `random.seed` is past the unsigned 64-bit range.
- `server-load-missing-peak.json`: `serverLoad` lacks `peak`.
- `map-empty.json`: `map` is the empty text.
- `game-mode-empty.json`: `gameMode` is the empty text.
- `objectives-partial.json`: an `objectives` entry lacks `z`.
- `slot-out-of-range.json`: a player `slot` is past 11.
- `match-phase-unknown.json`: `matchPhase` is not one of the six phases.

`crates/athena-sim/tests/scenario_schema.rs` checks that every file in `valid/` passes and every invalid example fails. `cargo test` and CI run it.
