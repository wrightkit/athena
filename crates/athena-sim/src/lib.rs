mod eval;
mod hero_settings;
mod hud;
mod scenario;
mod sched;
mod state;

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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub tick: u64,
    pub text: String,
}

/// A construct the simulator does not model. The run stops at it; nothing is
/// defaulted or skipped.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotModeled {
    /// What was met, for example `action wait` or `value getPosition`.
    pub construct: String,
    /// The tick on which it was met; `None` when found before the first tick.
    pub tick: Option<u64>,
}

impl NotModeled {
    pub(crate) fn new(construct: impl Into<String>) -> Self {
        Self {
            construct: construct.into(),
            tick: None,
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
    /// The event-value name for `eventValue`; `None` for the others.
    pub name: Option<String>,
    /// The tick on which the program read it.
    pub tick: Option<u64>,
    /// A short description for people, as in `not_modeled`. Do not match on
    /// it.
    pub construct: String,
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

/// A player's catalog-derived world state: health pools, ability cooldowns
/// and ultimate charge. Start values come from `workshop_rs::hero_values` at
/// `CURRENT_VERSION`; `Damage` and `Heal` move `current` inside a pool and
/// nothing else changes it (no ability use is simulated).
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
}

/// A player's row in the state table the trace reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerState {
    /// The scenario's `hero` text, `None` when undeclared.
    pub hero: Option<String>,
    /// `team1` or `team2`, `None` when undeclared.
    pub team: Option<String>,
    /// The catalog-derived world state, `None` when the run could not
    /// resolve it (a player without a `hero`, or a hero value the catalog
    /// lacks). Touching an unresolved player's world state ends the run as
    /// `not_modeled`.
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

/// The evaluated arguments a HUD element records. A field is present only
/// when the creating action takes that argument. Value arguments serialize
/// as [`Val`]; enum arguments (location, colors, reevaluation, spectator
/// visibility) serialize as the catalog member id.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subheader_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_bar_color: Option<String>,
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

/// What a run produced. `logs`, `globals`, `players` and `hud` describe the
/// state when the run ended: after the last tick for `Complete`, at the stop
/// for `NotModeled` and `MissingInput`. Consumers must check `outcome`
/// before using any of them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trace {
    pub outcome: Outcome,
    pub logs: Vec<LogEntry>,
    pub globals: BTreeMap<String, Val>,
    /// The player state table: one row per scenario player. Empty when the
    /// run had no scenario.
    pub players: BTreeMap<String, PlayerState>,
    /// The HUD elements the run created, in creation order.
    pub hud: Vec<HudElement>,
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

/// Why a run stopped before its time was up.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stop {
    NotModeled(NotModeled),
    MissingInput(MissingInput),
}

impl Stop {
    pub(crate) fn at(self, tick: u64) -> Self {
        match self {
            Self::NotModeled(not_modeled) => Self::NotModeled(not_modeled.at(tick)),
            Self::MissingInput(missing) => Self::MissingInput(missing.at(tick)),
        }
    }
}

impl From<NotModeled> for Stop {
    fn from(not_modeled: NotModeled) -> Self {
        Self::NotModeled(not_modeled)
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
