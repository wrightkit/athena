use std::cell::RefCell;
use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::call::Random;
use crate::scenario::{EventCtx, Player, ServerLoad};
use crate::{Change, MissingInput, MissingInputKind, NotModeled, Stop, Val};

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
    /// The element and place `Current Array Element`/`Current Array Index`
    /// read; an element-context array value binds it per element of its
    /// array argument. `None` outside one.
    pub element: Option<(&'a Val, u64)>,
    /// The `Evaluate Once` pins of the scope this env belongs to, keyed
    /// by the value's site in the parsed program. A rule-instance run
    /// shares its table for the whole run, a rule-condition check gets
    /// a fresh table per pass, and one `Wait Until` call shares a table
    /// across its per-tick re-checks.
    pub once: &'a RefCell<BTreeMap<usize, Val>>,
    /// The `id` of the newest `createEffect` entity; what
    /// `Last Created Entity` reads. `None` before the first one.
    pub last_entity: Option<u64>,
    /// The scenario's `random` source; `None` when the scenario supplies
    /// none, so a `Random *` value is a missing input.
    pub random: Option<&'a RefCell<Random>>,
    /// The scenario's `serverLoad` numbers; `None` when the scenario
    /// supplies none, so a `Server Load *` value is a missing input.
    pub server_load: Option<ServerLoad>,
    /// The catalog `Map` member id the scenario's `map` resolves to;
    /// `None` when the scenario supplies none, so a `Current Map` value
    /// is a missing input.
    pub map: Option<&'a str>,
    /// The index of the rule the evaluated value belongs to — inside a
    /// `Call Subroutine`, the subroutine's own. Recorded on each
    /// `randomDraw` change.
    pub rule: usize,
    /// The tick's recorded changes; a `Random *` value appends its draw.
    pub changes: &'a RefCell<Vec<Change>>,
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

/// The slot names a scenario player's `buttons` map may name — the values
/// [`button_slot`] yields.
pub(crate) const BUTTON_SLOTS: &[&str] = &[
    "ability1",
    "ability2",
    "secondaryFire",
    "primaryFire",
    "ultimate",
    "crouch",
    "interact",
    "jump",
    "melee",
    "reload",
];

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
        Val::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Val::Null => Ok(0.0),
        other => unsupported(format!("arithmetic on {other:?}")),
    }
}

/// The `Compare` equality of two evaluated values: numbers, booleans and
/// `Null` by number coercion (`Null` and `False` equal `0`), text by text,
/// players by scenario `id`, arrays by element-wise equality, vectors by
/// component equality. Values of different types are never equal.
pub(crate) fn values_equal(a: &Val, b: &Val) -> Result<bool, Stop> {
    Ok(match (a, b) {
        (Val::Array(a), Val::Array(b)) => {
            a.len() == b.len()
                && a.iter().zip(b.iter()).try_fold(true, |equal, (a, b)| {
                    Ok::<_, Stop>(equal && values_equal(a, b)?)
                })?
        }
        (Val::Number(_) | Val::Bool(_) | Val::Null, Val::Number(_) | Val::Bool(_) | Val::Null) => {
            number(a)? == number(b)?
        }
        (Val::Text(a), Val::Text(b)) => a == b,
        (Val::Player(a), Val::Player(b)) => a == b,
        (Val::Vector { .. }, Val::Vector { .. }) => a == b,
        _ => false,
    })
}

pub(crate) fn arguments<'a, const N: usize>(
    name: &str,
    args: &'a [Value],
) -> Result<&'a [Value; N], Stop> {
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
        // An enum literal in a value position yields its catalog member
        // id as text — the same form `Hero Of` and hero settings yield,
        // so `Compare(Hero Of(p), ==, Hero(Ana))` compares equal. A
        // `Vector` member is the directional constant it names instead.
        Value::Enum { value_type, value } => {
            if value_type == "Vector" {
                return match crate::vector::constant(value) {
                    Some(constant) => Ok(constant.into()),
                    None => unsupported(format!("value Vector member {value}")),
                };
            }
            Ok(Val::Text(value.clone()))
        }
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
        // `Evaluate Once` pins its operand's first read for the rest of
        // the env's scope, keyed by the site: a `For`/`While`/`Loop`
        // body, a restarted frame or a `Wait Until` re-check reads the
        // site again and gets the pin, while each rule-condition check
        // is a fresh scope. Inside an element-context operand the site
        // is re-read once per element with `Current Array Element`/
        // `Current Array Index` rebound; whether the client's pin
        // survives that is unmeasured, so it stops.
        Value::Call { name, args } if name == "evaluateOnce" => {
            if env.element.is_some() {
                return unsupported("evaluateOnce inside an element context");
            }
            // `evaluateOnce` is handled ahead of `call`, so a sentinel
            // operand is annotated here for the same stop.
            let resolved = crate::ambiguous::resolve_args(name, args);
            let [operand] = arguments::<1>(name, &resolved)?;
            let site = value as *const Value as usize;
            if let Some(pinned) = env.once.borrow().get(&site) {
                return Ok(pinned.clone());
            }
            let pinned = eval(operand, env)?;
            env.once.borrow_mut().insert(site, pinned.clone());
            Ok(pinned)
        }
        Value::Call { name, args } => crate::call::call(name, args, env),
        other => unsupported(format!("value {other:?}")),
    }
}
