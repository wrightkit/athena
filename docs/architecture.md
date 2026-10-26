# Architecture

## Parts

```text
probes/<id>/        probe programs and captures        (this repository, exists)
docs/               goal, conventions, procedure       (this repository, exists)
scheduler simulator Rust crate, reproduces captures    (not built; lives in this repository)
```

The simulator is outside the semantic crate. It takes a Workshop program as parsed by `workshop-rs`, runs a tick loop, and emits a trace that tests compare with a capture's `observed`.

## Ownership

| Concern | Owner |
| --- | --- |
| Probe programs, captures, capture convention, verdict rules | this repository |
| Workshop semantics, catalog, parsing, validation, emission | `workshop-rs` |
| Scheduler simulator | this repository (see [placement.md](placement.md)) |
| `wright check` / `wright inspect cost` used to validate probes | `wright` |
| Orchestration or analysis that would consume the simulator | `wright` |

## Dependency direction

Consumer to `workshop-rs`, never the reverse:

```text
athena simulator ──► workshop-rs
opy-rs, deltin-rs tests ──► athena (dev-dependency, not yet agreed)
```

`workshop-rs` takes no dependency on this project. A confirmed behavior reaches `workshop-rs` only through a request to its owners (an Issue), not by editing it from here. Only behavior that is a declarative Workshop fact is a candidate for catalog metadata; scheduling rules are behavior and belong in typed code (`workshop-rs` core boundaries). Nothing is promoted from a result with fewer than three agreeing runs.

## Simulator input

