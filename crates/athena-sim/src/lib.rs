mod actions;
mod ambiguous;
mod blocks;
mod call;
mod effects;
mod eval;
mod expect;
mod geometry;
mod hero_settings;
mod hud;
mod load;
mod modify;
mod noop;
mod player;
mod scenario;
mod sched;
mod state;
mod state_reads;
mod state_writes;
mod step;
mod timeline;
mod vector;

pub use timeline::{
    Change, RuleRun, RunEnd, TIMELINE_CHANGE_LIMIT, TIMELINE_ENTRY_LIMIT, TIMELINE_RULE_LIMIT,
    Timeline, TimelineEntry,
};

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Val {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Val>),
    /// A mocked player, identified by the scenario `id` that declared it.
    Player(String),
    /// A Workshop vector: `x` is left, `y` is up, `z` is forward.
    /// Serializes as `{ "type": "vector", "value": { "x": n, "y": n, "z": n } }`.
    Vector {
        x: f64,
        y: f64,
        z: f64,
    },
    /// An opaque display value — the client rendering token a display-only
    /// producer yields: `producer` id + evaluated `args`, never the content.
    Display {
        producer: String,
        args: Vec<Val>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub tick: u64,
    pub text: String,
}

/// Where inside a rule a run stopped: an action or a condition index,
/// zero-based in the rule's own list.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopIndex {
    /// An action's position in the rule's action list.
    Action(usize),
    /// A condition's position in the rule's condition list.
    Condition(usize),
}

/// Where a run stopped. `rule`, `name` and `event_player` mean what they do
/// in a `rules` timeline element, except that `rule` is the index of the
/// rule the stopped construct belongs to — inside a `Call Subroutine`, the
/// subroutine's own. `at` is the action or condition index that stopped
/// the run; `None` when the rule's event filter or block structure stopped
/// it instead.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopLocation {
    /// The index of the rule holding the stopped construct among the
    /// program's enabled rules, in file order.
    pub rule: usize,
    /// The rule's name.
    pub name: String,
    /// The scenario `id` the rule ran or was matched for: the `Event
    /// Player` binding; `None` for `Ongoing - Global` runs and checks with
    /// none.
    pub event_player: Option<String>,
    #[serde(flatten)]
    pub at: Option<StopIndex>,
}

/// A construct the simulator does not model. The run stops at it; nothing is
/// defaulted or skipped.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotModeled {
    /// What was met, for example `action wait` or `value getPosition`.
    pub construct: String,
    /// The tick on which it was met; `None` when found before the first tick.
    pub tick: Option<u64>,
    /// Where the run stopped: the rule, its `Event Player` and the action
    /// or condition index. `None` only when no rule was involved, which no
    /// current path produces.
    pub location: Option<StopLocation>,
}

impl NotModeled {
    pub(crate) fn new(construct: impl Into<String>) -> Self {
        Self {
            construct: construct.into(),
            tick: None,
            location: None,
        }
    }

    pub(crate) fn at(mut self, tick: u64) -> Self {
        self.tick = Some(tick);
        self
    }
}

/// Which scenario input a run stopped on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MissingInputKind {
    /// The `hero` of a player.
    Hero,
    /// The `team` of a player.
    Team,
    /// The `eventPlayer` of an injected event.
    EventPlayer,
    /// An entry of an injected event's `values`.
    EventValue,
    /// The held state of a player's button.
    Button,
    /// The scenario's `random` source.
    Random,
    /// The scenario's `serverLoad` numbers.
    ServerLoad,
    /// The scenario's `map`.
    Map,
    /// The `position` of a scenario player.
    Position,
    /// The `facing` of a scenario player.
    Facing,
    /// An entry of the scenario's `objectives` list.
    Objectives,
    /// An entry of a scenario player's `state` map.
    State,
    /// The `slot` of a scenario player.
    Slot,
    /// The scenario's `matchPhase`.
    MatchPhase,
    /// The scenario's `gameMode`.
    GameMode,
    /// The scenario player marked `local`.
    Local,
}

