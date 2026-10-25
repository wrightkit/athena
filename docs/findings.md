# Findings

Behavior currently believed about the Workshop client, each tied to the probe that supports it. This is a reading aid, not an authority: the evidence, run count and result are in `probes/<id>/capture.json`, and a statement here is only as firm as that record. A statement leaves this list if its capture is re-judged `mismatch` or goes stale.

Check `runs` and `result` in the capture before relying on one.

## Scheduling

- The tick is 16 ms (P0, P1).
- Wait rounds up to a whole number of ticks (P1).
- `Wait Until(True, 0)` takes 0 ticks; `Wait(0)` takes 1 tick (P4).
- Ongoing is edge-triggered; an edge that occurs while the rule is in a Wait is dropped, not queued (P2).
- Rules whose conditions become true on the same tick run in file order when the rule that sets the condition is earlier in the file (P3).
- When the rule that sets the condition is later in the file, the earlier rules see the change one tick later (P3 driver-last).
- Wait Behavior (P6), with a condition that goes true, false, true, false while the rule waits 1 s: `Ignore Condition` finishes the wait and drops the later edges; `Abort When False` abandons the rest of the actions when the condition goes false, and a later true edge starts a new run; `Restart When True` restarts the full wait at the new true edge.

## Values

- `Position Of` read inside an action changed on every tick; the 12.5 Hz refresh hypothesis was not supported (P5). `Update Every Frame` made no difference in that access pattern and was not tested inside continuously re-evaluated elements.

## Inspector

- Log lines are prefixed `[mm:ss]`, so they cannot time anything.
- `Custom String` shows numbers with two decimals; the variable block shows three. Probes print milliseconds for that reason.
- Within one capture, whole-number values print either bare (`0`, `1056`, `1424`) or with two decimals (`1008.00`), so a formatter cannot assume one form.
- The native variable export omits variables whose value is zero (P6 `endAbort`).

The client version is recorded per capture in `capture.json` under `client.version`.

## Modeled, not measured

Simulator choices no capture backs yet — the detail lives in [architecture.md](architecture.md#choices-no-capture-covers-yet):

- `Ongoing - Each Player` per-player instantiation (instance order, per-instance edge and wait state) and the `team`/`player` filters of rule events, including hero-name targets (athena#46).
- `For Global Variable`/`For Player Variable` execution: bound evaluation points, the exit-check direction by step sign, and the `End` jump back to the first body action (athena#50).
