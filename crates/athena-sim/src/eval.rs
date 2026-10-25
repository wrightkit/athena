use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::scenario::{EventCtx, Player, Team};
use crate::state::WorldSlot;
use crate::{MissingInput, MissingInputKind, NotModeled, PlayerWorld, PoolState, Stop, Val};

pub(crate) struct Env<'a> {
    pub globals: &'a BTreeMap<String, Val>,
    pub players: &'a BTreeMap<String, Player>,
    /// The injected event the running rule was triggered by, if any.
    pub event: Option<&'a EventCtx>,
    /// The player an `Ongoing - Each Player` instance runs for; `Event
    /// Player` reads it when no injected event supplies one.
    pub each_player: Option<&'a str>,
    /// The `id` of the newest `hudText`/`progressBarHud` element; what
    /// `Last Text ID` reads. `None` before the first one is created.
    pub last_text_id: Option<u64>,
    pub now: f64,
}

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotModeled(NotModeled::new(what)))
}

fn missing<T>(
    input: MissingInputKind,
    player: Option<String>,
    name: Option<String>,
    construct: impl Into<String>,
) -> Result<T, Stop> {
    Err(Stop::MissingInput(MissingInput::new(
        input, player, name, construct,
    )))
}

/// Event-scoped values the simulator resolves from an injected event's
/// `values` map. Player-, enum- and vector-typed event values (`attacker`,
/// `victim`, `hostPlayer`, `eventAbility`, `eventDirection`) are not modeled.
const EVENT_VALUES: &[&str] = &[
    "eventDamage",
    "eventHealing",
    "eventWasCriticalHit",
    "eventWasEnvironment",
    "eventWasHealthPack",
];

/// Resolve a Workshop player expression to a scenario player `id`.
pub(crate) fn player_id(value: &Value, env: &Env) -> Result<String, Stop> {
    match eval(value, env)? {
        Val::Player(id) => Ok(id),
        other => unsupported(format!("{other:?} where a player is needed")),
    }
}

/// Resolve a `Player|Array` argument, as `Damage` and `Heal` take, to the
/// `id`s of the players it names.
pub(crate) fn player_ids(value: &Value, env: &Env) -> Result<Vec<String>, Stop> {
    match eval(value, env)? {
        Val::Player(id) => Ok(vec![id]),
        Val::Array(items) => items
            .iter()
            .map(|item| match item {
                Val::Player(id) => Ok(id.clone()),
                other => unsupported(format!("{other:?} where a player is needed")),
            })
            .collect(),
        other => unsupported(format!("{other:?} where a player is needed")),
    }
}

/// Resolve a Workshop "players" expression — the `Visible To`/`player`
/// argument of a HUD action — to scenario player `id`s. A single player,
/// an array of players (what `allPlayers` produces), or `Null` (nobody).
pub(crate) fn viewers(value: &Value, env: &Env) -> Result<Vec<String>, Stop> {
    match eval(value, env)? {
        Val::Player(id) => Ok(vec![id]),
        Val::Array(items) => items
            .iter()
            .map(|item| match item {
                Val::Player(id) => Ok(id.clone()),
                other => unsupported(format!("{other:?} where a player is needed")),
            })
            .collect(),
        Val::Null => Ok(Vec::new()),
        other => unsupported(format!("{other:?} where players are needed")),
    }
}

/// The resolved world state of a scenario player, as the player-state
/// reading values (`Health`, `Max Health`, `Ability Cooldown`) see it. A
/// player the scenario did not declare, or whose state failed to resolve at
/// run start, is reported, never defaulted — reading world state is a touch,
/// like `Damage`/`Heal`.
fn player_world<'a>(
    players: &'a BTreeMap<String, Player>,
    id: &str,
) -> Result<&'a PlayerWorld, Stop> {
    let Some(player) = players.get(id) else {
        return unsupported(format!("`{id}` is not a scenario player"));
    };
    match &player.world {
        WorldSlot::Resolved(state) => Ok(&state.world),
        WorldSlot::Failed(error) => Err(Stop::NotModeled(error.clone())),
        WorldSlot::Unresolved => unreachable!("player world state is resolved at run start"),
    }
}

/// The `Button` enum member to the cooldown slot name the player-state
/// table reports it under.
pub(crate) fn button_slot(button: &Value) -> Result<&'static str, Stop> {
    let Value::Enum { value, .. } = button else {
        return unsupported(format!("{button:?} where a button is needed"));
    };
    Ok(match value.as_str() {
        "ABILITY_1" => "ability1",
        "ABILITY_2" => "ability2",
        "SECONDARY_FIRE" => "secondaryFire",
        "PRIMARY_FIRE" => "primaryFire",
        "ULTIMATE" => "ultimate",
        "CROUCH" => "crouch",
        "INTERACT" => "interact",
        "JUMP" => "jump",
        "MELEE" => "melee",
        "RELOAD" => "reload",
        other => return unsupported(format!("button {other}")),
    })
}

pub(crate) fn number(value: &Val) -> Result<f64, Stop> {
    match value {
        Val::Number(n) => Ok(*n),
        Val::Null => Ok(0.0),
        other => unsupported(format!("arithmetic on {other:?}")),
    }
}

