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

Not included: player world, damage, hit detection, values that depend on the world such as `Position Of`. An unimplemented action or value is reported as not modeled in the trace, never skipped ([simulator-interface.md](simulator-interface.md)).

Acceptance: for every probe with result `match`, `sim(probe.workshop)` equals the capture's `observed`. For `mismatch`, the capture wins and the simulator follows it. For `inconclusive`, the test is an explicit unasserted case.

Load figures are relative counts of evaluations, never real server cost.

## Choices no capture covers yet

The simulator makes these choices only to stay deterministic; each needs a probe before it counts as measured:

- A rule's condition is tracked every tick, also while its run is suspended; an edge during a Wait is dropped (P2). A rising edge on the exact tick a Wait ends is treated as dropped too.
- Rules run in file order within a tick; several runs waking on the same tick resume in file order.
- Comparing values of different types with `==` gives false; `Null` equals `0`.
