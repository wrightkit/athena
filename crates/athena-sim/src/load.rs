//! Program loading: enabled rules become `Instance`s — one per matching
//! scenario player for `Ongoing - Each Player` — with their block maps and
//! the subroutine table.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use workshop_rs::{Event, EventTarget, EventTeam, Program, Rule};

use crate::blocks::{Blocks, block_maps};
use crate::scenario::{EventCtx, Player, Team};
use crate::{Stop, StopLocation, Val};

/// The position of a rule execution in progress: the rule whose actions the
/// program counter reads. A `Call Subroutine` action pushes a frame.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Frame {
    pub rule_idx: usize,
    pub pc: usize,
}

/// What ends or cancels the suspension of a rule run besides its wake tick.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum WaitMode {
    Ignore,
    AbortWhenFalse,
    RestartWhenTrue,
    /// A `Wait Until`; the frame points at that action, whose condition is
    /// checked every tick.
    Until(Frame),
}

pub(crate) struct Instance<'a> {
    pub rule: &'a Rule,
    pub rule_idx: usize,
    /// The player an `Ongoing - Each Player` instance runs for; `Event
    /// Player` binds it. `None` for every other event.
    pub each_player: Option<String>,
    pub frames: Vec<Frame>,
    pub wake: Option<u64>,
    pub wait_mode: WaitMode,
    pub prev: bool,
    /// Injected events that triggered this rule and wait for it to be idle.
    pub pending: VecDeque<EventCtx>,
    /// The injected event the current run executes for, if event-triggered.
    pub event: Option<EventCtx>,
    /// The clause head a false `If`/`Else If` jumped to, as a frame
    /// position. A head that is not this target was reached by fall-through
    /// after a matched clause and exits its chain. The jump is consumed the
    /// step after it is set or names an `End` line, so a stale target left
    /// by a nested chain or a subroutine frame can never satisfy the
    /// enclosing chain's search.
    pub clause_jump: Option<Frame>,
    /// An `Abort`/`Abort If` ended the run in progress; the timeline
    /// records it `aborted` rather than `finished`.
    pub aborted: bool,
    /// `Evaluate Once` pins of the run in progress, keyed by the value's
    /// site in the parsed program: a `For`/`While`/`Loop` body or a
    /// `Loop`-restarted frame re-reads the site and gets the pin, and a
    /// `Wait` suspension keeps the pins for the resumed run. Cleared
    /// when a run of this instance starts or restarts.
    pub once: RefCell<BTreeMap<usize, Val>>,
    /// `Evaluate Once` pins of the `Wait Until` in progress: one call's
    /// per-tick condition re-checks share them and reaching the action
    /// again clears them.
    pub until_once: RefCell<BTreeMap<usize, Val>>,
}

pub(crate) struct Loaded<'a> {
    pub rules: Vec<&'a Rule>,
    pub blocks: Vec<Blocks>,
    /// Subroutine name → rule index.
    pub subroutines: BTreeMap<String, usize>,
    pub instances: Vec<Instance<'a>>,
}

/// Whether a scenario player satisfies a rule event's `team`/`target`
/// filters: `Team 1`/`Team 2` match the declared `team`, a hero name matches
/// the catalog member the declared `hero` resolves to, `Slot N` matches the
/// declared `slot`, and `All` imposes no constraint. A player that does not
/// declare the read field does not match; a filter the `team` arm rejects
/// first is never evaluated.
pub(crate) fn player_matches(
    team: EventTeam,
    target: &EventTarget,
    player: &Player,
) -> Result<bool, Stop> {
    let matches = match team {
        EventTeam::All => true,
        EventTeam::Team1 => player.team == Some(Team::Team1),
        EventTeam::Team2 => player.team == Some(Team::Team2),
    };
    if !matches {
        return Ok(false);
    }
    match target {
        EventTarget::All => Ok(true),
        EventTarget::Hero(hero) => Ok(player.hero_member.as_deref() == Some(hero.as_str())),
        EventTarget::Slot(slot) => Ok(player.slot == Some(*slot)),
    }
}

pub(crate) fn load<'a>(
    program: &'a Program,
    players: &BTreeMap<String, Player>,
) -> Result<Loaded<'a>, Stop> {
    let mut rules = Vec::new();
    let mut blocks = Vec::new();
    let mut subroutines = BTreeMap::new();
    let mut instances = Vec::new();
    for (rule_idx, rule) in program
        .rules
        .iter()
        .filter(|rule| !rule.disabled)
        .enumerate()
    {
        // An `Ongoing - Each Player` rule becomes one instance per matching
        // scenario player; a filter matching nobody yields no instance, so
        // the rule never fires. Every other event gets a single instance.
        let each_players: Vec<Option<String>> = match &rule.event {
            Event::Global | Event::Player { .. } | Event::Subroutine(_) => vec![None],
            Event::EachPlayer | Event::EachPlayerWithFilters { .. } => {
                let (team, target) = match &rule.event {
                    Event::EachPlayer => (EventTeam::All, &EventTarget::All),
                    Event::EachPlayerWithFilters { team, target } => (*team, target),
                    _ => unreachable!(),
                };
                let mut bound = Vec::new();
                for (id, player) in players {
                    if player_matches(team, target, player).map_err(|stop| {
                        stop.stopped_at(StopLocation {
                            rule: rule_idx,
                            name: rule.name.clone(),
                            event_player: Some(id.clone()),
                            at: None,
                        })
                    })? {
                        bound.push(Some(id.clone()));
                    }
                }
                bound
            }
        };
        if let Event::Subroutine(name) = &rule.event {
            subroutines.insert(name.clone(), rule_idx);
        }
        rules.push(rule);
        blocks.push(block_maps(&rule.actions).map_err(|stop| {
            stop.stopped_at(StopLocation {
                rule: rule_idx,
                name: rule.name.clone(),
                event_player: None,
                at: None,
            })
        })?);
        for each_player in each_players {
            instances.push(Instance {
                rule,
                rule_idx,
                each_player,
                frames: Vec::new(),
                wake: None,
                wait_mode: WaitMode::Ignore,
                prev: false,
                pending: VecDeque::new(),
                event: None,
                clause_jump: None,
                aborted: false,
                once: RefCell::new(BTreeMap::new()),
                until_once: RefCell::new(BTreeMap::new()),
            });
        }
    }
    Ok(Loaded {
        rules,
        blocks,
        subroutines,
        instances,
    })
}