/// A live input the program read but the scenario did not supply. The run
/// stops at the first one; the simulator never fills it in.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissingInput {
    /// The kind of input.
    pub input: MissingInputKind,
    /// The `id` of the player the input belongs to, or `None` for
    /// `eventPlayer` and `eventValue`.
    pub player: Option<String>,
    /// The event-value name for `eventValue`, the button slot for
    /// `button`; `None` for the others.
    pub name: Option<String>,
    /// The tick on which the program read it.
    pub tick: Option<u64>,
    /// A short description for people, as in `not_modeled`. Do not match on
    /// it.
    pub construct: String,
    /// Where the run stopped, as in `not_modeled`.
    pub location: Option<StopLocation>,
}

impl MissingInput {
    pub(crate) fn new(
        input: MissingInputKind,
        player: Option<String>,
        name: Option<String>,
        construct: impl Into<String>,
    ) -> Self {
        Self {
            input,
            player,
            name,
            tick: None,
            construct: construct.into(),
            location: None,
        }
    }

    pub(crate) fn at(mut self, tick: u64) -> Self {
        self.tick = Some(tick);
        self
    }
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    /// The whole requested time was simulated.
    Complete,
    NotModeled(NotModeled),
    /// The run stopped at a live input the scenario did not supply; see
    /// `missingInputs`.
    MissingInput,
}

/// Whether one expectation held at the end of the run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectationStatus {
    Pass,
    Fail,
    /// The outcome was not `complete`, so nothing was checked.
    NotEvaluated,
}

/// The result of one scenario expectation, in file order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExpectationResult {
    /// The expectation's position in the scenario's `expect` list.
    pub index: usize,
    pub status: ExpectationStatus,
    /// The observed value; `null` when it does not exist or was not checked.
    pub actual: Val,
}

/// One health pool: `current` points out of `max`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PoolState {
    pub current: f64,
    pub max: f64,
}

/// Ultimate charge: `charge` points toward `cost`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UltimateState {
    pub charge: f64,
    pub cost: f64,
}

/// A player's catalog-derived world state: health pools, ability cooldowns,
/// ultimate charge, status effects and button slots. Start values come from
/// `workshop_rs::hero_values` at `CURRENT_VERSION`; `Damage` and `Heal` move
/// `current` inside a pool and nothing else changes it (no ability use is
/// simulated).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerWorld {
    /// The `health` pool; a hero always has one.
    pub health: PoolState,
    /// The `shield` pool, `None` when the catalog records none.
    pub shield: Option<PoolState>,
    /// The `armor` pool, `None` when the catalog records none.
    pub armor: Option<PoolState>,
    /// Cooldown seconds per ability slot the catalog records a value for;
    /// keys are `slot` or `slot/variant`.
    pub cooldowns: BTreeMap<String, f64>,
    /// Ultimate charge toward the catalog's cost.
    pub ultimate: UltimateState,
    /// Status effects `Set Status` applied: `Status` member id to the
    /// `Total Time Elapsed` seconds at which the status ends.
    pub statuses: BTreeMap<String, f64>,
    /// The `Allow Button`/`Disallow Button` writes per button slot; a
    /// slot never written is allowed.
    pub buttons: BTreeMap<String, bool>,
}

/// A player's row in the state table the trace reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerState {
    /// The scenario's `hero` text, `None` when undeclared.
    pub hero: Option<String>,
    /// `team1` or `team2`, `None` when undeclared.
    pub team: Option<String>,
    /// The player's `state` values at the end of the run, keyed by the
    /// `state` key: the scenario's declared map overlaid by the cells
    /// the state-write actions wrote. A write defines a key the
    /// scenario never supplied.
    pub state: BTreeMap<String, Val>,
    /// The modifiers the stored-modifier actions recorded on the
    /// player, keyed by the modifier name (`ultEnabled`,
    /// `damageDealt`, `forcedHero`, …). Stored state nothing in the
    /// model reads back.
    pub modifiers: BTreeMap<String, Val>,
    /// The catalog-derived world state, `None` when the run could not
    /// resolve it (a player without a `hero`, a hero value the catalog
    /// lacks, or a hero setting the program authors that the simulator
    /// does not apply and the player's `state` map does not cover).
    /// Touching an unresolved player's world state — dealt or received —
    /// ends the run as `not_modeled`.
    pub world: Option<PlayerWorld>,
}

