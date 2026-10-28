# Trace timeline and JSON form

The `timeline` records and the JSON serialization of the `Trace` that [simulator-interface.md](simulator-interface.md) defines. Scheduler behavior is in [architecture.md](architecture.md) and [findings.md](findings.md).

## Timeline

`timeline` holds one `entries` element per tick that had activity — a rule execution or a state change — in order; ticks without activity are omitted. A run that ends `not_modeled` or `missing_input` still returns the entries up to and including the stop tick.

The timeline is bounded: `entries` stops at 10 000 (`TIMELINE_ENTRY_LIMIT`), and across all entries `rules` stops at 100 000 (`TIMELINE_RULE_LIMIT`) and `changes` at 100 000 (`TIMELINE_CHANGE_LIMIT`). Records past a bound are dropped — also inside a tick — and `truncated` is `true` once any bound dropped something.

Each entry:

| Field | Meaning |
| --- | --- |
| `tick` | The tick. |
| `time` | The tick's `Total Time Elapsed` in milliseconds: 15 + the run's elapsed game time (tick·16 at the normal rate). The match clock drives it — `Pause Match Time` holds it and `Set Slow Motion` scales its advance — so a paused or slowed clock repeats values across ticks. |
| `rules` | The rule executions that started, resumed or ended on the tick, in execution order. See below. |
| `changes` | The state changes the executions made, in execution order across rules. See below. |

Each `rules` element:

| Field | Meaning |
| --- | --- |
| `rule` | The rule's index among the program's enabled rules, in file order. |
| `name` | The rule's name. |
| `eventPlayer` | The scenario `id` of the run's `Event Player`: the triggering event's player — an injected event's `eventPlayer` or the player a raised event happened to — or the player an `Ongoing - Each Player` run is bound to; `null` for `Ongoing - Global` runs and events without one. |
| `ended` | How the execution ended on the tick: `finished` (stepped past its last action), `waiting` with `wake` (suspended; `wake` is the tick the wait expires — the resume tick for `Wait`, the timeout tick for `Wait Until`, whose condition can resume it earlier), `stopped` (the run's `not_modeled`/`missing_input` stop was inside this rule — an action, a condition check or an event filter; `rule` then names the rule that owns the stopped construct, the subroutine's inside `Call Subroutine`) or `aborted` (killed by an `Abort When False` wait going false or an `Abort`/`Abort If` action). |

Each `changes` element has `kind` plus its own fields; `old`/`new` serialize as the `Val` form:

