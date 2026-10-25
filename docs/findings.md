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
- `Workshop Setting *` values (`Real`, `Integer`, `Toggle`, `Combo`, `Hero`) evaluate to their declared `default` argument — one lobby, no player-edited settings; a `Hero` default yields its member id as text (athena#54).
- Control-flow actions (athena#58): `Skip`/`Skip If` (a positional jump — it can land inside a block the jump never entered; a `True`/`False` count coerces to 1/0 and a negative count skips nothing), `Abort`/`Abort If` (ends the whole run, subroutine frames included; the timeline records it `aborted`), `Loop`/`Loop If`/`Loop If Condition Is True`/`Loop If Condition Is False` (restart the current frame's action list — inside `Call Subroutine`, the subroutine's own — with the condition list re-checked per pass), and `Break`/`Continue` inside `While`/`For` (`Continue` lands on the `End`, which steps a `For` or re-enters a `While` head; `Break` lands past it; outside a loop they are `not_modeled`).
- General and array values plus `At Index` variable writes (athena#56): truthiness and `Null`/boolean number coercions, `==`/`!=` element-wise array equality, `Round To Integer` modes, `Null` read as the empty array, past-end append and out-of-range no-op index writes, `Remove From Array By Value` dropping every match.
- Element-context array values and `Evaluate Once` (athena#60): `Filtered Array`/`Sorted Array`/`Is True For All`/`Is True For Any` bind `Current Array Element` and `Current Array Index` per element and read `Null` as the empty array, `Sorted Array` orders ascending by a numeric key with equal keys keeping place, `Is True For All`/`Any` stop at the deciding element and read the empty array as true/false, `Current Array Element`/`Index` outside an element context are `not_modeled`, and `Evaluate Once` is a plain read of its operand outside an element context and `not_modeled` inside one. `Mapped Array` stays `not_modeled`.
