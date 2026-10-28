//! The scenario document: players, injected events, expected observations.
//!
//! The format is defined in `docs/scenario-format.md` and
//! `docs/scenario.schema.json`. This module refuses malformed scenarios;
//! a well-formed scenario always runs.

use std::collections::BTreeMap;

use serde::Deserialize;
use workshop_rs::catalog::{Catalog, Locale};

use crate::expect::Expectation;
use crate::state_reads::{state_key, state_value};
use crate::{ScenarioError, Val};

/// The `scenarioVersion` values this simulator runs: version 1 keeps
/// the player's `state` field meaning player variables; version 2
/// moves them to `vars` and makes `state` the player-state map.
const SUPPORTED_VERSIONS: &[u64] = &[1, 2];

/// A scenario `team` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Team {
    Team1,
    Team2,
}

impl Team {
    /// The scenario spelling, used by the trace `team` field and hero
    /// settings group names.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Team1 => "team1",
            Self::Team2 => "team2",
        }
    }

    /// The catalog `Team` member id `Team Of` yields; it compares equal to
    /// a `Team` enum literal, which evaluates to its member id.
    pub(crate) fn member_id(self) -> &'static str {
        match self {
            Self::Team1 => "TEAM_1",
            Self::Team2 => "TEAM_2",
        }
    }
}

/// A player as a scenario declares it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Player {
    /// Hero name; the program may read it, and a missing one is a missing
    /// input.
    pub hero: Option<String>,
    /// The catalog `Hero` member id `hero` names (for example `DVA` for
    /// `D.Va`); `Hero Of` yields it and the hero filter of a rule event
    /// matches on it. `None` when `hero` is absent.
    pub hero_member: Option<String>,
    /// The en-US spellings of the catalog member `hero` names, used to find
    /// the hero in the gameplay dataset for the hero values catalog. Empty
    /// when `hero` is absent.
    pub hero_spellings: Vec<String>,
    /// Team; likewise a live input.
    pub team: Option<Team>,
    /// The player `Host Player` returns; at most one player has it.
    pub host: bool,
    /// The player `Local Player` returns; at most one player has it.
    pub local: bool,
    /// Player variables: the version-1 `state` map, `vars` in
    /// version 2. A variable not listed starts as `null`.
    pub vars: BTreeMap<String, Val>,
    /// The player-state values the scenario's version-2 `state` map
    /// declares, keyed by [`crate::state_reads::STATE_READS`] key; `None` when the field is
    /// absent. A player that declares `state` opts into scenario-supplied
    /// reads: a key the program reads but the map does not list is a
    /// `missing_input`. A player without `state` keeps the established
    /// models (`Health`/`Max Health` pools, the empty world's
    /// `Is In Spawn Room`); every other listed read is a `missing_input`.
    pub state: Option<BTreeMap<String, Val>>,
    /// Held state of buttons from `buttons`, keyed by slot name
    /// (`jump`, `ability1`, …). A slot not listed is a live input the
    /// program cannot read: `Is Button Held` on it is a `missing_input`.
    pub held: BTreeMap<String, bool>,
    /// Values actions wrote into the player's `state` cells, keyed like
    /// `state` (athena#116). A write lands here even on a player that
    /// declared no `state`; a read consults it before `state` and the
    /// no-`state` fallbacks.
    pub cells: BTreeMap<String, Val>,
    /// The modifiers the stored-modifier actions recorded on the player
    /// (athena#116), keyed by the modifier name (`ultEnabled`,
    /// `damageDealt`, `forcedHero`, …): stored state nothing in the
    /// model reads back.
    pub modifiers: BTreeMap<String, Val>,
    /// The `position` the player stands at; `None` when the scenario
    /// omits it, so a position read is a `missing_input`.
    pub position: Option<crate::vector::Vec3>,
    /// The `facing` direction as horizontal/vertical degrees; `None`
    /// when the scenario omits it, so a facing read is a `missing_input`.
    pub facing: Option<crate::vector::Facing>,
    /// The `slot` lobby position `Slot Of` reads and a `Slot N` rule-event
    /// filter matches; `None` when the scenario omits it, so a `Slot Of`
    /// read is a `missing_input`. The loader checks the range and that no
    /// two players share a slot.
    pub slot: Option<u8>,
    /// The player's catalog-derived world state, resolved when the run
    /// starts.
    pub world: crate::state::WorldSlot,
}