The simulator should consume the public `Program` model of `workshop-rs`. The arena-backed WIR is `pub(crate)` there and described as an implementation detail, not a public contract. If the simulator ever needs WIR itself, that is a request to `workshop-rs` to expose it ([placement.md](placement.md#still-open)).

## Simulator scope (first layer)

Only what the P0–P7 fixtures need:

- tick loop with a 16 ms tick;
- rule instances, Ongoing edge triggering, Wait rounded up to ticks;
- Wait Until and the three Wait Behaviors (P4, P6);
- global variables and arrays;
- output: a tick-ordered rule-firing trace and `Log To Inspector` output.

Also included since athena#9: the player state table, the `Damage`/`Heal` actions, and the state-reading values `Health`, `Max Health` and `Ability Cooldown` with `Set Ability Cooldown` (modeled, see below).

Not included: hit detection, positions, values that depend on the world such as `Position Of`, ability effects, and the custom game settings outside the `heroes` scalars the player state table applies. An unimplemented action or value is reported as not modeled in the trace, never skipped ([simulator-interface.md](simulator-interface.md)).

Acceptance: for every probe with result `match`, `sim(probe.workshop)` equals the capture's `observed`. For `mismatch`, the capture wins and the simulator follows it. For `inconclusive`, the test is an explicit unasserted case.

Load figures are relative counts of evaluations, never real server cost.

## Choices no capture covers yet

The simulator makes these choices only to stay deterministic; each needs a probe before it counts as measured:

- A rule's condition is tracked every tick, also while its run is suspended; an edge during a Wait is dropped (P2). A rising edge on the exact tick a Wait ends is treated as dropped too.
- Rules run in file order within a tick; several runs waking on the same tick resume in file order.
- An injected scenario event fires on tick `floor(time / 16 ms)`. Event-triggered rules run in file order interleaved with that tick's other rules; several events for one rule queue in file order.
- A variable that is not set reads as `Null`, including a variable that a scenario does not declare ([scenario-format.md](scenario-format.md)).
- Comparing values of different types with `==` gives false; `Null` equals `0`. Two mocked players compare by scenario `id`.
- A mocked player is a scenario `id`; `Event Player` yields `{"type":"player"}`. `Hero Of` yields the catalog member id the declared `hero` resolves to; `Team Of` likewise yields the `Team` member id of the declared `team`, so it compares equal to a `Team` literal.
- An event rule fires when the injected event's `kind` matches the rule event's catalog id and the event player satisfies the rule's `team` and player (`target`) filters.
- **Modeled, not measured** — no client capture backs the player state table or the `Damage`/`Heal` rules:
  - Each scenario player has a state-table row built at run start from the `workshop-rs` hero-values file pinned at `CURRENT_VERSION`: a `health` pool, `shield` and `armor` pools where the catalog records them (a hero without the pool has no entry, never a zero), the cooldown seconds of the ability slots the catalog records (`slot` or `slot/variant` keys), and an ultimate charge of `0` toward the catalog's `ultimateCost`. Pools start at their maximum. The program's hero settings then apply: `health%` scales every pool, the `abilityCooldown%` and per-slot `*Cooldown%` scalars scale the reported cooldowns, and the dealt/received scalars feed `Damage`/`Heal` below. Percent scalars authored at several scopes multiply together. A hero the file does not supply, a player with no `hero`, or a hero setting authored for the player that the simulator does not apply (one that moves a reported value — ultimate generation, spawning with ultimate, passive health regeneration or the role passive, headshots-only, spawning without mech, or a cooldown scalar with no reported slot) leaves the row's `world` null; an action that touches that world state (`Damage`, `Heal`, dealt or received) stops the run as `not_modeled`. The exception is a player whose version-2 `state` map declares every `state` key the setting can move — `ultCharge` for the ultimate-charge settings (`ultGen%`, `passiveUltGen%`, `combatUltGen%`, `enableSpawningWithUlt`), `health`, `maxHealth` and `normalizedHealth` for the health ones (`passiveHealthRegen`, `enableRolePassive`): the scenario already supplies the value the setting could move, so the setting is skipped — `state.<key>` stands, the setting itself is not simulated (athena#91). The other unapplied settings have no `state` coverage and keep failing, and an opaque `Raw*` member fails the same way.
  - `Health`/`Max Health` read the pools' summed `current`/`max` only while the player declares no version-2 `state` map — a declared `state.health`/`state.maxHealth` answers instead (athena#89); `Ability Cooldown` reads the slot's reported seconds (`0` where the player has no such slot); `Set Ability Cooldown` writes the slot's reported seconds literally — whether `abilityCooldown%` also scales a scripted set is what probe P7 is for. Reading a player's world state is a touch like `Damage`/`Heal`.
  - `Damage(player, damager, amount)` takes `amount` off the pools in shield, armor, health order: each pool absorbs up to its `current` and the rest spills into the next; health floors at zero and a negative amount does nothing. `Heal(player, healer, amount)` raises the `health` pool by `amount`, capped at `max` — it never exceeds the maximum, never refills shield or armor, and a negative amount does nothing. Both accept a `Player` or an array of players. The amount lands scaled: by the dealer's `damageDealt%` and the target's `damageReceived%` for `Damage`, and by the healer's `healingDealt%` and the target's `healingReceived%` for `Heal`; a `Null` dealer or healer carries no scale. A `damager`/`healer` value that is not a player or `Null` is not modeled.
- **Modeled, not measured** — spawn state and the host player. No capture covers them:
  - `Has Spawned` of a scenario player with a `hero` is true from tick 0; `Is Alive` answers the `isDead` read — a state cell an action wrote or the declared `state.isDead` — when one answers, and the same hero model otherwise (the catalog names the two reads complements). Other respawn and death dynamics are not modeled. A player without a `hero` stops `Has Spawned` — and `Is Alive` when `isDead` cannot answer — as `not_modeled`.
  - `Host Player` returns the scenario player marked `"host": true`, a rule the project owner decided; the first player is not an implicit host. Without a marked player the run stops as `not_modeled`; two marked players fail the load.
  - `Wait Until(condition, timeout)` re-checks the condition on every tick after the one it starts on, and continues on the first tick it holds; P4 measures only constant conditions. `Wait Until(false, 0)` is `not_modeled`.
  - `Abort When False` and `Restart When True` apply only to ongoing rules (`Ongoing - Global` and `Ongoing - Each Player` instances); `Wait Until` also re-checks inside a started subroutine (see `Start Rule` below). In a rule triggered by an injected event the three stop the run as `not_modeled`. A restart reruns the rule from its first action (P6: `nRestart` counts both starts). When a wake and a behavior trigger fall on one tick, the wake wins.
- **Modeled, not measured** — `For Global Variable`/`For Player Variable` execution; no capture covers the loop:
  - A `For` evaluates `start`, `stop` and `step`, writes the control variable with `start`, and exits before the body when the variable already reached or passed `stop` — "passed" follows `step`'s sign (a non-negative step exits at `>=`, a negative one at `<=`). Its `End` evaluates `stop` and `step`, adds `step` to the variable, and re-checks; a continuing loop jumps to the action after the `For`. A `step` of `0` on a range the start has not passed cannot terminate and ends at the per-run step limit instead.
- **Modeled, not measured** — the control-flow actions (athena#58); no capture covers them:
  - `Skip`/`Skip If` jump `count` actions ahead, counting from the action after the jump: a positional jump, so it can land inside a block the run never entered and the actions there execute raw. The count takes the catalog's `Number|Boolean` coercion — `True`/`False` read as 1/0 — and a negative count skips nothing.
  - `Abort`/`Abort If` end the whole run, frames of a `Call Subroutine` caller included; the timeline records the run `aborted`, not `finished`.
  - `Loop`, `Loop If`, `Loop If Condition Is True` and `Loop If Condition Is False` restart the action list the frame executes — inside `Call Subroutine`, the subroutine's own list — and the `Condition Is True/False` forms re-check the rule's condition list per pass with the run's bindings (`Event Player`, injected event). A restart that never suspends ends at the per-run step limit.
  - `Break`/`Continue` work inside `While`/`For` blocks, through an intervening `If`: `Continue` lands on the innermost loop's `End` (which steps a `For` or re-enters a `While` head), `Break` lands past it. Outside a loop they are `not_modeled`.
- **Modeled, not measured** — rule event `team`/`target` filters and `Ongoing - Each Player` instantiation; no client capture covers them:
  - `team` is `All`, `Team 1` or `Team 2` against the declared `team` (a player with no `team` is in `All` but no `Team 1`/`Team 2`, as for HUD recipient groups); `target` is `All` or a hero name, matched against the catalog `Hero` member the declared `hero` resolves to. `Slot N` matches the declared `slot` (a player without one does not match), and a `Slot` filter no evaluation reaches never fires and is not an error.
  - An `Ongoing - Each Player` rule becomes one instance per matching scenario player at load, ordered by scenario `id`. A filter matching nobody produces no instance: the rule never fires and is not an error. Each instance runs the ongoing-rule logic with `Event Player` bound to its player, including inside a subroutine it calls; the instance order of several instances of one rule on the same tick has no measured order.
  - A player event rule (`Player Died`, `Player Joined Match`, …) runs for an injected event only when the event's `eventPlayer` satisfies the filters; an event that names no `eventPlayer` satisfies only `All`/`All`, and evaluating a filtered rule against it reads the event player — a `missing_input` of kind `eventPlayer`.
- **Modeled, not measured** — the general and array values of athena#56 (`If-Then-Else`, `Compare`, `Not`, `Or`, `Divide`, `Round To Integer`, `First Of`, `Count Of`, `Array Contains`, `Index Of Array Value`, `Array Slice`, `Append To Array`, `Empty Array`), the `Set`/`Modify * Variable At Index` actions and the full `Modify` operator set; no capture covers them:
  - `If-Then-Else`, `And`, `Or` and `Not` read truthiness: a boolean as itself, a number or `Null` (as `0`) by nonzero-ness. `And`/`Or` short-circuit; `If-Then-Else` reads only the taken branch.
  - `==`/`!=` compare arrays element-wise, numbers/booleans/`Null` by number coercion (`Null` and `False` equal `0`), text by text, players by scenario `id`; different types are never equal. The ordering operators read numbers only. An enum literal in a value position yields its catalog member id as text. A `display` value (athena#113) is different again: two of them answer true only on identical producer and recursively-equal arguments, and any other pairing — including one against a literal — stops as `not_modeled` because equality would read the client token's content.
  - `First Of`, `Count Of`, `Array Contains`, `Index Of Array Value`, `Array Slice` and `Append To Array` read `Null` as the empty array; `Index Of Array Value` reports `-1` on a miss. `Array Slice` clamps its bounds to the array.
  - A `Set * At Index` whose index is past the end appends the value; a negative index writes nothing. A `Modify * At Index` whose index is outside the array writes nothing. `Remove From Array By Value` drops every element equal to the operand.
- **Modeled, not measured** — the element-context array values of athena#60; no capture covers them:
  - `Filtered Array`, `Sorted Array`, `Is True For All` and `Is True For Any` read the array once — `Null` as the empty array — then read the operand once per element with `Current Array Element` and `Current Array Index` bound to the element and its place; a nested element-context value rebinds the pair for its own operand. `Current Array Element`/`Current Array Index` outside an element context are `not_modeled`.
  - `Sorted Array` orders ascending by the per-element key under the number coercion — a key that is not a number, boolean or `Null` is `not_modeled` — and an equal key keeps the element's place. `Is True For All`/`Is True For Any` stop at the deciding element: the first false, the first true; the empty array is true for `All`, false for `Any`.
  - `Evaluate Once` pins its operand's first evaluated value, keyed by the site in the program, for the rest of one scope. A rule-instance run is a scope: a `For`/`While`/`Loop` body or a `Loop`-restarted action list re-reads the site and gets the pin, a `Wait` suspension keeps the pins for the resumed run, and the instance's next run start or restart clears them. A rule-condition check — the scheduler's per-tick check or a `Loop If Condition Is True`/`False` mid-run — is a fresh scope per pass, so a site in the conditions re-reads every check. A `Wait Until` call is a scope its per-tick condition re-checks share; reaching the action again clears it. Inside an element-context operand the site is re-read once per element with `Current Array Element`/`Current Array Index` rebound; whether the client's pin survives that is unmeasured, so `Evaluate Once` there is `not_modeled`. `Mapped Array` is not modeled.
- **Modeled, not measured** — `Workshop Setting *` values; no capture covers the read:
  - The simulator runs one lobby whose settings nobody edits, so `Workshop Setting Real`/`Integer`/`Toggle`/`Combo`/`Hero` evaluates to its declared `default` argument; `min`/`max`, `options` and `sortOrder` are authoring metadata. A `Hero` default yields its `Hero` literal's catalog member id as text — the member id `Hero Of` yields, so the two compare equal. The scenario format has no place for setting overrides.
- HUD recipient groups resolve from the declared `team`s: a player without a `team` is in `All Teams` but in no `Team 1`/`Team 2`, and a `Null` `Visible To` resolves to no recipients.
- `Last Text ID` yields the `id` of the newest element a destroy action can target (`hudText`, `progressBarHud`) or `null`; a destroy affects only its own kind and ignores an unknown, already-destroyed or `null` id.
- A new `setObjectiveDescription` replaces earlier objective descriptions per recipient; an element left with no recipient gets `destroyedTick`.
- HUD arguments are evaluated once at creation; the recorded `reevaluation` does not re-evaluate. Message display duration and stacking are excluded display behavior, so `enableMessages`/`disableMessages` only resolve their player argument.
- **Modeled, not measured** — statuses, buttons, effects and `Start Rule` (the rest of athena#58); no capture covers them:
  - `Set Status`/`Clear Status`/`Has Status` keep per-player expiries on `Total Time Elapsed`: a status is held while `now < expiry`. Setting, clearing or reading a status on a player whose `world` did not resolve is a world-state touch and stops the run as `not_modeled`. The `assister` argument resolves like `Damage`'s dealer — a player or `Null` — and is otherwise dropped.
  - `Allow`/`Disallow Button` record per-slot allowed flags on the player's world; they do not feed `Is Button Held` — whether a disallowed button still reads held has no capture. `Is Button Held` is a live input: the scenario player's `buttons` map supplies the held state per slot, and a slot not listed is a `missing_input` of kind `button`.
  - `Start Rule` launches the subroutine's own rule instance; the launch is applied when the tick loop visits that instance, so a later-listed subroutine starts on the same tick and an earlier one on the next. `Restart Rule` vs `Do Nothing` distinguishes an already-running or already-queued instance from an idle one. A started subroutine has no conditions: `Abort When False`/`Restart When True` do not apply inside it; a `Wait Until` still re-checks its own condition each tick.
  - `Create Effect`/`Play Effect`/`Destroy Effect` record entities like HUD elements (id, recipients, evaluated arguments, `destroyedTick`) but with no geometry: the `position` argument is recorded as evaluated, and a position expression that needs unmodeled geometry stops the run. `Last Created Entity` yields the newest `effect` entity's id; `playedEffect` entities have no id a destroy can target. `Destroy Effect` on an unknown, dead or `null` id destroys nothing.
  - `Stop Chasing Global/Player Variable` is a no-op — `Chase *` is not modeled, so nothing is ever chasing. The player argument still resolves.
- **Modeled, not measured** — `All Heroes` (athena#69); no capture covers the element order:
  - `All Heroes` yields the catalog's `Hero` members in catalog order, as the member-id texts `Hero Of` and a `Hero` literal yield, so an element compares equal to both. The client's element order is unmeasured; the catalog order is the modeled order.
- **Modeled, not measured** — `Current Map` (athena#71); no capture covers the lobby's map choice:
  - `Current Map` reads the scenario's `map`, resolved like `hero` to a catalog `Map` member id, and yields that member id as text — the form a `Map` literal yields, so the two compare equal. The map is a scenario constant; a mid-run change is not modeled. A read without `map` is a `missing_input`; other map-derived values stay `not_modeled` unless they read the same field and the catalog.
- **Modeled, not measured** — `Objective Position` (athena#82); no capture covers the positions a map and mode give the objectives:
  - `Objective Position(i)` reads `objectives[i]` of the scenario — the map and mode place the objectives outside the script, so the list is a constant input and objective movement is not modeled. The index is the evaluated argument under the number coercion (`False`/`True` as 0/1), the catalog's index "starting at 0 and counting up". A read with no `objectives` field or an index naming no entry is a `missing_input` of kind `objectives` named `objectives[i]`; other objective-derived values stay `not_modeled`.
- **Modeled, not measured** — `Is Hero Being Played` (athena#84); no capture covers the hero a player plays:
  - `Is Hero Being Played(hero, team)` reads the scenario's declared `hero` and is true when at least one player on `team` has it — `All Teams` selects every player; the hero-selection screen is outside the model. The arguments resolve to catalog member ids, the form `Hero Of`/`Team Of` yield — an argument that is not a `Hero`/`Team` member after evaluation is `not_modeled` naming the parameter position — and a player on the team without a `hero` is a `missing_input` of kind `hero`, the same read `Hero Of` makes.
- **Modeled, not measured** — `Players On Hero` (athena#86); no capture covers the hero a player plays:
  - `Players On Hero(hero, team)` reads the scenario's declared `hero` and yields the players on `team` that have it — `All Teams` selects every player — in roster order; the hero-selection screen is outside the model. The arguments resolve to catalog member ids, the form `Hero Of`/`Team Of` yield — an argument that is not a `Hero`/`Team` member after evaluation is `not_modeled` naming the parameter position — and a player on the team without a `hero` is a `missing_input` of kind `hero`, the same read `Hero Of` makes.
- **Modeled, not measured** — `Value In Array` on a non-array operand (athena#97); no capture covers it:
  - `Value In Array` yields the element at the index, or `Null` when no element exists there — an out-of-range or negative index, or a non-array operand (`Null`, number, text, vector, player, boolean): a non-array has no element at any index, so the catalog's "element does not exist" answer — the same `Null` the out-of-range case yields — applies. `First Of` of `Null` and of the empty array answer the same way (both `Null`).
- **Modeled, not measured** — `Null` in a `Vector` position (athena#108); no capture covers it:
  - An evaluated `Null` where a `Vector` operand is expected is the zero vector — an unset variable reads `Null`, and the coercion follows the model's other default-zero reads (`Null` is `0` to the number coercion, the empty array to array positions), the catalog's `zeroAsNull`/`nullVectorAsNull` equivalence between `0`, `Null` and `Vector(0, 0, 0)`, and the read `Teleport`'s position already made of `Null`. The rule covers every strict `Vector` position the model answers, so the zero-length guards still stop (`Normalize`, `Direction Towards`, `Angle Between Vectors` on `Null` are `not_modeled`); a literal number, boolean, text or array stays `not_modeled`.
- **Empty world, modeled not measured** — the geometry reads (athena#77, the `athena#75` decision); no capture covers them and the goal excludes real geometry:
  - The run has no map geometry, and the trace marks it as `geometry: "emptyWorld"` so the viewer can warn that geometry-dependent results are not real. A raycast never hits and answers by the catalog's documented no-hit return: `Ray Cast Hit Position` the end position, `Ray Cast Hit Player` `Null`, `Ray Cast Hit Normal` the direction from the end back to the start — a zero-length cast is `not_modeled`. `Is In Line of Sight` is true; `Nearest Walkable Position` yields its input; `Is On Ground` is true and `Is In Air`/`Is On Wall`/`Is In Spawn Room` are false — an empty world has none of what they ask about. `Is In Spawn Room` is also a version-2 `state` read: a player that declares `state` answers `state.isInSpawnRoom` instead of the empty world's `false` (athena#89).
  - `Teleport` writes the player's scenario `position` to the evaluated destination (`Null` is the origin, the catalog's `zeroAsNull` coercion) and records a `world` change with `field` `position`; `Position Of` reads the new value. `Apply Impulse` changes velocity, which needs physics, and stays `not_modeled`, as does `Is Standing`. `Is Moving` and `Eye Position` are version-2 `state` reads instead (athena#89): a player that declares `state` answers `state.isMoving`/`state.eyePosition`, and a player without `state` reports a `missing_input`.