/// Which create action produced a HUD element.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HudKind {
    /// `createHudText`.
    HudText,
    /// `progressBarHud`.
    ProgressBarHud,
    /// `bigMessage`.
    BigMessage,
    /// `smallMessage`.
    SmallMessage,
    /// `setObjectiveDescription`.
    ObjectiveDescription,
}

/// Which action produced an effect entity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectKind {
    /// `createEffect` — a durable entity `Destroy Effect` can target and
    /// `Last Created Entity` reads.
    Effect,
    /// `playEffect` — a one-shot effect, present only on its tick.
    PlayedEffect,
}

/// Serialize a member-id text bare (`"WHITE"`), anything else as [`Val`].
fn member_or_val<S: serde::Serializer>(
    value: &Option<Val>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(Val::Text(member)) => serializer.serialize_str(member),
        _ => value.serialize(serializer),
    }
}

/// The evaluated arguments an effect entity records, present only when the
/// action takes the argument. [`Val`]s serialize as values; enum arguments
/// (effect type, color, reevaluation) as the catalog member id — a `color`
/// holding no member id takes the [`Val`] form, for example an opaque
/// `display` value. `position` is whatever the expression evaluated to — the
/// model has no geometry, and needing it is `not_modeled`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectArgs {
    /// The `Effect`/`DynamicEffect` member id of the `Type` argument.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "member_or_val"
    )]
    pub color: Option<Val>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Val>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<Val>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reevaluation: Option<String>,
}

/// An effect entity the run created or played. The `effects` trace field
/// lists every entity in creation order; a played effect records
/// `destroyedTick` equal to `createdTick` — it is never present past its
/// tick.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EffectElement {
    /// Stable id in creation order, starting at 0. `lastCreatedEntity`
    /// resolves to the `id` of the newest `effect` element.
    pub id: u64,
    pub kind: EffectKind,
    /// The tick the entity was created or played on.
    #[serde(rename = "createdTick")]
    pub created_tick: u64,
    /// The tick a `Destroy Effect` removed the entity; `null` while
    /// present.
    #[serde(rename = "destroyedTick")]
    pub destroyed_tick: Option<u64>,
    /// The scenario `id`s of the players the entity is presented to: the
    /// evaluated `Visible To` argument.
    pub recipients: Vec<String>,
    /// A short description of the creating statement, for people, as in
    /// `outcome`. Do not match on it.
    pub construct: String,
    /// The evaluated arguments of the action.
    #[serde(flatten)]
    pub args: EffectArgs,
}

/// The evaluated arguments a HUD element records, present only when the action
/// takes the argument. [`Val`]s serialize as values; enum arguments (location,
/// reevaluation, spectator visibility) as the catalog member id — a color holding
/// no member id takes the [`Val`] form, for example an opaque `display` value.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<Val>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subheader: Option<Val>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Val>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Val>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<Val>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "member_or_val"
    )]
    pub header_color: Option<Val>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "member_or_val"
    )]
    pub subheader_color: Option<Val>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "member_or_val"
    )]
    pub text_color: Option<Val>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "member_or_val"
    )]
    pub progress_bar_color: Option<Val>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reevaluation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_visibility: Option<String>,
}

