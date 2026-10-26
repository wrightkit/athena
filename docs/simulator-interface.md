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
| `geometry` | The geometry model the run answered spatial queries with: `emptyWorld` — the world has no map geometry, so raycasts never hit, line of sight always holds and every position is walkable and on the ground. **Empty world, modeled not measured**; a viewer should warn that geometry-dependent results are not real. See [Geometry (empty world)](#geometry-empty-world). |
| `logs` | `Log To Inspector` lines in order, each with the `tick` it was written on. |
| `globals` | Global variables at the end of the run. A declared variable that was never set is `null`. |
| `players` | The player state table: one entry per scenario player, keyed by its `id`. Empty when the run had no scenario. See below. |
| `hud` | The HUD elements the run created, in creation order. See [HUD elements](#hud-elements). |
| `effects` | The effect entities the run created or played, in creation order. See [Effect entities](#effect-entities). |
| `timeline` | The per-tick timeline: `{ "entries": [...], "truncated": false }`. See [trace-format.md](trace-format.md). |
| `missingInputs` | The live inputs the program read but the scenario did not supply. Empty unless `outcome.status` is `missing_input`; at most one entry in version 1. Each entry: `input` (`hero`, `team`, `eventPlayer`, `eventValue`, `button`, `random`, `serverLoad`, `map`, `matchPhase`, `gameMode`, `local`, `position`, `facing`, `objectives`, `state`, `slot`), `player` (the player `id` the input belongs to, `null` for `eventPlayer`, `eventValue`, `random`, `serverLoad`, `map`, `matchPhase`, `gameMode`, `local` and `objectives`), `name` (the event-value name for `eventValue`, the button slot for `button`, the scenario field's own name for `random`, `serverLoad`, `map`, `matchPhase`, `gameMode`, `local`, `position`/`facing`/`slot` for the player-field inputs, `objectives[i]` — the evaluated index — for `objectives`, `state.<key>` for `state`, `null` for the others), `tick`, `construct`, `location` (where the read happened, as under `not_modeled` below). |
| `expectations` | One result per scenario expectation, in file order: `index`, `status` (`pass`, `fail`, `not_evaluated`), `actual` (the observed value, `null` when it does not exist). |

`logs`, `globals`, `players` and `hud` describe the state when the run ended: after the last tick for `complete`, at the stop for `not_modeled` and `missing_input`. `timeline` covers the ticks before the stop the same way.

### `players` rows

Each row repeats the declared `hero` (the scenario text, `null` when undeclared) and `team` (`team1`/`team2`, `null` when undeclared), plus `state` — the player's `state` values at the end of the run, the declared map overlaid by the cells the state-write actions wrote — `modifiers` — the modifiers the stored-modifier actions recorded on the player, keyed by the modifier name — and `world`, the catalog-derived state at the end of the run:

| `world` field | Meaning |
| --- | --- |
| `health` | The `health` pool: `{ "current": ..., "max": ... }`. |
| `shield` | The `shield` pool, same shape; `null` when the catalog records none. |
| `armor` | The `armor` pool, same shape; `null` when the catalog records none. |
| `cooldowns` | Cooldown seconds per ability slot, keyed by `slot` or `slot/variant`; the catalog's recorded slots appear scaled by the program's hero settings, and a `Set Ability Cooldown` action writes a slot's reported seconds literally. |
| `ultimate` | `{ "charge": ..., "cost": ... }`: charge points toward the catalog's cost. Read-only in version 1. |
| `statuses` | The status effects `Set Status` applied, keyed by `Status` member id; the value is the `Total Time Elapsed` seconds at which the status ends. `Has Status` reads it as held while its expiry is still ahead. `Clear Status` removes the entry. |
| `buttons` | The `Allow Button`/`Disallow Button` writes, keyed by button slot (`jump`, `ability1`, …); a slot never written is allowed. |

`world` is `null` when the run could not resolve the player's state — no `hero`, a hero name the file does not have, or an authored hero setting the simulator does not apply that the player's `state` map does not cover. `Damage`/`Heal` on or dealt by such a player, a `Set Ability Cooldown`, `Set Status`/`Clear Status` or `Allow`/`Disallow Button` on such a player, or a `Health`/`Max Health`/`Ability Cooldown`/`Has Status` read of such a player that no `state` key answers (see Player state values), stops the run as `not_modeled`; the row still reports the declared `hero`/`team`.

### Outcome and "not modeled"

- `complete`: the whole requested time was simulated.
- `not_modeled`: the run met a construct the simulator has no passing capture fixture for, and stopped there. The outcome carries `construct` (a short description, for example `action wait` or `value getPosition`), `tick` (the tick on which it was met, or `null` if it was found before the first tick, for example an unsupported rule event) and `location` (below).
- `missing_input`: the run met a live input the scenario does not supply and stopped at that read. The input is named in `missingInputs`, whose entry carries the same `location`; expectations are all `not_evaluated`.

`location` names where the run stopped: `rule` is the index of the rule holding the stopped construct among the program's enabled rules — inside `Call Subroutine`, the subroutine's own, as in a `rules` timeline element — `name` is the rule's name, `eventPlayer` is the `Event Player` the stopping evaluation ran for (`null` when none applied), and exactly one of `action` or `condition` gives the zero-based index inside that rule. A stop no single statement owns — a rule's event filter, or an action block that failed to load — reports only `rule`, `name` and `eventPlayer`; `location` is `null` only when no rule was involved.

The simulator never skips an unmodeled construct and never replaces it with a default value. A consumer must treat a trace with `outcome` other than `complete` as partial. The text of `construct` is for people; do not match on it.

### Player-scoped rules

The `team` and player (`target`) filters of a rule event resolve against the scenario players: `All` imposes no constraint, `Team 1`/`Team 2` match the declared `team`, a hero name matches the catalog `Hero` member the declared `hero` resolves to, and `Slot N` matches the declared `slot` — a player that does not declare the filter's field does not match. A `Slot` filter no evaluation reaches — the `team` arm fails first for every player, or no matching event is injected — never fires and is not an error. `Slot Of(player)` reads the same field: the lobby seats players outside the script, so the value yields the declared `slot` as a number, a read on a player without `slot` is a `missing_input` of kind `slot`, and a non-player operand is `not_modeled`.

- An `Ongoing - Each Player` rule becomes one instance per matching scenario player, ordered by scenario `id`. Each instance binds `Event Player` to its player — also inside a subroutine it calls — and has its own condition edge, `Wait`/`Wait Until` and wait-behavior state. A filter matching nobody yields no instance: the rule never fires and is not an error.
- A player event rule (`Player Died`, `Player Joined Match`, …) runs for an injected event only when the event's `eventPlayer` satisfies the filters. An event that names no `eventPlayer` satisfies only `All`/`All`; evaluating a filtered rule against it is a `missing_input` of kind `eventPlayer`.

### General and array values

`If-Then-Else`, `Compare` (all six operators), `Not`, `Or`, `Divide`, `Modulo`, `Min`, `Max`, `Raise To Power`, `Round To Integer`, `Sine From Radians`, `First Of`, `Last Of`, `Count Of`, `Empty Array`, `Array Contains`, `Index Of Array Value`, `Array Slice`, `Append To Array`, `Remove From Array` and `Value In Array` are modeled, as are the `Set`/`Modify * Variable At Index` actions and the full `Modify` operator set (`Add` through `Remove From Array By Index`). Coercions follow the Workshop rules: numbers, booleans and `Null` read as numbers (`Null` and `False` are `0`), `Null` reads as the empty array, and an enum literal in a value position yields its catalog member id as text. `==`/`!=` compare arrays element-wise; different types are never equal. `Value In Array` yields the element at the index, or `Null` when none exists — an out-of-range or negative index, or a non-array operand (athena#97). A `Set * At Index` past the end appends; a negative index writes nothing. A `Modify * At Index` outside the array writes nothing. `Remove From Array` drops every element equal to the operand — an array operand, every element equal to any element it holds — and `Remove From Array By Value` drops every match. `And`/`Or`/`Not`/`If-Then-Else` read truthiness (`Null` and `0` are false) and `And`/`Or` short-circuit; `If-Then-Else` reads only the taken branch.

The element-context values: `Evaluate Once` pins its operand's first read per site for the rest of one scope — a rule-instance run (loop bodies and restarted frames re-read the site; a `Wait` suspension keeps them; the next run start or restart clears them), one rule-condition check pass, or one `Wait Until` call's per-tick re-checks — and inside an element-context operand, re-read per element, it is `not_modeled`. `Filtered Array` keeps the elements its condition holds for, `Sorted Array` orders ascending by the per-element key — a number, `Null` or boolean under the number coercion, and an equal key keeps the element's place — and `Is True For All`/`Is True For Any` test the condition per element, stopping at the deciding element; an empty array is true for `All`, false for `Any`. Each binds `Current Array Element` and `Current Array Index` to the element and its place during evaluation, nested contexts included; a read outside one is `not_modeled`. `Mapped Array` is not modeled.

`All Heroes` yields the catalog's `Hero` members in catalog order: an array of member-id texts — the form `Hero Of` and a `Hero` literal yield. The client's element order is unmeasured; the catalog order is the modeled order.

None of these has a client capture; all are **modeled, not measured**.

### String values

`Char In String`, `String Split`, `Index Of String Char`, `String Replace` and `Custom String` are modeled. A string parameter coerces its argument: text as itself, an empty array as the empty string (the Workshop's empty-string spelling), and a number, boolean or `Null` by the `Custom String` render coercion (numbers with two decimals, `Null` as `0`); a non-empty array is `not_modeled`. Characters index and count as characters, not UTF-8 bytes.

- `Char In String` yields the character at the index; an index outside the string's bounds — negative or past the end — yields the empty string, as does the empty string itself.
- `String Split` splits around the separator; a separator the source does not contain yields the source alone, an empty separator splits into characters, and an empty source yields the empty array.
- `Index Of String Char` reports the needle's first index, or -1 when absent; an empty needle names no character and reports -1.
- `String Replace` replaces every occurrence of the pattern; an empty pattern is `not_modeled`.
- `Custom String` replaces `{0}`/`{1}`/`{2}` with the rendered arguments it was given; a placeholder without an argument stays literal. One holding a display value yields a `customString` display value (below).

Whether the client still caps a produced string at 511 bytes is unmeasured — the sources conflict on whether the limit was removed — so a produced string over 511 bytes, a `String Split` part included, is `not_modeled`.

None of these has a client capture; all are **modeled, not measured**.

### Display values

`Icon String`, `Hero Icon String`, `Ability Icon String`, `Input Binding String` and `Custom Color` are modeled: each produces a client rendering token no modeled construct can read, so the value stays opaque — `{ "type": "display", "value": { "producer": "<value id>", "args": [<evaluated arguments>] } }`, never the token's glyph, key name or color. It flows through variables, arrays and the text/color arguments of HUD and effect actions. A `Custom String` holding one is itself display (`producer` `customString`, the template the first argument). Two display values compare equal on identical producer and recursively-equal arguments; every other comparison — a `customString` display's included, its content being the rendered text — and every content read (the string reads above, arithmetic, an index, a sort key, a coercion) is `not_modeled`.

No capture covers the client tokens; this is **modeled, not measured**.

### Random values

`Random Integer`, `Random Real`, `Random Value In Array` and `Randomized Array` are modeled. The client draws nondeterministically and the simulator must stay deterministic, so draws come from the scenario's `random` source — `{ "random": { "seed": n } }` drives a splitmix64 stream that the values consume in evaluation order, each draw a fraction `u` in `[0, 1)`. A `Random *` value in a run without `random` is a `missing_input` of kind `random`. Every evaluated random value records a `randomDraw` change on its tick.

- `Random Integer(min, max)` floors each bound to an integer and draws an inclusive value; a `Min` past `Max` swaps the bounds.
- `Random Real(min, max)` is `min + (max − min)·u`, so the bound order swaps the same way and the range is `[min, max)`.
- `Random Value In Array(array)` draws an index; an empty array — `Null` reads as one — yields `Null` without a draw.
- `Randomized Array(array)` is a Fisher-Yates shuffle of the same stream, last index down; an empty or one-element array draws nothing.

A non-finite bound is `not_modeled`. No capture covers the client's generator or the distribution; all of this is **modeled, not measured**.

### Server load values

`Server Load`, `Server Load Average` and `Server Load Peak` are modeled. Server load is a client-measured quantity the simulator does not guess, so the values read the scenario's `serverLoad` — `{ "serverLoad": { "average": n, "peak": n, "current": n } }` — constant for the run: `Server Load` reads `current`, `Server Load Average` reads `average`, `Server Load Peak` reads `peak`. A `Server Load *` value in a run without `serverLoad` is a `missing_input` of kind `serverLoad`. A read records no change; the field is an input, not a draw.

No capture covers it; this is **modeled, not measured**.

### Map and game mode values

`Current Map` and `Current Game Mode` are modeled. The lobby chooses the map and the mode outside the Workshop script, and the simulator does not guess them, so the values read the scenario's `map` and `gameMode` — a `Map` or `Gamemode` member id of the `workshop-rs` catalog or its en-US spelling, matched case-insensitively — and yield the member id as text: the same form a `Map` or `Game Mode` literal yields, so the two compare equal. Both are constant for the run; a mid-run change is not modeled. A read without the field is a `missing_input` of kind `map` or `gameMode`; a read records no change — the fields are inputs.

### Match phase

`Is Waiting For Players`, `Is Assembling Heroes`, `Is In Setup`, `Is Game In Progress`, `Is Between Rounds` and `Is Match Complete` are modeled. The match phase is a lobby fact the simulator does not run, so the values read the scenario's `matchPhase` — one of the six phase ids `docs/scenario.schema.json` lists — and each answers true exactly for its own phase and false for the others: the catalog descriptions name one phase per read, so no read is true across two declared phases. A phase read in a run without `matchPhase` is a `missing_input` of kind `matchPhase`. A read records no change; the field is an input, not a draw.

No capture covers the lobby's phase machine; this is **modeled, not measured**.

### Objective positions

`Objective Position` is modeled. The position of an objective depends on the map and mode, which the lobby decides outside the Workshop script, so the value reads the scenario's `objectives` — `{ "objectives": [{ "x": n, "y": n, "z": n }, …] }`, a list of positions in the same frame `players[].position` uses, indexed by the evaluated argument: the catalog's "index of the objective to consider, starting at 0 and counting up". The argument reads under the number coercion, so `False`/`True` name indexes 0/1. A read with no `objectives` field, or whose evaluated index names no entry — out of range, negative, fractional or non-finite — is a `missing_input` of kind `objectives` named `objectives[i]`; there is no default, and objective movement is not modeled. Other objective-derived values (`Objective Index`, `Is Objective Complete`, `Payload Position`, …) are not modeled. A read records no change; the field is an input, not a draw.

No capture covers the positions a mode's objectives take; this is **modeled, not measured**.

### Player state values

In scenario version 2, a player may declare `state`: an object keyed by the accepted ids `docs/scenario.schema.json` lists. The no-extra-argument player reads enumerated in `docs/scenario-format.md` (decision 19) answer from it — each value **scenario-supplied, not simulated**. The two derivations the catalog states exactly are the only exceptions to the no-derivation rule: `Speed Of` yields the length of `velocity` (it reads no key of its own), and `Is Moving` is *not* derived — the scenario supplies it.

A player that declares `state` opts into it fully: a key the program reads but the map does not list is a `missing_input` of kind `state` named `state.<key>` — for `Speed Of`, `state.velocity`. An unknown `state` key or a wrong JSON type refuses the scenario as malformed. A player without `state` keeps the established models — `Health`/`Max Health` read the `world` pools `Damage`/`Heal` move, `Is In Spawn Room` is false in the empty world — and every other listed read is a `missing_input`. Reads that take extra arguments (`Has Status`, `Ability Cooldown`, `Player Stat`) are not state reads and stay `not_modeled`. A read records no change; the field is an input, not a draw.

The state-write actions of athena#116 write a cell over the map (see [State-cell writes and stored modifiers](#state-cell-writes-and-stored-modifiers)): a read consults the cell first, then the declared map, then — for a player without `state` — the established models. A write defines a key the scenario never supplied; a read before any write of an undeclared key is the `missing_input` above.

### Vector values

`Vector`, `X/Y/Z Component Of`, `Vector Towards`, `Direction From Angles`, `Distance Between`, `Dot Product`, `Cross Product`, `Normalize`, `Magnitude`, `Angle Between Vectors`, `Horizontal Angle From Direction`, `Vertical Angle From Direction`, `Direction Towards` and `Local`/`World Vector Of` are modeled, as is the vector form of `Add`, `Subtract`, `Multiply` and `Divide` — component-wise on two vectors, scalar-broadcast against a number — and the `Modify` operations of the same names. The frame is the catalog's: `x` is left, `y` is up, `z` is forward; a horizontal angle is 0 at forward, increases turning left and wraps at ±180; a vertical angle is 0 level and increases looking down. `Direction From Angles` maps a `facing` pair to the unit vector `Ryaw(h)·Rpitch(v)·(0,0,1)`, the angle-from-direction values invert it, `Vector Towards` is the raw displacement and `Direction Towards` its unit form.

The `Vector` enum's six members — `Left`, `Right`, `Up`, `Down`, `Forward` and `Backward` — evaluate as the frame's directional constants in any enum-literal position. A bare spelling that names a member in more than one domain (`Left`/`Right` against `HudPosition`, `Up`/`Down` against `Rounding`) reaches the simulator as `__ambiguous_enum` and resolves to the `Vector` member only where the argument's catalog parameter type names a vector or number operand — `Add(Left, Right)` is `(0, 0, 0)`; elsewhere it stays `not_modeled`, naming the spelling and its argument position.

A `Player` operand in a position place — `Distance Between`, `Vector Towards`, `Direction Towards`, the arithmetic four, and the `Modify` operations of the same names — reads the player's `position`, and `Position Of` returns it. An evaluated `Null` in a `Vector` position is the zero vector — the default-zero reads `number()` and `Teleport`'s position already make of `Null` (athena#108); a literal number, boolean, text or array stays `not_modeled`. `Facing Direction Of` returns the unit vector of the player's `facing`, and `Vertical Facing Angle Of` its vertical component. `Players Within Radius` yields the scenario players on `team` whose `position` lies within `radius` of `center`, in roster order — dead players included; `losCheck` holds in the empty world. `Local Vector Of`/`World Vector Of` take the `Transform` members the catalog declares — `ROTATION` (reads `facing`) and `ROTATION_AND_TRANSLATION` (reads `facing` and `position`); any other transform is `not_modeled`. An omitted `position`/`facing` is a `missing_input` naming the field.

Two edges stop rather than guess, the client's answers being unmeasured: `Normalize` and `Direction Towards` on a zero-length vector, and `Angle Between Vectors` on one. A non-finite produced component — division by zero among them — is `not_modeled` for the same reason. `Eye Position` reads the `state` map (see Player state values).

A vector serializes as `{ "type": "vector", "value": { "x": n, "y": n, "z": n } }`. Vectors compare equal by component and are not strings, conditions, sort keys or array elements the string/array coercions reach; a vector in a `Custom String` argument is `not_modeled`, its client render unmeasured.

None of these has a client capture; all are **modeled, not measured**.

### Geometry (empty world)

The run has no map geometry — the goal excludes physics and hit detection, and the owner picked "empty world" (`athena#75`). Every geometry read answers for an empty world, marked `geometry: "emptyWorld"` in the trace so the viewer can warn that geometry-dependent results are not real. **Empty world, modeled not measured**.

- A raycast never hits, and the answer follows the catalog's documented no-hit return: `Ray Cast Hit Position` yields the end position, `Ray Cast Hit Player` yields `Null`, and `Ray Cast Hit Normal` yields the direction from the end position back to the start — a zero-length cast has no direction and is `not_modeled`. A `Player` start or end resolves to the position two meters above the player's feet, as the catalog documents. The include/exclude players still evaluate but decide nothing.
- `Is In Line of Sight` is true, with the same two-meters-above-the-feet resolution for a `Player` start or end; `Nearest Walkable Position` yields its input.
- `Is On Ground` is true; `Is In Air` and `Is On Wall` are false — an empty world has none of what they ask about. `Is In Spawn Room` reads the scenario's `state.isInSpawnRoom` when the player declares `state`, and is false without one; the motion queries `Is Moving` (the `state` map) and `Is Standing` (a speed the model does not carry) are a `missing_input` and `not_modeled`.
- `Teleport` writes the player's scenario `position` to the evaluated destination — `Null` is the origin, the catalog's `zeroAsNull` coercion — and records a `world` change with `field` `position`; `Position Of` reads the new value. `Apply Impulse` changes velocity, which needs physics, and stays `not_modeled`.

The geometry answers do not depend on which player they name, but an undeclared player is `not_modeled`, and a `Player` in a position place still reads the scenario `position` — an omitted field is a `missing_input`.

### Workshop settings

A `Workshop Setting` value — `Real`, `Integer`, `Toggle`, `Combo` or `Hero` — evaluates to its declared `default` argument: the simulator runs one lobby and no player edits settings. `min`/`max`, `options` and `sortOrder` are authoring metadata the running rules never see. A `Hero` setting yields its `Hero` literal's catalog member id as text — the member id `Hero Of` yields, so the two compare equal. The scenario format has no place for setting overrides, so a run always sees the authored defaults.

### HUD elements

The first HUD layer models `createHudText`, `progressBarHud`, `createInWorldText`, `createProgressBarInWorldText`, `bigMessage`, `smallMessage`, `setObjectiveDescription`, the destroy actions `destroyHudText`, `destroyAllHudText`, `destroyProgressBarHud`, `destroyAllProgressBarHudText`, `destroyInWorldText`, `destroyAllInWorldText`, `destroyProgressBarInWorldText` and `destroyAllProgressBarInWorldText`, and `enableMessages`/`disableMessages`. Any other HUD-related action or value is `not_modeled`.

`hud` holds one entry per element the run created, in creation order — including elements destroyed since, so a destroy never removes an entry:

| Field | Meaning |
| --- | --- |
| `id` | Stable id in creation order, starting at 0. The `lastTextId` value resolves to the `id` of the newest text element — `hudText`, `progressBarHud`, `inWorldText` or `progressBarInWorldText`, the four kinds the catalog's four text creates produce — or `null` before the first one. `getNumberOfTextIds` is the number of live elements of those kinds. |
| `kind` | `hudText`, `progressBarHud`, `inWorldText`, `progressBarInWorldText`, `bigMessage`, `smallMessage` or `objectiveDescription`. |
| `createdTick` | The tick the element was created on. |
| `destroyedTick` | The tick a destroy action removed the element, `null` while it is present. An element counts as present exactly when `destroyedTick` is `null`. |
| `recipients` | The scenario `id`s of the players the element is presented to: the evaluated `Visible To` argument. A recipient group (for example `All Players(Team 1)`) resolves from the declared `team`s. |
| `construct` | A short description of the creating statement, for people, as in `outcome`. Do not match on it. |

The other fields are the evaluated arguments of the creating action; only the arguments the action takes are present. Text and other value arguments serialize as the `Val` form; enum arguments serialize as the catalog member id:

| Field | Kinds | Argument |
| --- | --- | --- |
| `header` | `hudText`, `bigMessage`, `smallMessage` | the header text |
| `subheader` | `hudText` | the subheader text |
| `text` | `hudText`, `progressBarHud`, `inWorldText`, `progressBarInWorldText`, `objectiveDescription` | the text |
| `value` | `progressBarHud`, `progressBarInWorldText` | the bar value |
| `location` | `hudText`, `progressBarHud` | the `HudPosition` member id |
| `position` | `inWorldText`, `progressBarInWorldText` | the evaluated `Vector|Player` position — the model has no geometry |
| `scale` | `inWorldText`, `progressBarInWorldText` | the scale |
| `clipping` | `inWorldText`, `progressBarInWorldText` | the `Clipping` member id |
| `sortOrder` | `hudText`, `progressBarHud` | the sort order |
| `headerColor`, `subheaderColor`, `textColor` | `hudText` | the `Color` member id, or the `Val` form for a non-member |
| `progressBarColor`, `textColor` | `progressBarHud`, `progressBarInWorldText` | the `Color` member id, or the `Val` form likewise |
| `color` | `inWorldText`, `progressBarInWorldText` | the `Color` member id, or the `Val` form likewise |
| `reevaluation` | `hudText`, `progressBarHud`, `inWorldText`, `progressBarInWorldText`, `objectiveDescription` | the reevaluation member id |
| `specVisibility` | `hudText`, `progressBarHud`, `inWorldText`, `progressBarInWorldText` | the `SpecVisibility` member id |

HUD behavior is **modeled** with these limits:

- Arguments are evaluated once, at creation. The `reevaluation` field is recorded but nothing is re-evaluated over time.
- Message display duration and stacking are client display behavior with no capture. `enableMessages`/`disableMessages` resolve their player argument but produce no element and no other trace state.
- The objective description is a singleton per player: a new `objectiveDescription` removes its recipients from every present `objectiveDescription` element, which is destroyed (`destroyedTick` set) when no recipient remains.
- A destroy action affects only elements of its own kind. An unknown, already-destroyed or `null` text id destroys nothing.
- The catalog defines no `Set ...`/`Update ...` action that takes a text or entity id; any id-taking action outside the destroy set is `not_modeled`.

### Effect entities

The effect layer models `createEffect`, `createBeamEffect`, `createIcon`, `createProjectileEffect`, `playEffect`, `destroyEffect`, `destroyAllEffects`, `destroyIcon` and `destroyAllIcons`, recorded like HUD elements but without geometry: the model never represents positions. `effects` holds one entry per entity the run created or played, in creation order — including entities destroyed since:

| Field | Meaning |
| --- | --- |
| `id` | Stable id in creation order, starting at 0. `lastCreatedEntity` resolves to the `id` of the newest durable entity — `effect`, `beam`, `icon` or `projectileEffect` — or `null` before the first one; a played effect has no entity id. `getNumberOfEntityIds` is the number of live `effect`, `beam` and `icon` entities — the kinds the catalog's sentence names; a `projectileEffect` is not counted. |
| `kind` | `effect` (`createEffect`), `beam` (`createBeamEffect`), `icon` (`createIcon`), `projectileEffect` (`createProjectileEffect`) or `playedEffect` (a `playEffect`, present only on its tick). |
| `createdTick` | The tick the entity was created or played on. |
| `destroyedTick` | The tick a destroy action removed the entity, `null` while it is present; a `playedEffect` records `createdTick` here. |
| `recipients` | The scenario `id`s of the players the entity is presented to: the evaluated `Visible To` argument. |
| `construct` | A short description of the creating statement, for people, as in `outcome`. Do not match on it. |

The other fields are the evaluated arguments of the action; only the arguments the action takes are present:

| Field | Kinds | Argument |
| --- | --- | --- |
| `effect` | `effect`, `beam`, `projectileEffect`, `playedEffect` | the `Effect`/`DynamicEffect`, `Beam` or `Projectile` member id of the `Type` argument |
| `icon` | `icon` | the `Icon` member id |
| `color` | `effect`, `beam`, `icon`, `playedEffect` | the `Color` member id, or the `Val` form for a non-member (a `Custom Color`) |
| `position` | `effect`, `icon`, `playedEffect` | the evaluated `Position` argument as a `Val`: a player yields `{ "type": "player", "value": "<id>" }` |
| `radius` | `effect`, `playedEffect` | the radius |
| `startPosition`, `endPosition` | `beam` | the beam's two positions |
| `direction`, `oversize` | `projectileEffect` | the direction and scale |
| `friendlyTo` | `projectileEffect` | the scenario `id`s the entity appears friendly to |
| `showWhenOffscreen` | `icon` | whether the icon renders off-screen |
| `reevaluation` | `effect`, `beam`, `icon`, `projectileEffect` | the reevaluation member id |

`entityExists` answers the catalog's `Player|EntityId`: a scenario player exists for the whole run (the roster never changes); a number answers whether the entity table holds a live entity of that id — a text id is just a number here, so the answer follows whatever live entity holds it — and `Null` names nothing. Any other value is `not_modeled`.

A position expression that needs unmodeled geometry stops the run as `not_modeled` before the entity is recorded; under the empty-world model `Nearest Walkable Position` and the raycasts evaluate instead (see [Geometry (empty world)](#geometry-empty-world)). No capture backs this model of effects at all, so the whole layer is **modeled, not measured**. `destroyEffect` and `destroyAllEffects` target `effect` and `beam` entities — the catalog names both create actions in the destroy sentence; `destroyIcon` and `destroyAllIcons` target `icon` entities. An unknown, already-destroyed or `null` entity id destroys nothing. A `projectileEffect` is durable but no destroy action names it, and a played effect is already destroyed on its tick.

### Actions with no readable effect

Actions the catalog ties to no readable value run as no-ops: arguments still evaluate, and nothing is written or recorded (athena#103). The set is enumerated in [findings.md](findings.md); **modeled, not measured**.

### Statuses, buttons and started rules

- `Set Status(player, assister, status, duration)` records the status on each target's `statuses` with an expiry of `now + duration`; the `assister` resolves like `Damage`'s dealer — a player or `Null`. `Has Status(player, status)` reads it as held while the expiry is ahead. `Clear Status` removes the entry. Setting, clearing or reading a status on a player whose `world` did not resolve is a world-state touch and stops the run as `not_modeled`.
- `Allow Button`/`Disallow Button` write the target's `buttons` map per slot. `Is Button Held(player, button)` reads a live input: the scenario player's `buttons` map supplies the held state per slot, and a slot not listed stops the run as `missing_input` of kind `button`. Whether `Disallow Button` changes a held read has no capture; the read is the scenario's supplied state.
- `Start Rule(subroutine, ifAlreadyExecuting)` launches the subroutine's own rule instance, applied when the tick loop visits that instance: a subroutine listed later in the program starts on the same tick, an earlier one on the next tick. `Restart Rule` restarts an instance already running or waiting; `Do Nothing` leaves it alone. A started subroutine runs until it finishes or waits; a `Wait` inside resumes on its wake tick. A subroutine rule has no conditions, so `Abort When False`/`Restart When True` do not apply inside it; a `Wait Until` still re-checks its own condition each tick.
- `Stop Chasing Global/Player Variable` does nothing: the `Chase *` actions are not modeled, so a run never has a chase in progress. The action still resolves its player argument.

### State-cell writes and stored modifiers

Two action classes write per-player state directly (athena#116), each modeled — not measured — from its catalog sentence:

- **State-cell writes.** A write lands on a `state` key of the player — a cell over the declared `state` map — even on a key the scenario never supplied; a read after the write returns the written value, and a read before any write of an undeclared key is the `missing_input` of decision 19. Every write records a `state` change naming the player and the key, and the write alone changes the cell — no damage, healing, movement or physics follows. The set: `Set Ultimate Charge` writes `ultCharge` (`False`/`True` coerce to `0`/`1`); `Set Player Health` writes `health`, gated by the catalog's "only has an effect on living players" — it reads `isDead` like a program read and skips a dead target, while a scenario that cannot answer `isDead` stops `missing_input`; `Set Max Health` writes `maxHealth` as a percentage of the current max and clamps `health` to the new max ("will ensure that a player's current health will not exceed the new max health"), reading the current values the same way — cell, declared `state` or the pools fallback; `Kill` writes `isDead` true, `Respawn` writes `isDead` false and `health` to the `maxHealth` read — the relocation to a spawn point is a gameplay effect the model does not simulate — and `Resurrect` writes `isDead` false. `Set Facing(player, direction, To World)` writes the scenario `facing` to the direction's angles, like `Teleport` writes `position`; `To Player` is `not_modeled` — the catalog does not state the composition.
- **Stored modifiers.** An action whose catalog sentence stores a fact no modeled read answers records a `modifier` change — the action id, the player `id` and the evaluated non-target arguments — and stores it on the player's `modifiers` row under a key named for it; nothing reads it back and no `state` or `world` value moves. The set, each with its key: `Set Ultimate Ability Enabled` → `ultEnabled`, `Set Ability 1/2 Enabled` → `ability1Enabled`/`ability2Enabled`, `Set Damage Dealt`/`Set Damage Received`/`Set Healing Dealt`/`Set Healing Received` → `damageDealt`/`damageReceived`/`healingDealt`/`healingReceived`, `Set Move Speed` → `moveSpeed`, `Set Gravity` → `gravity`, `Set Knockback Received` → `knockbackReceived`, `Set Player Allowed Heroes` → `allowedHeroes`, `Start Forcing Player To Be Hero` → `forcedHero` (`Stop Forcing Player To Be Hero` removes it), and `Start Forcing Spawn Room` → `forcedSpawnRoom` on every player of the team.


## Fixtures

`crates/athena-sim/tests/fixtures.rs` reads every probe through `run_text`, requires `outcome` to be `complete`, and compares `logs` and `globals` with the captures.
