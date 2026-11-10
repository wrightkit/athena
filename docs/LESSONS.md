# Lessons

Short review queue of reusable failure classes from merged athena work. Each entry names a prevention action and the evidence behind it. Promote an entry into a regression test, an owner contract, or an existing engineering standard, then remove it here. The procedure is the `merged-pr-retro` skill in `wrightkit/.agents`.

### A behavior change updates every document that states the behavior
- **Rule:** When a PR changes what the simulator does, search `architecture.md`, `simulator-interface.md`, `scenario-format.md`, `trace-format.md`, `findings.md`, and doc comments for the old claim and fix each one in the same commit.
- **Why:** The same claim is restated in several documents. Review rounds kept finding the copy the fix missed, so agents and readers saw contradicting contracts.
- **Prevention home:** none yet — the owner decided no new rule in [athena#126](https://github.com/wrightkit/athena/issues/126) (2026-11-03); revisit if the class recurs.
- **Incidents:** [athena#49](https://github.com/wrightkit/athena/pull/49), [athena#63](https://github.com/wrightkit/athena/pull/63), [athena#78](https://github.com/wrightkit/athena/pull/78), [athena#90](https://github.com/wrightkit/athena/pull/90), [athena#92](https://github.com/wrightkit/athena/pull/92), [athena#109](https://github.com/wrightkit/athena/pull/109), [athena#117](https://github.com/wrightkit/athena/pull/117) — observed 2026-10-25, 2026-10-26
