//! The timeline record types: what a tick's `rules` and `changes`
//! entries hold (athena#48), re-exported at the crate root.

use serde::Serialize;

use crate::{EffectKind, Finite, HudKind, Val};

/// One rule execution that started, resumed or ended on a tick.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRun {
    /// The rule's index among the program's enabled rules, in file order.
    pub rule: usize,
    /// The rule's name.
    pub name: String,
    /// The scenario `id` of the run's `Event Player`: the triggering
    /// event's player, or the player an `Ongoing - Each Player` run is
    /// bound to; `null` for `Ongoing - Global` runs and events without
    /// one.
    pub event_player: Option<String>,
    /// How the execution ended on this tick.
    #[serde(flatten)]
    pub ended: RunEnd,
}

/// How a rule execution recorded in the timeline ended on its tick.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "ended", rename_all = "snake_case")]
pub enum RunEnd {
    /// The execution stepped past its last action.
    Finished,
    /// A `Wait`/`Wait Until` suspended it; `wake` is the tick the wait
    /// expires: the resume tick for `Wait`, the timeout tick for
    /// `Wait Until`, whose condition can resume it earlier.
    Waiting { wake: u64 },
    /// It was killed: by an `Abort When False` wait going false or by an
    /// `Abort`/`Abort If` action.
    Aborted,
    /// The run stopped on this rule — `not_modeled` or `missing_input`;
    /// the outcome's location names the action or condition.
    Stopped,
}

/// One state change recorded in a timeline entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Change {
    /// A `Set`/`Modify Global Variable` write. `index` is the element path
    /// of an array-element write a `* At Index` action made (`null` for a
    /// whole-variable write).
    GlobalVariable {
        name: String,
        index: Option<Vec<u64>>,
        old: Val,
        new: Val,
    },
    /// A `Set`/`Modify Player Variable` write, with the player `id`.
    PlayerVariable {
        player: String,
        name: String,
        index: Option<Vec<u64>>,
        old: Val,
        new: Val,
    },
    /// A change to a player's catalog-derived world state — or to the
    /// scenario `position`/`facing` a `Teleport`/`Set Facing` writes.
    /// `field` is `health`,
    /// `shield`, `armor`, `cooldown.<slot>` (for example
    /// `cooldown.ability1`), `status.<member>` (lowercase `Status`
    /// member id), `button.<slot>`, `ultimate.charge`, `position` or
    /// `facing`;
    /// for pools `old`/`new` are the `current` points, for a cooldown
    /// the reported seconds (`null` when the slot had none), for
    /// `status.<member>` the expiry seconds (`null` on a `Clear
    /// Status`), for `button.<slot>` a bool, for `position`/`facing`
    /// the vectors (`null` when the player had none), and nothing
    /// writes `ultimate.charge` in this version.
    World {
        player: String,
        field: String,
        old: Val,
        new: Val,
    },
    /// A HUD element was created; `id` indexes `hud`.
    HudCreate { id: u64, element: HudKind },
    /// A HUD element was destroyed.
    HudDestroy { id: u64, element: HudKind },
    /// An effect entity was created or played; `id` indexes `effects`.
    EffectCreate { id: u64, element: EffectKind },
    /// An effect entity was destroyed.
    EffectDestroy { id: u64, element: EffectKind },
    /// A match-clock write (athena#120): `Pause`/`Unpause Match Time`
    /// writes `paused`, `Set Match Time` writes `matchTime` (`old` is
    /// `null` until a first write), `Set Slow Motion` writes
    /// `slowMotion`. `action` is the action id.
    MatchClock {
        action: String,
        field: String,
        old: Val,
        new: Val,
    },
    /// A `Log To Inspector` line.
    Log { text: String },
    /// A `Random *` value drew from the scenario's `random` source.
    /// `rule` is the index of the rule the value belongs to (as in
    /// `rules`; inside a `Call Subroutine`, the subroutine's own),
    /// `construct` the value id, `value` the produced value.
    RandomDraw {
        rule: usize,
        construct: String,
        value: Val,
    },
    /// A state-cell write: `Set Ultimate Charge`, `Set Health`/
    /// `Set Max Health`, `Kill`/`Respawn`/`Resurrect` and the rest of
    /// the class write a `state` key of the player. `key` is the
    /// `state` key (`ultCharge`, `health`, `isDead`, …); `old` is the
    /// value the cell previously read — the earlier write, the
    /// declared `state` entry or the established fallback — `null`
    /// when nothing answered it (athena#116).
    State {
        player: String,
        key: String,
        old: Val,
        new: Val,
    },
    /// A stored-modifier action applied to a player: the action id,
    /// the player `id` and the evaluated non-target arguments. The
    /// modifier is recorded on the player and changes no readable
    /// value (athena#116).
    Modifier {
        player: String,
        action: String,
        args: Vec<Val>,
    },
}

/// One timeline entry: the activity of one tick. `rules` lists the rule
/// executions that started, resumed or ended on the tick, `changes` the
/// state changes they made, both in execution order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimelineEntry {
    pub tick: u64,
    /// The tick's `Total Time Elapsed` in milliseconds: 15 + the
    /// elapsed game-time seconds. The match clock drives it, so a
    /// paused or slowed clock repeats values across ticks.
    pub time: Finite,
    pub rules: Vec<RuleRun>,
    pub changes: Vec<Change>,
}

/// The per-tick timeline of a run: one entry per tick that had activity, in
/// order. The bounds below keep a long or dense run from growing it
/// without limit. A run that stops early still returns the timeline up to
/// the stop.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Timeline {
    pub entries: Vec<TimelineEntry>,
    /// `true` once a bound dropped records: `entries` past
    /// [`TIMELINE_ENTRY_LIMIT`], `rules` past [`TIMELINE_RULE_LIMIT`] or
    /// `changes` past [`TIMELINE_CHANGE_LIMIT`].
    pub truncated: bool,
}

/// The most ticks with activity a trace's `timeline` keeps.
pub const TIMELINE_ENTRY_LIMIT: usize = 10_000;

/// The most rule executions a trace's `timeline` keeps across all
/// entries; past it they are dropped and `truncated` is set.
pub const TIMELINE_RULE_LIMIT: usize = 100_000;

/// The most state changes a trace's `timeline` keeps across all
/// entries; past it they are dropped and `truncated` is set.
pub const TIMELINE_CHANGE_LIMIT: usize = 100_000;
