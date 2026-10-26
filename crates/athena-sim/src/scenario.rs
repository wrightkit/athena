//! The scenario document: players, injected events, expected observations.
//!
//! The format is defined in `docs/scenario-format.md` and
//! `docs/scenario.schema.json`. This module refuses malformed scenarios;
//! a well-formed scenario always runs.

use std::collections::BTreeMap;

use serde::Deserialize;
use workshop_rs::catalog::{Catalog, Locale};

use crate::{ScenarioError, Val};

/// The `scenarioVersion` values this simulator runs: version 1 keeps
/// the player's `state` field meaning player variables; version 2
/// moves them to `vars` and makes `state` the player-state map.
const SUPPORTED_VERSIONS: &[u64] = &[1, 2];

/// The JSON type a `state` key takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateType {
    Bool,
    Number,
    Vector,
}

impl StateType {
    /// The article-and-name a malformed-scenario error reports.
    fn desc(self) -> &'static str {
        match self {
            Self::Bool => "a boolean",
            Self::Number => "a number",
            Self::Vector => "a vector",
        }
    }
}

/// One row of the player-state table (athena#89): the no-extra-argument
/// player reads a version-2 `state` map answers. `id` is the catalog
/// value id the program spells, `key` the `state` map key — the `id`
/// minus a `get` prefix — and `ty` the key's JSON type. A scenario
/// supplies every value; nothing is derived or defaulted except
/// `Speed Of`, which is the length of `velocity` (`derived`), while
/// `Is Moving` is supplied, not derived.
pub(crate) struct StateRead {
    /// The catalog value id the program spells (`getHealth`).
    pub id: &'static str,
    /// The `state` key it reads (`health`).
    pub key: &'static str,
    /// The key's JSON type.
    pub ty: StateType,
    /// `getSpeed` alone: the read is the key's vector length, not the
    /// stored value.
    pub derived: bool,
}

/// The player-state table: one row per modeled no-extra-argument
/// player read. Adding a read is one row plus a test.
pub(crate) const STATE_READS: &[StateRead] = &[
    StateRead {
        id: "getHealth",
        key: "health",
        ty: StateType::Number,
        derived: false,
    },
    StateRead {
        id: "getMaxHealth",
        key: "maxHealth",
        ty: StateType::Number,
        derived: false,
    },
    StateRead {
        id: "getNormalizedHealth",
        key: "normalizedHealth",
        ty: StateType::Number,
        derived: false,
    },
    StateRead {
        id: "getVelocity",
        key: "velocity",
        ty: StateType::Vector,
        derived: false,
    },
    StateRead {
        id: "getSpeed",
        key: "velocity",
        ty: StateType::Number,
        derived: true,
    },
    StateRead {
        id: "getEyePosition",
        key: "eyePosition",
        ty: StateType::Vector,
        derived: false,
    },
    StateRead {
        id: "getUltCharge",
        key: "ultCharge",
        ty: StateType::Number,
        derived: false,
    },
    StateRead {
        id: "isDead",
        key: "isDead",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isUsingUltimate",
        key: "isUsingUltimate",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isMeleeing",
        key: "isMeleeing",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isFiringSecondary",
        key: "isFiringSecondary",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isInAlternateForm",
        key: "isInAlternateForm",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isCommunicatingEmote",
        key: "isCommunicatingEmote",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isInSpawnRoom",
        key: "isInSpawnRoom",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isDummy",
        key: "isDummy",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isMoving",
        key: "isMoving",
        ty: StateType::Bool,
        derived: false,
    },
];

/// The read a value `id` names, when `id` is a player-state value.
pub(crate) fn state_read(id: &str) -> Option<&'static StateRead> {
    STATE_READS.iter().find(|read| read.id == id)
}

/// The table row a `state` map `key` declares, when `key` is a known
/// `state` key. `derived` rows name no key of their own.
pub(crate) fn state_key(key: &str) -> Option<&'static StateRead> {
    STATE_READS
        .iter()
        .find(|read| read.key == key && !read.derived)
}

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
    /// Player variables: the version-1 `state` map, `vars` in
    /// version 2. A variable not listed starts as `null`.
    pub vars: BTreeMap<String, Val>,
    /// The player-state values the scenario's version-2 `state` map
    /// declares, keyed by [`STATE_READS`] key; `None` when the field is
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
    /// The `position` the player stands at; `None` when the scenario
    /// omits it, so a position read is a `missing_input`.
    pub position: Option<crate::vector::Vec3>,
    /// The `facing` direction as horizontal/vertical degrees; `None`
    /// when the scenario omits it, so a facing read is a `missing_input`.
    pub facing: Option<crate::vector::Facing>,
    /// The player's catalog-derived world state, resolved when the run
    /// starts.
    pub world: crate::state::WorldSlot,
}

/// The runtime context of an injected event while a rule it triggered runs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EventCtx {
    /// The Workshop event name.
    pub kind: String,
    /// The `id` of the event player, if the event declares one.
    pub player: Option<String>,
    /// The event's `values` map.
    pub values: BTreeMap<String, Val>,
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

/// One expected observation checked at the end of a complete run.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expectation {
    Global {
        name: String,
        value: Val,
    },
    PlayerVariable {
        player: String,
        name: String,
        value: Val,
    },
    Log {
        tick: u64,
        text: String,
    },
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
    /// scenarioVersion 1: the player's variables. scenarioVersion 2:
    /// the player's `state` map. `Option` keeps an absent field apart
    /// from a declared empty map: in version 2 the latter still opts
    /// the player into scenario-supplied reads.
    state: Option<BTreeMap<String, serde_json::Value>>,
    /// scenarioVersion 2: the player's variables.
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

