# Placement

Decision (2026-10-19, project owner): `runtime` is a **standalone project** that depends on `workshop-rs`. It is not merged into `workshop-rs`. The scheduler simulator, when built, is a crate in this repository.

## Why

- The simulator is meant to check several layers: `workshop-rs`, OverPy and OSTW. A checking tool placed inside one of them would share its origin with what it checks and would make the other layers depend on it. **[judgment]**
- `workshop-rs` prescribes consumer-to-`workshop-rs` dependency and owns Workshop semantics, not runtime scheduling (`workshop-rs/AGENTS.md`, `docs/architecture/core-boundaries.md`). **[verified]**
- `workshop-rs` is two crates in one release-plz version group, and its CI is built around them. Adding a simulator crate there would put it in that cadence, while probe runs need a human and a client. **[verified]** for the release setup, **[judgment]** for the cadence mismatch.
- Rejected: merging into `workshop-rs` as `tools/runtime-probes` plus a simulator crate; keeping this repo as data only with the simulator elsewhere (the simulator would have no named owner and fixtures would sit in a different repository from the code that asserts them). **[judgment]**

## Consequences

- Dependency direction: `runtime` to `workshop-rs`; OverPy and OSTW tests may depend on `runtime` as a dev-dependency. `workshop-rs` takes no dependency on this project. **[judgment]** for the OverPy/OSTW part, not yet agreed with those repositories.
- Captures stay in `probes/` and are the fixtures; simulator tests in this repository read them by path.
- Confirmed behavior reaches `workshop-rs` only through an Issue there. Declarative facts (for example the tick length) are candidates for catalog data; rule ordering and edge triggering are behavior, which its boundaries place in typed code. **[judgment]**
- Input is the public `Program` model. WIR is `pub(crate)` in `workshop-rs` and not a public contract. **[verified]** Whether WIR is ever needed is open.

## Still open

1. The workspace routing table in `wrightkit/.github` has no row for this project; ownership needs an Architect-approved row. **[verified]** that none exists.
2. The workspace goal lists "a game runtime simulator" as a non-goal. The owner set this aside until feasibility is established; it must be resolved before the simulator is published or routed to.
3. WIR versus `Program` as simulator input; start with `Program`.
4. A remote and CI that runs `wright check` on probes and compares `probe_sha256`; none exists, and no script does it today.
5. Whether `opy-rs` and `deltin-rs` want to consume the simulator.
6. Whether the Chinese procedure is translated to English.
