# Lessons

Short review queue of reusable failure classes from merged athena work. Each entry names a prevention action and the evidence behind it. Promote an entry into a regression test, an owner contract, or an existing engineering standard, then remove it here. The procedure is the `merged-pr-retro` skill in `wrightkit/.agents`.

### A behavior change updates every document that states the behavior
- **Rule:** When a PR changes what the simulator does, search `architecture.md`, `simulator-interface.md`, `scenario-format.md`, `trace-format.md`, `findings.md`, and doc comments for the old claim and fix each one in the same commit.
- **Why:** The same claim is restated in several documents. Review rounds kept finding the copy the fix missed, so agents and readers saw contradicting contracts.
- **Prevention home:** owner contract — decide in [athena#126](https://github.com/wrightkit/athena/issues/126) whether each claim gets one home document that the others link to.
- **Incidents:** [athena#49](https://github.com/wrightkit/athena/pull/49), [athena#63](https://github.com/wrightkit/athena/pull/63), [athena#78](https://github.com/wrightkit/athena/pull/78), [athena#90](https://github.com/wrightkit/athena/pull/90), [athena#92](https://github.com/wrightkit/athena/pull/92), [athena#109](https://github.com/wrightkit/athena/pull/109), [athena#117](https://github.com/wrightkit/athena/pull/117) — observed 2026-10-25, 2026-10-26

### An optional scenario object rejects an explicit `null`
- **Rule:** Every optional object field in the scenario format uses `object_or_absent`, so `null` is malformed rather than treated as absent, and the schema says the same.
- **Why:** serde's default reads `null` as a missing field, so `"random": null`, `"position": null`, and `"state": null` loaded silently instead of failing.
- **Prevention home:** test — one loader test that sends `null` for every optional object field the schema declares.
- **Incidents:** [athena#66](https://github.com/wrightkit/athena/pull/66), [athena#76](https://github.com/wrightkit/athena/pull/76), [athena#90](https://github.com/wrightkit/athena/pull/90) — observed 2026-10-26

### A value read yields the catalog member id, not the scenario spelling
- **Rule:** A read that returns an enum value (`Hero Of`, `Team Of`, and the like) produces the same catalog member id that the enum literal evaluates to.
- **Why:** `Hero Of` returned the scenario's `"Ana"` and `Team Of` returned `team1`, so `Compare` with a `Hero(Ana)` or `Team 1` literal answered false.
- **Prevention home:** test — compare each enum-valued read against its literal.
- **Incidents:** [athena#55](https://github.com/wrightkit/athena/pull/55), [athena#61](https://github.com/wrightkit/athena/pull/61) — observed 2026-10-25

### An unmeasured edge stops `not_modeled`, never falls back to a plausible value
- **Rule:** When an argument cannot be read, a range overflows, or a client limit is unconfirmed, stop the run `not_modeled` or `missing_input` before any side effect. Do not skip the argument, clamp, or truncate.
- **Why:** `Skip If` skipped its unreadable count, `String Replace` truncated at an unconfirmed 511 bytes, and `Random Real` drew from a non-finite range. Each run reported `complete` on behavior nobody measured.
- **Prevention home:** owner contract — the modeled/not-modeled rule in `docs/architecture.md`.
- **Incidents:** [athena#59](https://github.com/wrightkit/athena/pull/59), [athena#64](https://github.com/wrightkit/athena/pull/64), [athena#66](https://github.com/wrightkit/athena/pull/66) — observed 2026-10-25, 2026-10-26

### A scheduler rule holds on every resume and dispatch path
- **Rule:** A gate or environment field added to one resume path (ongoing, player event, subroutine, `Wait Until` re-check) or one evaluation entry is added to all of them, with one test per path.
- **Why:** The slow-motion gate stopped new waits at the suspend site but none of the three resume arms, a `Wait Until` re-check built its env without the rule index, and `Evaluate Once` bypassed the argument pass that `call` runs.
- **Prevention home:** existing policy (`engineering-quality.md` "Hold the change on the whole path") plus per-arm tests in the fixing PRs.
- **Incidents:** [athena#66](https://github.com/wrightkit/athena/pull/66), [athena#81](https://github.com/wrightkit/athena/pull/81), [athena#121](https://github.com/wrightkit/athena/pull/121) — observed 2026-10-26
