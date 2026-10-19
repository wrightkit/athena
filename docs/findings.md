# Findings

Behavior currently believed about the Workshop client, each tied to the probe that supports it. This is a reading aid, not an authority: the evidence, run count and result are in `probes/<id>/capture.json`, and a statement here is only as firm as that record. A statement leaves this list if its capture is re-judged `mismatch` or goes stale.

Most are provisional (fewer than three agreeing runs). Check `runs` and `result` before relying on one.

## Scheduling

- The tick is 16 ms (P0, P1).
- Wait rounds up to a whole number of ticks (P1).
- `Wait Until(True, 0)` takes 0 ticks; `Wait(0)` takes 1 tick (P4).
- Ongoing is edge-triggered; an edge that occurs while the rule is in a Wait is dropped, not queued (P2).
- Rules whose conditions become true on the same tick run in file order when the rule that sets the condition is earlier in the file (P3). The setter-later placement is tested by the separate driver-last variant, not by this finding.

## Values

- `Position Of` read inside an action changed on every tick; the 12.5 Hz refresh hypothesis was not supported (P5). `Update Every Frame` made no difference in that access pattern and was not tested inside continuously re-evaluated elements.

## Inspector

- Log lines are prefixed `[mm:ss]`, so they cannot time anything.
- `Custom String` shows numbers with two decimals; the variable block shows three. Probes print milliseconds for that reason.

The client version is recorded per capture in `capture.json` under `client.version`.