/// A HUD element the run created. The `hud` trace field lists every element
/// in creation order; `destroyedTick` is `null` while the element is
/// present.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HudElement {
    /// Stable id in creation order, starting at 0. `lastTextId` resolves to
    /// the `id` of the newest `hudText` or `progressBarHud` element.
    pub id: u64,
    pub kind: HudKind,
    /// The tick the element was created on.
    #[serde(rename = "createdTick")]
    pub created_tick: u64,
    /// The tick a destroy action removed it; `null` while present.
    #[serde(rename = "destroyedTick")]
    pub destroyed_tick: Option<u64>,
    /// The scenario `id`s of the players the element is presented to: the
    /// evaluated `Visible To` argument.
    pub recipients: Vec<String>,
    /// A short description of the creating statement, for people, as in
    /// `outcome`. Do not match on it.
    pub construct: String,
    /// The evaluated arguments of the creating action.
    #[serde(flatten)]
    pub args: HudArgs,
}

/// The geometry model a run answers spatial queries with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Geometry {
    /// No map geometry: raycasts never hit, line of sight always holds,
    /// every position is walkable, on the ground and in no spawn room.
    /// Modeled, not measured — a viewer should warn that
    /// geometry-dependent results are not real.
    EmptyWorld,
}

/// What a run produced. `logs`, `globals`, `players` and `hud` describe the
/// state when the run ended: after the last tick for `Complete`, at the stop
/// for `NotModeled` and `MissingInput`. `timeline` records what happened on
/// each tick that had activity. Consumers must check `outcome`
/// before using any of them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trace {
    pub outcome: Outcome,
    /// The geometry model the run answered spatial queries with —
    /// `emptyWorld` in this version.
    pub geometry: Geometry,
    pub logs: Vec<LogEntry>,
    pub globals: BTreeMap<String, Val>,
    /// The player state table: one row per scenario player. Empty when the
    /// run had no scenario.
    pub players: BTreeMap<String, PlayerState>,
    /// The HUD elements the run created, in creation order.
    pub hud: Vec<HudElement>,
    /// The effect entities the run created or played, in creation order.
    pub effects: Vec<EffectElement>,
    /// The per-tick timeline.
    pub timeline: Timeline,
    /// The live inputs the program read but the scenario did not supply.
    /// Empty unless `outcome` is `missing_input`; at most one entry in
    /// version 1.
    #[serde(rename = "missingInputs")]
    pub missing_inputs: Vec<MissingInput>,
    /// One result per scenario expectation, in file order.
    pub expectations: Vec<ExpectationResult>,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Simulated seconds to run, counted from the first tick.
    pub seconds: f64,
}

/// The text did not parse as Workshop, so there is nothing to simulate.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

/// The scenario document is not well-formed or fails a loader check, so the
/// run is refused and nothing is simulated.
#[derive(Debug, Clone, PartialEq)]
pub enum ScenarioError {
    /// Not a scenario document: bad JSON, a missing or mistyped field, an
    /// unknown field, or a value outside the format.
    Format(String),
    /// `scenarioVersion` is missing, not an integer, or not in
    /// `supported`.
    Version { found: String, supported: Vec<u64> },
    /// A loader check the JSON Schema cannot express failed: a duplicated
    /// player `id`, a reference to an undeclared player, or an unknown hero
    /// name.
    Load(String),
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(message) => write!(f, "malformed scenario: {message}"),
            Self::Version { found, supported } => write!(
                f,
                "unsupported scenarioVersion {found}; supported versions: {supported:?}"
            ),
            Self::Load(message) => write!(f, "scenario check failed: {message}"),
        }
    }
}

impl std::error::Error for ScenarioError {}

/// Why `run_scenario` refused to run.
#[derive(Debug, PartialEq)]
pub enum RunError {
    /// The text is not Workshop.
    Parse(ParseError),
    /// The scenario is malformed or fails a loader check.
    Scenario(ScenarioError),
    /// The pinned hero-values file failed to load; neither the program nor
    /// the scenario is the cause.
    HeroValues(workshop_rs::hero_values::HeroValuesError),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Scenario(error) => write!(f, "{error}"),
            Self::HeroValues(error) => write!(f, "hero values: {error}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Why a run stopped before its time was up. The variants are boxed so
/// `Result<_, Stop>` stays small.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stop {
    NotModeled(Box<NotModeled>),
    MissingInput(Box<MissingInput>),
}

impl Stop {
    pub(crate) fn at(self, tick: u64) -> Self {
        match self {
            Self::NotModeled(not_modeled) => Self::NotModeled(Box::new(not_modeled.at(tick))),
            Self::MissingInput(missing) => Self::MissingInput(Box::new(missing.at(tick))),
        }
    }

