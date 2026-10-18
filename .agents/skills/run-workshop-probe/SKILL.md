---
name: run-workshop-probe
description: >
  Run one Workshop runtime-behavior probe from this repository and record the
  result: prepare and validate `probes/<id>/probe.zh-CN.ow`, hand it to the user
  to paste into the Overwatch client, receive the Inspector text, save it as a
  verbatim capture, judge it against the README verdict table, and write
  `capture.json`. Use when the user asks to run, re-run, repeat, or record a
  probe (P0 to P5), to judge a pasted Inspector log, or to find which probe still
  needs runs. Do NOT use to design a new probe, to edit Workshop semantics, or to
  claim a probe was run without a capture.
---

# Run a Workshop probe

One probe, one session. The agent prepares the file and judges the result. Pasting into
the client and copying the Inspector back is done by the user. Do not attempt to drive
the game client yourself, and never state or imply a probe ran unless a capture exists.

The expected values and the `match` / `mismatch` / `inconclusive` conditions are in
`README.md` under "操作与判定". Read that table for the chosen probe. Do not restate or
invent expected values here or in the reply. The rules for captures are in `AGENTS.md`.

## 1. Choose the probe

If the user named one, use it. Otherwise list `probes/*/capture.json` and pick the
probe with the fewest `runs` (fewer than 3), or one whose `probe_sha256` no longer
matches. Report which one and why.

## 2. Prepare and validate

```bash
wright check --kind workshop probes/<id>/probe.workshop
wright check --kind workshop probes/<id>/probe.zh-CN.ow
wright inspect cost --kind workshop probes/<id>/probe.workshop | grep -o "[0-9]* element(s) of"
wright inspect cost --kind workshop --locale zh-CN probes/<id>/probe.zh-CN.ow | grep -o "[0-9]* element(s) of"
shasum -a 256 probes/<id>/probe.zh-CN.ow
```

Both checks must pass and the element counts must be equal. If not, stop and report.
Note the SHA-256: it goes into `probe_sha256`.

## 3. Hand over to the user

Tell the user, briefly:

1. Which file to paste: `probes/<id>/probe.zh-CN.ow` (the file path, not its content
   unless they ask).
2. The general steps from the README "通用步骤" (Workshop mode, Inspector and recording
   on, no Slow Motion, only this probe pasted, start the game, do not touch the character).
3. The probe's last log line to wait for, from the README table.
4. P5 only: choose a hero and spawn, leave about 3 seconds of open ground ahead. The
   first log line must show `spawned=真` and a coordinate triple.
5. To paste back the **whole** Inspector text, variable block included.

Then wait. Do not continue until the user sends the text.

## 4. Save the capture verbatim

Check the pasted text before saving:

- the variable block is present;
- the probe's last log line is present, and the line count matches the README table;
- for P5, the first line shows `spawned=真` and a coordinate, not `pos=0`.

If a check fails, say which one and ask for the text again, or mark the run invalid
(P5 with `pos=0` means the host had not spawned). Never repair, reformat, or complete
the text.

Save it unchanged: `capture.txt` for run 1, `capture-2.txt` for run 2, `capture-3.txt`
for run 3. Do not overwrite an earlier run.

## 5. Judge

Compare the log with the README expected values for this probe and decide `match`,
`mismatch`, or `inconclusive` by the README conditions only. Quote the observed numbers
next to the expected ones. Cross-check with the variable block (for example `i`,
`runs`, `t0`) and report any disagreement instead of smoothing it over.

With several runs: the result is final only when at least three runs agree. Fewer
than three agreeing runs, or runs that disagree, keep the result provisional and say so.
If runs disagree, do not pick one; report `inconclusive` with the differences.

## 6. Write `capture.json`

Update or create `probes/<id>/capture.json` with the existing shape:

```json
{
  "probe": "<id>",
  "client": { "version": "", "date": "YYYY-MM-DD", "locale": "zh-CN" },
  "runs": 1,
  "probe_sha256": "<from step 2>",
  "hypothesis": "...",
  "observed": [],
  "result": "match | mismatch | inconclusive",
  "notes": ""
}
```

- Ask the user for the client version once if it is empty. Do not guess it.
- `runs` is the number of saved captures that are valid for this `probe_sha256`.
- `notes` states whether the result is provisional, anything the probe cannot test
  (for example P5 does not test `Update Every Frame` inside HUD or effects), and
  invalid runs.
- Keep `observed` to what the capture shows, in milliseconds.

## 7. Report

Lead with the verdict and whether it is provisional. Then the observed values beside
the expected ones, anything that disagreed, and what is still owed (remaining runs,
client version). Do not commit unless the user asks.