| `kind` | Fields | The change |
| --- | --- | --- |
| `globalVariable` | `name`, `index`, `old`, `new` | A `Set`/`Modify Global Variable` write. `index` is the element path a `Set`/`Modify Global Variable At Index` action wrote (`null` for a whole-variable write). |
| `playerVariable` | `player`, `name`, `index`, `old`, `new` | A `Set`/`Modify Player Variable` write; `player` is the player `id`, `index` the element path an `At Index` action wrote. |
| `world` | `player`, `field`, `old`, `new` | A change to a player's world state or scenario `position`. `field` is `health`, `shield`, `armor`, `cooldown.<slot>` (for example `cooldown.ability1`), `status.<member>` (lowercase `Status` member id; `new` is the expiry seconds, `null` for a `Clear Status`), `button.<slot>` (a `bool` for `Allow`/`Disallow Button`), `position` (a `Teleport` write; `old`/`new` are vectors, `null` when the player had no `position`), `facing` (a `Set Facing` `To World` write; `old`/`new` are vectors of the facing's direction, `null` when the player had no `facing`) or `ultimate.charge`. For pools `old`/`new` are the `current` points; for a cooldown the reported seconds (`null` when the slot had none). Nothing writes `ultimate.charge` in this version. |
| `hudCreate` | `id`, `element` | A HUD element was created; `id` indexes `hud`, `element` is its `kind`. |
| `hudDestroy` | `id`, `element` | A HUD element was destroyed. |
| `effectCreate` | `id`, `element` | An effect entity was created or played; `id` indexes `effects`, `element` is its `kind`. |
| `effectDestroy` | `id`, `element` | An effect entity was destroyed. |
| `log` | `text` | A `Log To Inspector` line. |
| `randomDraw` | `rule`, `construct`, `value` | A `Random *` value drew from the scenario's `random` source: `rule` is the index of the rule the value belongs to (as in `rules`; inside `Call Subroutine`, the subroutine's own), `construct` the value id, `value` the produced value. |
| `state` | `player`, `key`, `old`, `new` | A state-cell write (see State-cell writes in [simulator-interface.md](simulator-interface.md#state-cell-writes-and-stored-modifiers)): `key` is the `state` key (`ultCharge`, `health`, `isDead`, …); `old` is what the key read before the write — the earlier write, the declared `state` entry or the established fallback — `null` when nothing answered it. |
| `modifier` | `player`, `action`, `args` | A stored-modifier action applied to a player: `action` is the action id, `args` the evaluated non-target arguments. The modifier changes no readable value. |
| `matchClock` | `action`, `field`, `old`, `new` | A match-clock write: `action` is the action id, `field` is `paused` (`Pause`/`Unpause Match Time`), `matchTime` (`Set Match Time`; `old` is `null` until a first write) or `slowMotion` (`Set Slow Motion`). |

## JSON form

`Trace` implements `serde::Serialize`. The JSON shape is part of the contract:

```json
{
  "outcome": {
    "status": "not_modeled",
    "construct": "value getPosition",
    "tick": 0,
    "location": { "rule": 0, "name": "init", "eventPlayer": null, "action": 1 }
  },
  "geometry": "emptyWorld",
  "logs": [{ "tick": 3, "text": "t0=3.023" }],
  "globals": { "a": { "type": "number", "value": 1.0 }, "b": { "type": "null" } },
  "players": {
    "p1": {
      "hero": "Ana",
      "team": "team1",
      "state": { "isDead": { "type": "bool", "value": false } },
      "modifiers": {},
      "world": {
        "health": { "current": 150.0, "max": 200.0 },
        "shield": { "current": 0.0, "max": 100.0 },
        "armor": null,
        "cooldowns": { "ability1": 10.0, "ability2/secondary": 12.0 },
        "ultimate": { "charge": 0.0, "cost": 2100.0 },
        "statuses": { "INVINCIBLE": 3.54 },
        "buttons": { "jump": false }
      }
    }
  },
  "hud": [
    {
      "id": 0,
      "kind": "hudText",
      "createdTick": 0,
      "destroyedTick": null,
      "recipients": ["p1"],
      "construct": "action createHudText",
      "header": { "type": "text", "value": "Score: 0" },
      "subheader": { "type": "null" },
      "text": { "type": "null" },
      "location": "LEFT",
      "sortOrder": { "type": "number", "value": 0.0 },
      "headerColor": "WHITE",
      "subheaderColor": "WHITE",
      "textColor": "WHITE",
      "reevaluation": "VISIBILITY_AND_STRING",
      "specVisibility": "DEFAULT"
    }
  ],
  "effects": [],
  "timeline": {
    "entries": [
      {
        "tick": 0,
        "time": 15.0,
        "rules": [
          {
            "rule": 0,
            "name": "init",
            "eventPlayer": null,
            "ended": "stopped"
          }
        ],
        "changes": [
          {
            "kind": "globalVariable",
            "name": "a",
            "index": null,
            "old": { "type": "null" },
            "new": { "type": "number", "value": 1.0 }
          },
          { "kind": "hudCreate", "id": 0, "element": "hudText" }
        ]
      }
    ],
    "truncated": false
  },
  "missingInputs": [],
  "expectations": []
}
```

`outcome` is `{"status": "complete"}`, `{"status": "not_modeled", "construct": ..., "tick": ..., "location": ...}` or `{"status": "missing_input"}`. A value is `{"type": ..., "value": ...}` with `type` one of `null`, `bool`, `number`, `text`, `array`, `player`, `vector`, `display`; `null` has no `value`. `player` holds the scenario `id` that declared the player; `vector` holds `{ "x": n, "y": n, "z": n }`. `players` is a map keyed by the scenario player `id`; a row's `world` is `null` when the player's catalog state could not be resolved.

Adding a field is compatible. Removing or renaming one, or changing a value's meaning, is a breaking change and needs a version bump of this crate.