    /// Fill in where the run stopped when the raising code did not say;
    /// a location already set by a site closer to the construct wins.
    pub(crate) fn stopped_at(mut self, location: StopLocation) -> Self {
        match &mut self {
            Self::NotModeled(stop) => {
                if stop.location.is_none() {
                    stop.location = Some(location);
                }
            }
            Self::MissingInput(stop) => {
                if stop.location.is_none() {
                    stop.location = Some(location);
                }
            }
        }
        self
    }
}

impl From<NotModeled> for Stop {
    fn from(not_modeled: NotModeled) -> Self {
        Self::NotModeled(Box::new(not_modeled))
    }
}

impl From<MissingInput> for Stop {
    fn from(missing: MissingInput) -> Self {
        Self::MissingInput(Box::new(missing))
    }
}

fn parse_program(
    text: &str,
    catalog: &workshop_rs::catalog::Catalog,
) -> Result<workshop_rs::Program, ParseError> {
    let parse = |error: &dyn fmt::Display| ParseError(error.to_string());
    let locale = workshop_rs::detect::resolve_locale(text, catalog, None).map_err(|e| parse(&e))?;
    workshop_rs::parser::parse(text, catalog, &locale).map_err(|e| parse(&e))
}

/// Parse Workshop text (any supported locale) and run it.
///
/// A program that parses always yields a [`Trace`]. Behavior outside the model
/// is reported in [`Trace::outcome`], never defaulted.
pub fn run_text(text: &str, options: Options) -> Result<Trace, ParseError> {
    let parse = |error: &dyn fmt::Display| ParseError(error.to_string());
    let catalog = workshop_rs::catalog::Catalog::builtin_ref().map_err(|e| parse(&e))?;
    let program = parse_program(text, catalog)?;
    let values = hero_values().map_err(|e| parse(&e))?;
    Ok(sched::run(&program, options, None, values))
}

fn hero_values()
-> Result<&'static workshop_rs::hero_values::HeroValues, workshop_rs::hero_values::HeroValuesError>
{
    workshop_rs::hero_values::builtin(workshop_rs::hero_values::CURRENT_VERSION)
}

/// Parse Workshop text and run it with a scenario: the players, injected
/// events and expectations the document declares.
///
/// A malformed scenario is refused; a well-formed one always yields a
/// [`Trace`]. A live input the program reads but the scenario omits stops the
/// run with `outcome.status` `missing_input`. Player start values come from
/// the hero-values file pinned at [`workshop_rs::hero_values::CURRENT_VERSION`].
pub fn run_scenario(text: &str, scenario_json: &str) -> Result<Trace, RunError> {
    let values = hero_values().map_err(RunError::HeroValues)?;
    run_scenario_with_hero_values(text, scenario_json, values)
}

/// [`run_scenario`] against an explicit hero-values set instead of the
/// pinned file. Tests use it with values loaded through
/// `workshop_rs::hero_values::load`; production callers use `run_scenario`.
pub fn run_scenario_with_hero_values(
    text: &str,
    scenario_json: &str,
    hero_values: &workshop_rs::hero_values::HeroValues,
) -> Result<Trace, RunError> {
    let catalog = workshop_rs::catalog::Catalog::builtin_ref()
        .map_err(|e| RunError::Parse(ParseError(e.to_string())))?;
    let program = parse_program(text, catalog).map_err(RunError::Parse)?;
    let scenario = scenario::load(scenario_json, catalog).map_err(RunError::Scenario)?;
    let options = Options {
        seconds: scenario.seconds,
    };
    Ok(sched::run(&program, options, Some(scenario), hero_values))
}
