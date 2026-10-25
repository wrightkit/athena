//! The scenario document: players, injected events, expected observations.
//!
//! The format is defined in `docs/scenario-format.md` and
//! `docs/scenario.schema.json`. This module refuses malformed scenarios;
//! a well-formed scenario always runs.

use std::collections::BTreeMap;

use serde::Deserialize;
use workshop_rs::catalog::{Catalog, Locale};

use crate::{ScenarioError, Val};

/// The `scenarioVersion` values this simulator runs.
const SUPPORTED_VERSIONS: &[u64] = &[1];

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
    /// Player variables from `state`. A variable not listed starts as
    /// `null`.
    pub vars: BTreeMap<String, Val>,
    /// Held state of buttons from `buttons`, keyed by slot name
    /// (`jump`, `ability1`, …). A slot not listed is a live input the
    /// program cannot read: `Is Button Held` on it is a `missing_input`.
    pub held: BTreeMap<String, bool>,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerDoc {
    id: String,
    hero: Option<String>,
    team: Option<TeamDoc>,
    #[serde(default)]
    host: bool,
    #[serde(default)]
    state: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    buttons: BTreeMap<String, bool>,
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

/// Load a scenario JSON document, checking `scenarioVersion` before any
/// other field is read.
pub(crate) fn load(json: &str, catalog: &Catalog) -> Result<Scenario, ScenarioError> {
    let doc: serde_json::Value =
        serde_json::from_str(json).map_err(|error| ScenarioError::Format(error.to_string()))?;
    let version = doc.get("scenarioVersion");
    let supported = version
        .and_then(|version| version.as_u64())
        .is_some_and(|version| SUPPORTED_VERSIONS.contains(&version));
    if !supported {
        let found = version.map_or_else(|| "absent".to_string(), |value| value.to_string());
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
        let mut vars = BTreeMap::new();
        for (name, value) in &player.state {
            vars.insert(name.clone(), value_to_val(value)?);
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
            held: player.buttons,
            world: crate::state::WorldSlot::Unresolved,
        };
        if players.insert(player.id.clone(), loaded).is_some() {
            return load_error(format!("player `id` `{}` is declared twice", player.id));
        }
    }

    if players.values().filter(|player| player.host).count() > 1 {
        return load_error("more than one player is marked `host`");
    }

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
    })
}