fn format_error<T>(message: impl Into<String>) -> Result<T, ScenarioError> {
    Err(ScenarioError::Format(message.into()))
}

fn load_error<T>(message: impl Into<String>) -> Result<T, ScenarioError> {
    Err(ScenarioError::Load(message.into()))
}

/// The `{ "x": n, "y": n, "z": n }` a `state` vector takes; anything
/// else — a missing or extra key, a non-number component — is malformed.
fn state_vector(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<crate::vector::Vec3> {
    if object.len() != 3 {
        return None;
    }
    Some(crate::vector::Vec3 {
        x: object.get("x")?.as_f64()?,
        y: object.get("y")?.as_f64()?,
        z: object.get("z")?.as_f64()?,
    })
}

/// One `state` map entry to its `Val`; a wrong JSON type is malformed
/// naming the key.
fn state_value(
    read: &StateRead,
    player: &str,
    value: &serde_json::Value,
) -> Result<Val, ScenarioError> {
    let mistyped = || {
        ScenarioError::Format(format!(
            "player `{player}` `state.{}` must be {}",
            read.key,
            read.ty.desc()
        ))
    };
    Ok(match (read.ty, value) {
        (StateType::Bool, serde_json::Value::Bool(b)) => Val::Bool(*b),
        (StateType::Number, serde_json::Value::Number(n)) => Val::Number(
            n.as_f64()
                .ok_or_else(|| ScenarioError::Format(format!("number {n} is too large")))?,
        ),
        (StateType::Vector, serde_json::Value::Object(object)) => match state_vector(object) {
            Some(vector) => vector.into(),
            None => return Err(mistyped()),
        },
        _ => return Err(mistyped()),
    })
}

fn value_to_val(value: &serde_json::Value) -> Result<Val, ScenarioError> {
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

fn named_id(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ScenarioError> {
    match object.get(key).and_then(|value| value.as_str()) {
        Some(text) if !text.is_empty() => Ok(text.to_string()),
        Some(_) => format_error(format!("`{key}` must not be empty")),
        None => format_error(format!("`{key}` is missing or not a text")),
    }
}

fn expectation(index: usize, value: &serde_json::Value) -> Result<Expectation, ScenarioError> {
    let Some(object) = value.as_object() else {
        return format_error(format!("expect[{index}] is not an object"));
    };
    let kind = object.get("kind").and_then(|kind| kind.as_str());
    let allowed: &[&str] = match kind {
        Some("global") => &["kind", "name", "value"],
        Some("playerVariable") => &["kind", "player", "name", "value"],
        Some("log") => &["kind", "tick", "text"],
        other => {
            return format_error(format!(
                "expect[{index}].kind is {other:?}, expected `global`, `playerVariable` or `log`"
            ));
        }
    };
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return format_error(format!("expect[{index}] has an unknown field `{key}`"));
        }
    }
    match kind {
        Some("global") => Ok(Expectation::Global {
            name: named_id(object, "name")?,
            value: value_to_val(
                object
                    .get("value")
                    .ok_or_else(|| ScenarioError::Format("expect value is missing".into()))?,
            )?,
        }),
        Some("playerVariable") => Ok(Expectation::PlayerVariable {
            player: named_id(object, "player")?,
            name: named_id(object, "name")?,
            value: value_to_val(
                object
                    .get("value")
                    .ok_or_else(|| ScenarioError::Format("expect value is missing".into()))?,
            )?,
        }),
        _ => {
            let Some(serde_json::Value::Number(tick)) = object.get("tick") else {
                return format_error(format!("expect[{index}].tick is missing or not an integer"));
            };
            let Some(tick) = tick.as_u64() else {
                return format_error(format!("expect[{index}].tick is missing or not an integer"));
            };
            match object.get("text").and_then(|text| text.as_str()) {
                Some(text) => Ok(Expectation::Log {
                    tick,
                    text: text.to_string(),
                }),
                None => format_error(format!("expect[{index}].text is missing or not a text")),
            }
        }
    }
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

/// Every map name a scenario may give, from the `workshop-rs` catalog: the
/// canonical `Map` member id (such as `KINGS_ROW`) or an en-US spelling
/// (such as `King's Row`), matched case-insensitively — the same form
/// `hero` accepts. Each accepted name maps to the member id.
fn map_names(catalog: &Catalog) -> BTreeMap<String, String> {
    let en_us = Locale::new("en-US");
    let mut names = BTreeMap::new();
    if let Some(domain) = catalog.enum_domain("Map") {
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
        let loaded = Player {
            hero: player.hero,
            hero_member,
            hero_spellings,
            team: player.team.map(|team| match team {
                TeamDoc::Team1 => Team::Team1,
                TeamDoc::Team2 => Team::Team2,
            }),
            host: player.host,
            vars,
            state,
            held: player.buttons,
            position: player.position.map(|position| crate::vector::Vec3 {
                x: position.x,
                y: position.y,
                z: position.z,
            }),
            facing: player.facing.map(|facing| crate::vector::Facing {
                horizontal: facing.horizontal,
                vertical: facing.vertical,
            }),
            world: crate::state::WorldSlot::Unresolved,
        };
        if players.insert(player.id.clone(), loaded).is_some() {
            return load_error(format!("player `id` `{}` is declared twice", player.id));
        }
    }

    if players.values().filter(|player| player.host).count() > 1 {
        return load_error("more than one player is marked `host`");
    }

    let map = match &doc.map {
        Some(map) if map.is_empty() => return format_error("`map` must not be empty"),
        Some(map) => match map_names(catalog).get(&map.to_lowercase()) {
            Some(member) => Some(member.clone()),
            None => return load_error(format!("unknown map `{map}`")),
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
            values.insert(name.clone(), value_to_val(value)?);
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
        let expectation = expectation(index, value)?;
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
