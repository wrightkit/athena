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

Only what P0 to P4 fixtures need:

- tick loop with a 16 ms tick;
- rule instances, Ongoing edge triggering, Wait rounded up to ticks;
- Wait Until and the three Wait Behaviors;
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
- A mocked player is a scenario `id`; `Event Player` yields `{"type":"player"}`. `Hero Of` and `Team Of` yield the declared `hero`/`team` as text.
- An event rule fires when the injected event's `kind` matches the rule event's catalog id; a team or player filter other than `All` is not modeled.
- **Modeled, not measured** — no client capture backs the player state table or the `Damage`/`Heal` rules:
  - Each scenario player has a state-table row built at run start from the `workshop-rs` hero-values file pinned at `CURRENT_VERSION`: a `health` pool, `shield` and `armor` pools where the catalog records them (a hero without the pool has no entry, never a zero), the cooldown seconds of the ability slots the catalog records (`slot` or `slot/variant` keys), and an ultimate charge of `0` toward the catalog's `ultimateCost`. Pools start at their maximum. The program's hero settings then apply: `health%` scales every pool, the `abilityCooldown%` and per-slot `*Cooldown%` scalars scale the reported cooldowns, and the dealt/received scalars feed `Damage`/`Heal` below. Percent scalars authored at several scopes multiply together. A hero the file does not supply, a player with no `hero`, or a hero setting authored for the player that the simulator does not apply (one that moves a reported value — ultimate generation, spawning with ultimate, passive health regeneration or the role passive, headshots-only, spawning without mech, or a cooldown scalar with no reported slot) leaves the row's `world` null; an action that touches that world state (`Damage`, `Heal`, dealt or received) stops the run as `not_modeled`.
  - `Health`/`Max Health` read the pools' summed `current`/`max`; `Ability Cooldown` reads the slot's reported seconds (`0` where the player has no such slot); `Set Ability Cooldown` writes the slot's reported seconds literally — whether `abilityCooldown%` also scales a scripted set is what probe P7 is for. Reading a player's world state is a touch like `Damage`/`Heal`.
  - `Damage(player, damager, amount)` takes `amount` off the pools in shield, armor, health order: each pool absorbs up to its `current` and the rest spills into the next; health floors at zero and a negative amount does nothing. `Heal(player, healer, amount)` raises the `health` pool by `amount`, capped at `max` — it never exceeds the maximum, never refills shield or armor, and a negative amount does nothing. Both accept a `Player` or an array of players. The amount lands scaled: by the dealer's `damageDealt%` and the target's `damageReceived%` for `Damage`, and by the healer's `healingDealt%` and the target's `healingReceived%` for `Heal`; a `Null` dealer or healer carries no scale. A `damager`/`healer` value that is not a player or `Null` is not modeled.
- HUD recipient groups resolve from the declared `team`s: a player without a `team` is in `All Teams` but in no `Team 1`/`Team 2`, and a `Null` `Visible To` resolves to no recipients.
- `Last Text ID` yields the `id` of the newest element a destroy action can target (`hudText`, `progressBarHud`) or `null`; a destroy affects only its own kind and ignores an unknown, already-destroyed or `null` id.
- A new `setObjectiveDescription` replaces earlier objective descriptions per recipient; an element left with no recipient gets `destroyedTick`.
- HUD arguments are evaluated once at creation; the recorded `reevaluation` does not re-evaluate. Message display duration and stacking are excluded display behavior, so `enableMessages`/`disableMessages` only resolve their player argument.