/// The runtime context of a player event — injected by the scenario or
/// raised by a modeled state change — while a rule it triggered runs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EventCtx {
    /// The Workshop event name.
    pub kind: String,
    /// The `id` of the event player, if the event declares one.
    pub player: Option<String>,
    /// The event's `values` map.
    pub values: BTreeMap<String, Val>,
    /// `true` when a modeled state change (`Damage`/`Heal`) raised the
    /// event. Only an injected event takes scenario `values`, so an
    /// event value a raised event does not bind is `not_modeled`,
    /// never a missing input.
    pub raised: bool,
}

/// An event the scenario injects.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InjectedEvent {
    /// The Workshop event name, for example `playerDealtFinalBlow`.
    pub kind: String,
    /// Seconds from the first tick.
    pub time: f64,
    /// The `id` of the event player; a live input when the program reads it.
    pub event_player: Option<String>,
    /// Event values by event-value name.
    pub values: BTreeMap<String, Val>,
}

/// A `matchPhase` value: one of the six match phases the catalog's
/// phase reads define — the phases `Is Waiting For Players`,
/// `Is Assembling Heroes`, `Is In Setup`, `Is Game In Progress`,
/// `Is Between Rounds` and `Is Match Complete` describe. Each catalog
/// description names exactly one phase, so a read answers true exactly
/// for its own phase; overtime is no phase the catalog defines, so the
/// field cannot express it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    WaitingForPlayers,
    AssemblingHeroes,
    Setup,
    GameInProgress,
    BetweenRounds,
    MatchComplete,
}

/// The server-load numbers a scenario's `serverLoad` supplies: what
/// `Server Load`, `Server Load Average` and `Server Load Peak` read.
/// The client measures load per tick; the simulator takes it as a
/// constant input — modeled, not measured.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServerLoad {
    /// What `Server Load` reads.
    pub current: f64,
    /// What `Server Load Average` reads.
    pub average: f64,
    /// What `Server Load Peak` reads.
    pub peak: f64,
}

