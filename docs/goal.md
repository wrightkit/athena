# Goal

## Purpose

Build a code-level model of how Overwatch Workshop rules are scheduled and executed, grounded in public information and tests against the real client. The model is a scheduler simulator; this repository holds the measurements it is checked against.

Measured behavior is recorded as captures. Captures become test fixtures: for every probe whose result is `match`, the simulator output must equal the recorded observation. Where the client disagrees with a hypothesis, the client wins and the simulator follows it.

## Intended consumers

The owner's intent: a verified, logic-level Workshop runtime that checks the high-level layers (`workshop-rs`, OverPy, OSTW) against expected behavior and backs the `wright` UI with execution simulation. It is a shared tool for those layers, not a feature of any one of them. The UI itself lives in `wright`; this repository holds no UI.

## Method

1. Write a hypothesis about rule scheduling (timing, ordering, triggering).
2. Express it as a small Workshop program, a probe.
3. A human pastes the probe into the client; the whole Inspector text is saved verbatim as a capture.
4. A JSON record states the hypothesis, the observed values, the run count, the probe hash, and a result of `match`, `mismatch` or `inconclusive`.
5. Three agreeing runs make a result final. There are no evidence classes.

Details: [capture-convention.md](capture-convention.md), [procedure.zh-CN.md](procedure.zh-CN.md).

## Scope

In: tick length, Wait rounding and Wait Until, Ongoing edge triggering, same-tick rule ordering, Wait Behavior, the value-refresh behavior needed to interpret them, a player state table (hero, team, health pools, ability cooldowns, ultimate charge), mocked events supplied by the user, and the effect of custom game settings on those values. The simulator exposes this as headless HUD state (hero and health, ability set and cooldowns); `wright` renders it.

Out: physics, geometry, hit detection, and what an ability does in the world. Abilities and combat reach the simulator as mocked events with explicit values. Results hold under the supplied inputs; inputs the user did not supply are reported as missing, not defaulted silently.

Hero base values (health, cooldowns, ability parameters) come from a balance-versioned catalog owned by `workshop-rs`, and the simulator reads it by version; rolling back selects an earlier version's data only, never earlier scheduler behavior. Any load figure the model produces is a relative count of evaluations, never real server cost.

## Relation to WrightKit

This supports the tooling goals in the organization goal (`docs/goal.md` in `wrightkit/.github`), mainly principle 9 (verified technical facts about the Workshop environment) and server-load evaluation under principle 5. The workspace goal keeps a game-world simulator out of scope and names `athena` as the owner of Workshop execution modelling.

Workshop semantics belong to `workshop-rs`. A confirmed behavior may later inform catalog metadata there; this repository does not add it.