fn render(value: &Val) -> Result<String, Stop> {
    match value {
        Val::Number(n) => Ok(format!("{n:.2}")),
        Val::Null => Ok("0".into()),
        Val::Bool(b) => Ok(b.to_string()),
        Val::Text(text) => Ok(text.clone()),
        Val::Array(_) => unsupported("array inside a custom string"),
        Val::Player(_) => unsupported("player inside a custom string"),
    }
}

fn arguments<'a, const N: usize>(name: &str, args: &'a [Value]) -> Result<&'a [Value; N], Stop> {
    args.try_into()
        .or_else(|_| unsupported(format!("{name} with {} arguments", args.len())))
}

pub(crate) fn eval(value: &Value, env: &Env) -> Result<Val, Stop> {
    match value {
        Value::Number(n) => Ok(Val::Number(*n)),
        Value::Bool(b) => Ok(Val::Bool(*b)),
        Value::Null => Ok(Val::Null),
        Value::String(text) => Ok(Val::Text(text.clone())),
        Value::Array(items) => items
            .iter()
            .map(|item| eval(item, env))
            .collect::<Result<_, _>>()
            .map(Val::Array),
        Value::GlobalVariable(name) => Ok(env.globals.get(name).cloned().unwrap_or(Val::Null)),
        Value::PlayerVariable { player, variable } => {
            let id = player_id(player, env)?;
            Ok(env
                .players
                .get(&id)
                .and_then(|player| player.vars.get(variable))
                .cloned()
                .unwrap_or(Val::Null))
        }
        Value::EventPlayer => match env
            .event
            .and_then(|event| event.player.as_deref())
            .or(env.each_player)
        {
            Some(id) => Ok(Val::Player(id.to_string())),
            None => missing(
                MissingInputKind::EventPlayer,
                None,
                None,
                "value eventPlayer",
            ),
        },
        Value::Call { name, args } => call(name, args, env),
        other => unsupported(format!("value {other:?}")),
    }
}