/// A validated scenario.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Scenario {
    /// Simulated seconds, greater than zero.
    pub seconds: f64,
    /// Players by their scenario `id`.
    pub players: BTreeMap<String, Player>,
    /// Injected events in file order.
    pub events: Vec<InjectedEvent>,
    /// Expectations in file order.
    pub expect: Vec<Expectation>,
    /// The `random.seed`: the source `Random *` values draw from.
    /// `None` when the scenario supplies none; a random read is then a
    /// missing input.
    pub random: Option<u64>,
    /// The `serverLoad` numbers; `None` when the scenario supplies none,
    /// so a `Server Load *` read is a missing input.
    pub server_load: Option<ServerLoad>,
    /// The catalog `Map` member id the `map` field names; `None` when the
    /// scenario supplies none, so a `Current Map` read is a missing input.
    pub map: Option<String>,
    /// The `matchPhase` the phase reads answer; `None` when the scenario
    /// supplies none, so a phase read is a missing input.
    pub match_phase: Option<Phase>,
    /// The catalog `Gamemode` member id the `gameMode` field names;
    /// `None` when the scenario supplies none, so a `Current Game Mode`
    /// read is a missing input.
    pub game_mode: Option<String>,
    /// The `objectives` positions `Objective Position` reads, indexed by
    /// the evaluated argument; `None` when the scenario supplies none, so
    /// the read is a missing input.
    pub objectives: Option<Vec<crate::vector::Vec3>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioDoc {
    // Checked before anything else is read; kept so `required` rejects a
    // missing field.
    #[allow(dead_code)]
    #[serde(rename = "scenarioVersion")]
    scenario_version: serde_json::Value,
    seconds: f64,
    players: Vec<PlayerDoc>,
    #[serde(default)]
    events: Vec<EventDoc>,
    #[serde(default)]
    expect: Vec<serde_json::Value>,
    #[serde(default, deserialize_with = "object_or_absent")]
    random: Option<RandomDoc>,
    #[serde(default, rename = "serverLoad", deserialize_with = "object_or_absent")]
    server_load: Option<ServerLoad>,
    map: Option<String>,
    /// The match phase the phase reads report: one of the six
    /// catalog-defined phases; an unknown value is refused.
    #[serde(rename = "matchPhase")]
    match_phase: Option<Phase>,
    /// The game mode `Current Game Mode` reads: a catalog `Gamemode`
    /// member id or en-US spelling, like `map`.
    #[serde(rename = "gameMode")]
    game_mode: Option<String>,
    /// The objective positions `Objective Position` reads, supplied as a
    /// list or not at all.
    #[serde(default, deserialize_with = "object_or_absent")]
    objectives: Option<Vec<PositionDoc>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RandomDoc {
    seed: u64,
}

/// An `Option` field alone reads `null` as absent; these fields are
/// structured, as `events` and `expect` are lists.
fn object_or_absent<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerDoc {
    id: String,
    hero: Option<String>,
    team: Option<TeamDoc>,
    #[serde(default)]
    host: bool,
    /// `true` marks the player `Local Player` returns; like `host`.
    #[serde(default)]
    local: bool,
    /// scenarioVersion 1: the player's variables. scenarioVersion 2:
    /// the player's `state` map. `Option` keeps an absent field apart
    /// from a declared empty map: in version 2 the latter still opts
    /// the player into scenario-supplied reads.
    #[serde(default, deserialize_with = "object_or_absent")]
    state: Option<BTreeMap<String, serde_json::Value>>,
    /// scenarioVersion 2: the player's variables.
    #[serde(default, deserialize_with = "object_or_absent")]
    vars: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(default)]
    buttons: BTreeMap<String, bool>,
    /// The player's position: `{ "x": n, "y": n, "z": n }`, supplied
    /// whole or not at all.
    #[serde(default, deserialize_with = "object_or_absent")]
    position: Option<PositionDoc>,
    /// The player's facing: `{ "horizontal": n, "vertical": n }`
    /// degrees, supplied whole or not at all.
    #[serde(default, deserialize_with = "object_or_absent")]
    facing: Option<FacingDoc>,
    /// The player's lobby slot, an integer from 0 to 11.
    slot: Option<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionDoc {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FacingDoc {
    horizontal: f64,
    vertical: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum TeamDoc {
    Team1,
    Team2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventDoc {
    kind: String,
    time: f64,
    #[serde(rename = "eventPlayer")]
    event_player: Option<String>,
    #[serde(default)]
    values: BTreeMap<String, serde_json::Value>,
}

pub(crate) fn format_error<T>(message: impl Into<String>) -> Result<T, ScenarioError> {
    Err(ScenarioError::Format(message.into()))
}

fn load_error<T>(message: impl Into<String>) -> Result<T, ScenarioError> {
    Err(ScenarioError::Load(message.into()))
}

pub(crate) fn value_to_val(value: &serde_json::Value) -> Result<Val, ScenarioError> {
    Ok(match value {
        serde_json::Value::Null => Val::Null,
        serde_json::Value::Bool(b) => Val::Bool(*b),
        serde_json::Value::Number(n) => Val::Number(
            n.as_f64()
                .ok_or_else(|| ScenarioError::Format(format!("number {n} is too large")))?,
        ),
        serde_json::Value::String(text) => Val::Text(text.clone()),
        serde_json::Value::Array(items) => {
            Val::Array(items.iter().map(value_to_val).collect::<Result<_, _>>()?)
        }
        serde_json::Value::Object(_) => {
            return format_error(format!("{value} is not a scenario value"));
        }
    })
}

/// Every hero name a scenario may give, from the `workshop-rs` catalog: the
/// canonical `Hero` member id (such as `JUNKER_QUEEN`) or an en-US spelling
/// (such as `Junker Queen`), matched case-insensitively. Each accepted name
/// maps to the member id and the member's en-US spellings, which the
/// gameplay dataset shares.
fn hero_names(catalog: &Catalog) -> BTreeMap<String, (String, Vec<String>)> {
    let en_us = Locale::new("en-US");
    let mut names = BTreeMap::new();
    if let Some(domain) = catalog.enum_domain("Hero") {
        for member in &domain.members {
            let spellings = member.spellings(&en_us).to_vec();
            let entry = || (member.member.clone(), spellings.clone());
            names.insert(member.member.to_lowercase(), entry());
            for spelling in &spellings {
                names.insert(spelling.to_lowercase(), entry());
            }
        }
    }
    names
}

/// Every member name a scenario may give for `domain`, from the
/// `workshop-rs` catalog: the canonical member id (such as `KINGS_ROW`)
/// or an en-US spelling (such as `King's Row`), matched
/// case-insensitively — the same form `hero` accepts. Each accepted
/// name maps to the member id.
fn member_names(catalog: &Catalog, domain_name: &str) -> BTreeMap<String, String> {
    let en_us = Locale::new("en-US");
    let mut names = BTreeMap::new();
    if let Some(domain) = catalog.enum_domain(domain_name) {
        for member in &domain.members {
            names.insert(member.member.to_lowercase(), member.member.clone());
            for spelling in member.spellings(&en_us) {
                names.insert(spelling.to_lowercase(), member.member.clone());
            }
        }
    }
    names
}

/// Load a scenario JSON document, checking `scenarioVersion` before any
/// other field is read.
pub(crate) fn load(json: &str, catalog: &Catalog) -> Result<Scenario, ScenarioError> {
    let doc: serde_json::Value =
        serde_json::from_str(json).map_err(|error| ScenarioError::Format(error.to_string()))?;
    let raw_version = doc.get("scenarioVersion");
    let version = raw_version.and_then(|version| version.as_u64());
    let supported = version.is_some_and(|version| SUPPORTED_VERSIONS.contains(&version));
    if !supported {
        let found = raw_version.map_or_else(|| "absent".to_string(), |value| value.to_string());
        return Err(ScenarioError::Version {
            found,
            supported: SUPPORTED_VERSIONS.to_vec(),
        });
    }
    let doc: ScenarioDoc =
        serde_json::from_value(doc).map_err(|error| ScenarioError::Format(error.to_string()))?;

    if doc.seconds <= 0.0 {
        return format_error("`seconds` must be greater than zero");
    }
    let heroes = hero_names(catalog);
    let mut players = BTreeMap::new();
    let mut slots = BTreeMap::new();
    for player in doc.players {
        if player.id.is_empty() {
            return format_error("a player `id` must not be empty");
        }
        let mut hero_member = None;
        let mut hero_spellings = Vec::new();
        if let Some(hero) = &player.hero {
            if hero.is_empty() {
                return format_error(format!("player `{}` has an empty hero", player.id));
            }
            match heroes.get(&hero.to_lowercase()) {
                Some((member, spellings)) => {
                    hero_member = Some(member.clone());
                    hero_spellings = spellings.clone();
                }
                None => {
                    return load_error(format!("player `{}` has unknown hero `{hero}`", player.id));
                }
            }
        }
        // `state` means player variables in version 1 and the
        // player-state map in version 2, where variables moved to
        // `vars`.
        let (var_doc, state_doc) = if version == Some(1) {
            if player.vars.is_some() {
                return format_error(format!(
                    "player `{}` has `vars`, new in scenarioVersion 2",
                    player.id
                ));
            }
            (player.state.as_ref(), None)
        } else {
            (player.vars.as_ref(), player.state.as_ref())
        };
        let mut vars = BTreeMap::new();
        for (name, value) in var_doc.into_iter().flatten() {
            vars.insert(name.clone(), value_to_val(value)?);
        }
        let mut state = None;
        if let Some(state_doc) = state_doc {
            let mut map = BTreeMap::new();
            for (key, value) in state_doc {
                let Some(read) = state_key(key) else {
                    return format_error(format!(
                        "player `{}` has unknown `state.{key}`",
                        player.id
                    ));
                };
                map.insert(read.key.to_string(), state_value(read, &player.id, value)?);
            }
            state = Some(map);
        }
        for slot in player.buttons.keys() {
            if !crate::eval::BUTTON_SLOTS.contains(&slot.as_str()) {
                return load_error(format!(
                    "player `{}` has unknown button `{slot}`",
                    player.id
                ));
            }
        }
        if let Some(slot) = player.slot {
            if slot > 11 {
                return format_error(format!(
                    "player `{}` `slot` must be from 0 to 11",
                    player.id
                ));
            }
            if let Some(first) = slots.insert(slot, player.id.clone()) {
                return load_error(format!(
                    "players `{first}` and `{}` both declare `slot` {slot}",
                    player.id
                ));
            }
        }
        let loaded = Player {
            hero: player.hero,
            hero_member,
            hero_spellings,
            team: player.team.map(|team| match team {
                TeamDoc::Team1 => Team::Team1,
                TeamDoc::Team2 => Team::Team2,
            }),
            host: player.host,
            local: player.local,
            vars,
            state,
            held: player.buttons,
            cells: BTreeMap::new(),
            modifiers: BTreeMap::new(),
            position: player.position.map(|position| crate::vector::Vec3 {
                x: position.x,
                y: position.y,
                z: position.z,
            }),
            facing: player.facing.map(|facing| crate::vector::Facing {
                horizontal: facing.horizontal,
                vertical: facing.vertical,
            }),
            slot: player.slot,
            world: crate::state::WorldSlot::Unresolved,
        };
        if players.insert(player.id.clone(), loaded).is_some() {
            return load_error(format!("player `id` `{}` is declared twice", player.id));
        }
    }

    if players.values().filter(|player| player.host).count() > 1 {
        return load_error("more than one player is marked `host`");
    }
    if players.values().filter(|player| player.local).count() > 1 {
        return load_error("more than one player is marked `local`");
    }

    let map = match &doc.map {
        Some(map) if map.is_empty() => return format_error("`map` must not be empty"),
        Some(map) => match member_names(catalog, "Map").get(&map.to_lowercase()) {
            Some(member) => Some(member.clone()),
            None => return load_error(format!("unknown map `{map}`")),
        },
        None => None,
    };

    let game_mode = match &doc.game_mode {
        Some(mode) if mode.is_empty() => {
            return format_error("`gameMode` must not be empty");
        }
        Some(mode) => match member_names(catalog, "Gamemode").get(&mode.to_lowercase()) {
            Some(member) => Some(member.clone()),
            None => return load_error(format!("unknown gameMode `{mode}`")),
        },
        None => None,
    };

    let mut events = Vec::new();
    for event in doc.events {
        if event.kind.is_empty() {
            return format_error("an event `kind` must not be empty");
        }
        if event.time < 0.0 {
            return format_error(format!("event `{}` has a negative `time`", event.kind));
        }
        if let Some(id) = &event.event_player
            && !players.contains_key(id)
        {
            return load_error(format!(
                "event `{}` names unknown player `{id}`",
                event.kind
            ));
        }
        let mut values = BTreeMap::new();
        for (name, value) in &event.values {
            // The player-typed event values name a scenario player by
            // `id`, like `eventPlayer` itself; `null` stays `null`.
            let value = match (name.as_str(), value) {
                ("attacker" | "victim", serde_json::Value::String(id)) => {
                    if !players.contains_key(id) {
                        return load_error(format!(
                            "event `{}` value `{name}` names unknown player `{id}`",
                            event.kind
                        ));
                    }
                    Val::Player(id.clone())
                }
                ("attacker" | "victim", serde_json::Value::Null) => Val::Null,
                ("attacker" | "victim", _) => {
                    return load_error(format!(
                        "event `{}` value `{name}` must be a player `id` or `null`",
                        event.kind
                    ));
                }
                _ => value_to_val(value)?,
            };
            values.insert(name.clone(), value);
        }
        events.push(InjectedEvent {
            kind: event.kind,
            time: event.time,
            event_player: event.event_player,
            values,
        });
    }

    let mut expect = Vec::new();
    for (index, value) in doc.expect.iter().enumerate() {
        let expectation = crate::expect::expectation(index, value)?;
        if let Expectation::PlayerVariable { player, .. } = &expectation
            && !players.contains_key(player)
        {
            return load_error(format!("expect[{index}] names unknown player `{player}`"));
        }
        expect.push(expectation);
    }

    Ok(Scenario {
        seconds: doc.seconds,
        players,
        events,
        expect,
        random: doc.random.map(|random| random.seed),
        server_load: doc.server_load,
        map,
        match_phase: doc.match_phase,
        game_mode,
        objectives: doc.objectives.map(|objectives| {
            objectives
                .into_iter()
                .map(|position| crate::vector::Vec3 {
                    x: position.x,
                    y: position.y,
                    z: position.z,
                })
                .collect()
        }),
    })
}