fn call(name: &str, args: &[Value], env: &Env) -> Result<Val, Stop> {
    let binary = |op: fn(f64, f64) -> f64| -> Result<Val, Stop> {
        let [a, b] = arguments::<2>(name, args)?;
        Ok(Val::Number(op(
            number(&eval(a, env)?)?,
            number(&eval(b, env)?)?,
        )))
    };
    match name {
        "getTotalTimeElapsed" => Ok(Val::Number(env.now)),
        "lastTextId" => {
            let [] = arguments::<0>(name, args)?;
            Ok(env
                .last_text_id
                .map_or(Val::Null, |id| Val::Number(id as f64)))
        }
        "allPlayers" => {
            let [team] = arguments::<1>(name, args)?;
            // The `Team` argument is an enum literal in practice; an
            // evaluated `team1`/`team2` text (as `Team Of` yields) resolves
            // the same group.
            let filter = match team {
                Value::Enum { value, .. } => value.clone(),
                other => match eval(other, env)? {
                    Val::Text(text) => match text.as_str() {
                        "team1" => "TEAM_1".to_string(),
                        "team2" => "TEAM_2".to_string(),
                        _ => return unsupported(format!("allPlayers on {text}")),
                    },
                    other => return unsupported(format!("allPlayers on {other:?}")),
                },
            };
            let members = |team: Team| {
                env.players
                    .iter()
                    .filter(|(_, player)| player.team == Some(team))
                    .map(|(id, _)| Val::Player(id.clone()))
                    .collect()
            };
            Ok(Val::Array(match filter.as_str() {
                "ALL" => env
                    .players
                    .keys()
                    .map(|id| Val::Player(id.clone()))
                    .collect(),
                "TEAM_1" => members(Team::Team1),
                "TEAM_2" => members(Team::Team2),
                other => return unsupported(format!("allPlayers on {other}")),
            }))
        }
        "and" => {
            let [a, b] = arguments::<2>(name, args)?;
            // `&&` short-circuits: the right side is not read when the left
            // is false.
            let Val::Bool(left) = eval(a, env)? else {
                return unsupported("`&&` on a non-boolean");
            };
            if !left {
                return Ok(Val::Bool(false));
            }
            match eval(b, env)? {
                Val::Bool(right) => Ok(Val::Bool(right)),
                _ => unsupported("`&&` on a non-boolean"),
            }
        }
        "hostPlayer" => {
            let [] = arguments::<0>(name, args)?;
            match env.players.iter().find(|(_, player)| player.host) {
                Some((id, _)) => Ok(Val::Player(id.clone())),
                None => unsupported("Host Player: the scenario marks no host player"),
            }
        }
        // Modeled, not measured: the simulator runs one lobby whose settings
        // nobody edits, so a Workshop Setting yields its declared `default`
        // argument. `min`/`max`, `options` and `sortOrder` are authoring
        // metadata the running rules never see; the scenario format has no
        // settings overrides.
        "createWorkshopSettingFloat"
        | "workshopSettingInteger"
        | "workshopSettingToggle"
        | "workshopSettingCombo"
        | "createWorkshopSettingHero" => {
            let [_, _, default, ..] = args else {
                return unsupported(format!("{name} with {} arguments", args.len()));
            };
            match default {
                // A hero setting's default is a `Hero` literal; it yields the
                // catalog member id as text, as `Hero Of` yields the hero name.
                Value::Enum { value, .. } => Ok(Val::Text(value.clone())),
                _ => eval(default, env),
            }
        }
        // Modeled, not measured: a player with a hero is spawned and alive.
        "hasSpawned" | "isAlive" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            match env.players.get(&id) {
                Some(player) if player.hero.is_some() => Ok(Val::Bool(true)),
                Some(_) => unsupported(format!("{name} of player `{id}` without a hero")),
                None => unsupported(format!("`{id}` is not a scenario player")),
            }
        }
        "add" => binary(|a, b| a + b),
        "subtract" => binary(|a, b| a - b),
        "multiply" => binary(|a, b| a * b),
        "<" | ">=" => {
            let [a, b] = arguments::<2>(name, args)?;
            let (a, b) = (number(&eval(a, env)?)?, number(&eval(b, env)?)?);
            Ok(Val::Bool(if name == "<" { a < b } else { a >= b }))
        }
        "==" => {
            let [a, b] = arguments::<2>(name, args)?;
            let (a, b) = (eval(a, env)?, eval(b, env)?);
            Ok(Val::Bool(match (&a, &b) {
                (Val::Number(_) | Val::Null, Val::Number(_) | Val::Null) => {
                    number(&a)? == number(&b)?
                }
                (Val::Bool(a), Val::Bool(b)) => a == b,
                (Val::Text(a), Val::Text(b)) => a == b,
                (Val::Player(a), Val::Player(b)) => a == b,
                (Val::Array(_), _) | (_, Val::Array(_)) => {
                    return unsupported("== on an array");
                }
                _ => false,
            }))
        }
        "array" => args
            .iter()
            .map(|item| eval(item, env))
            .collect::<Result<_, _>>()
            .map(Val::Array),
        "valueInArray" => {
            let [array, index] = arguments::<2>(name, args)?;
            let (array, index) = (eval(array, env)?, number(&eval(index, env)?)?);
            match array {
                Val::Array(items) if index >= 0.0 => {
                    Ok(items.get(index as usize).cloned().unwrap_or(Val::Null))
                }
                Val::Array(_) => Ok(Val::Null),
                other => unsupported(format!("valueInArray on {other:?}")),
            }
        }
        "getHero" | "teamOf" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            let declared = env.players.get(&id).and_then(|player| match name {
                "getHero" => player.hero.clone(),
                _ => player.team.map(|team| team.as_str().to_string()),
            });
            match declared {
                Some(value) => Ok(Val::Text(value)),
                None => missing(
                    if name == "getHero" {
                        MissingInputKind::Hero
                    } else {
                        MissingInputKind::Team
                    },
                    Some(id),
                    None,
                    format!("value {name}"),
                ),
            }
        }
        name if EVENT_VALUES.contains(&name) => {
            let [] = arguments::<0>(name, args)?;
            match env.event.and_then(|event| event.values.get(name)).cloned() {
                Some(value) => Ok(value),
                None => missing(
                    MissingInputKind::EventValue,
                    None,
                    Some(name.to_string()),
                    format!("value {name}"),
                ),
            }
        }
        "getHealth" | "getMaxHealth" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            let world = player_world(env.players, &id)?;
            let pools = [
                Some(&world.health),
                world.shield.as_ref(),
                world.armor.as_ref(),
            ];
            let pick = if name == "getHealth" {
                |pool: &PoolState| pool.current
            } else {
                |pool: &PoolState| pool.max
            };
            Ok(Val::Number(pools.into_iter().flatten().map(pick).sum()))
        }
        "getAbilityCooldown" => {
            let [player, button] = arguments::<2>(name, args)?;
            let id = player_id(player, env)?;
            let slot = button_slot(button)?;
            let world = player_world(env.players, &id)?;
            if let Some(seconds) = world.cooldowns.get(slot) {
                return Ok(Val::Number(*seconds));
            }
            // A slot recorded only under a variant (`slot/variant`) has no
            // honest answer for the bare button; a slot with no recorded
            // cooldown at all reports 0 — the ability is never on cooldown.
            if world
                .cooldowns
                .keys()
                .any(|key| key.starts_with(&format!("{slot}/")))
            {
                return unsupported(format!("ability cooldown on variant-keyed slot {slot}"));
            }
            Ok(Val::Number(0.0))
        }
        "customString" => {
            let [template, rest @ ..] = args else {
                return unsupported("customString without a template");
            };
            let Val::Text(mut text) = eval(template, env)? else {
                return unsupported("customString template that is not a string");
            };
            let mut rendered = Vec::new();
            for arg in rest {
                rendered.push(render(&eval(arg, env)?)?);
            }
            for (index, value) in rendered.iter().enumerate() {
                text = text.replace(&format!("{{{index}}}"), value);
            }
            Ok(Val::Text(text))
        }
        other => unsupported(format!("value {other}")),
    }
}
