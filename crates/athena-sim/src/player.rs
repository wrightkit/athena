//! Player-roster and player-state values: `All Players`, `Host Player`,
//! `All Heroes`, the `Hero Of`/`Team Of`, `Is Hero Being Played` and
//! `Players On Hero` reads, spawn/alive state, the
//! cooldown/status/button reads a scenario player's resolved world
//! state answers, and the no-extra-argument reads a scenario player's
//! `state` map answers.

use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::eval::{Env, arguments, button_slot, eval, player_id};
use crate::scenario::{Player, Team};
use crate::state::WorldSlot;
use crate::{MissingInput, MissingInputKind, NotModeled, PlayerWorld, PoolState, Stop, Val};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(NotModeled::new(what).into())
}

fn missing<T>(
    input: MissingInputKind,
    player: Option<String>,
    name: Option<String>,
    construct: impl Into<String>,
) -> Result<T, Stop> {
    Err(MissingInput::new(input, player, name, construct).into())
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
        WorldSlot::Failed(error) => Err(error.clone().into()),
        WorldSlot::Unresolved => unreachable!("player world state is resolved at run start"),
    }
}

/// The catalog member id a `Hero`- or `Team`-parameter argument names:
/// an enum literal's member id, or the member-id text an evaluation
/// yields — the same form `Hero Of`/`Team Of` produce. The result must
/// name a member of the parameter's domain; anything else stops the
/// run as `not_modeled`, naming the argument's parameter position.
fn member_id(
    name: &str,
    param: &str,
    domain: &str,
    arg: &Value,
    env: &Env,
) -> Result<String, Stop> {
    let member = match arg {
        Value::Enum { value, .. } => value.clone(),
        other => match eval(other, env)? {
            Val::Text(text) => text,
            other => return unsupported(format!("{name} `{param}` argument on {other:?}")),
        },
    };
    let Some(domain) = workshop_rs::catalog::Catalog::builtin_ref()
        .map_err(|error| NotModeled::new(format!("catalog unavailable: {error}")))?
        .enum_domain(domain)
    else {
        return unsupported(format!("catalog has no `{domain}` domain"));
    };
    if domain.members.iter().any(|entry| entry.member == member) {
        Ok(member)
    } else {
        unsupported(format!("{name} `{param}` argument `{member}`"))
    }
}

/// The `(hero, team)` member ids a roster filter takes, each argument
/// resolved the way `Hero Of`/`Team Of` yield theirs — an enum literal's
/// member id or an evaluated member-id text.
fn hero_team_args(name: &str, args: &[Value], env: &Env) -> Result<(String, String), Stop> {
    let [hero, team] = arguments::<2>(name, args)?;
    let hero = member_id(name, "hero", "Hero", hero, env)?;
    let team = member_id(name, "team", "Team", team, env)?;
    Ok((hero, team))
}

/// Visit the scenario players on `team` in roster order — `All Teams`
/// visits every player — calling `visit` with each player's id and
/// declared-hero member; `visit` returns `true` to end the visit early.
/// A player on the team without a declared `hero` ends `missing_input`
/// naming that player's `hero` — the same read `Hero Of` makes — before
/// `visit` sees it.
fn visit_on_team(
    players: &BTreeMap<String, Player>,
    team: &str,
    name: &str,
    mut visit: impl FnMut(&String, &str) -> bool,
) -> Result<(), Stop> {
    for (id, player) in players {
        let on_team = match team {
            "ALL" => true,
            "TEAM_1" => player.team == Some(Team::Team1),
            "TEAM_2" => player.team == Some(Team::Team2),
            _ => unreachable!("member_id checked the `Team` domain"),
        };
        if !on_team {
            continue;
        }
        match &player.hero_member {
            Some(member) => {
                if visit(id, member) {
                    return Ok(());
                }
            }
            None => {
                return missing(
                    MissingInputKind::Hero,
                    Some(id.clone()),
                    None,
                    format!("value {name}"),
                );
            }
        }
    }
    Ok(())
}

/// The value names this module owns.
const NAMES: &[&str] = &[
    "allPlayers",
    "allHeroes",
    "hostPlayer",
    "localPlayer",
    "hasSpawned",
    "isAlive",
    "getHero",
    "teamOf",
    "getSlot",
    "isHeroBeingPlayed",
    "getPlayersOnHero",
    "getHealth",
    "getMaxHealth",
    "hasStatus",
    "isButtonHeld",
    "getAbilityCooldown",
];

/// `Some(result)` when `name` is a player-roster or player-state value;
/// `None` leaves dispatch to the next family. The [`crate::state_reads`]
/// `STATE_READS` table names the scenario-`state` reads alongside the
/// static names.
pub(crate) fn call(name: &str, args: &[Value], env: &Env) -> Option<Result<Val, Stop>> {
    if !NAMES.contains(&name) && crate::state_reads::state_read(name).is_none() {
        return None;
    }
    Some(dispatch(name, args, env))
}

fn dispatch(name: &str, args: &[Value], env: &Env) -> Result<Val, Stop> {
    match name {
        "allPlayers" => {
            let [team] = arguments::<1>(name, args)?;
            // The `Team` argument is an enum literal in practice; an
            // evaluated member-id text (as `Team Of` yields) names the
            // same member.
            let filter = match team {
                Value::Enum { value, .. } => value.clone(),
                other => match eval(other, env)? {
                    Val::Text(text) => text,
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
        // Modeled, not measured: the client's element order is
        // unmeasured, so the catalog's `Hero` member order is the
        // order. Elements are the member ids `Hero Of` yields, so
        // `Array Contains(All Heroes(), Hero Of(p))` holds.
        "allHeroes" => {
            let [] = arguments::<0>(name, args)?;
            let catalog = workshop_rs::catalog::Catalog::builtin_ref()
                .map_err(|error| NotModeled::new(format!("catalog unavailable: {error}")))?;
            let Some(domain) = catalog.enum_domain("Hero") else {
                return unsupported("catalog has no `Hero` domain");
            };
            Ok(Val::Array(
                domain
                    .members
                    .iter()
                    .map(|member| Val::Text(member.member.clone()))
                    .collect(),
            ))
        }
        "hostPlayer" => {
            let [] = arguments::<0>(name, args)?;
            match env.players.iter().find(|(_, player)| player.host) {
                Some((id, _)) => Ok(Val::Player(id.clone())),
                None => unsupported("Host Player: the scenario marks no host player"),
            }
        }
        // A live input: `Local Player` is the scenario player marked
        // `local`; the client binds it to the end user's player, which
        // the simulator cannot derive. A scenario without a `local`
        // player makes the read a missing input.
        "localPlayer" => {
            let [] = arguments::<0>(name, args)?;
            match env.players.iter().find(|(_, player)| player.local) {
                Some((id, _)) => Ok(Val::Player(id.clone())),
                None => missing(
                    MissingInputKind::Local,
                    None,
                    Some("local".to_string()),
                    format!("value {name}"),
                ),
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
        "getHero" | "teamOf" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            // `Hero Of` yields the catalog member id the declared `hero`
            // resolves to — hero text compares by member id, not by the
            // scenario's spelling; `Team Of` likewise yields the member id
            // of the declared `team`.
            let resolved = env.players.get(&id).and_then(|player| match name {
                "getHero" => player.hero_member.clone(),
                _ => player.team.map(|team| team.member_id().to_string()),
            });
            match resolved {
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
        // A live input: `Slot Of` reads the player's declared `slot`;
        // the lobby assigns slots outside the script, so the simulator
        // never derives one from roster order.
        "getSlot" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            match env.players.get(&id) {
                Some(player) => match player.slot {
                    Some(slot) => Ok(Val::Number(f64::from(slot))),
                    None => missing(
                        MissingInputKind::Slot,
                        Some(id),
                        Some("slot".to_string()),
                        format!("value {name}"),
                    ),
                },
                None => unsupported(format!("`{id}` is not a scenario player")),
            }
        }
        // Modeled, not measured: the declared `hero` stands in for the
        // hero the selection screen shows; the screen is not modeled.
        "isHeroBeingPlayed" => {
            let (hero, team) = hero_team_args(name, args, env)?;
            // The first match decides the read.
            let mut found = false;
            visit_on_team(env.players, &team, name, |_, member| {
                found = member == hero;
                found
            })?;
            Ok(Val::Bool(found))
        }
        // The same declared-`hero` read `Is Hero Being Played` makes,
        // yielding the matching players in roster order rather than a
        // boolean.
        "getPlayersOnHero" => {
            let (hero, team) = hero_team_args(name, args, env)?;
            let mut on_hero = Vec::new();
            visit_on_team(env.players, &team, name, |id, member| {
                if member == hero {
                    on_hero.push(Val::Player(id.clone()));
                }
                false
            })?;
            Ok(Val::Array(on_hero))
        }
        // Modeled, not measured: a status is held while its `Set Status`
        // expiry is still ahead.
        "hasStatus" => {
            let [player, status] = arguments::<2>(name, args)?;
            let id = player_id(player, env)?;
            let Value::Enum { value: status, .. } = status else {
                return unsupported(format!("{name} status {status:?}"));
            };
            let world = player_world(env.players, &id)?;
            Ok(Val::Bool(
                world
                    .statuses
                    .get(status.as_str())
                    .is_some_and(|until| *until > env.now),
            ))
        }
        // A live input: the held state comes from the scenario player's
        // `buttons` map; a button not listed is a missing input.
        "isButtonHeld" => {
            let [player, button] = arguments::<2>(name, args)?;
            let id = player_id(player, env)?;
            let slot = button_slot(button)?;
            match env.players.get(&id) {
                Some(player) => match player.held.get(slot) {
                    Some(held) => Ok(Val::Bool(*held)),
                    None => missing(
                        MissingInputKind::Button,
                        Some(id),
                        Some(slot.to_string()),
                        "value isButtonHeld",
                    ),
                },
                None => unsupported(format!("`{id}` is not a scenario player")),
            }
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
        // A live input: the value comes from the scenario player's
        // `state` map, keyed by [`crate::state_reads::STATE_READS`] —
        // scenario-supplied, never simulated; `Speed Of` alone derives
        // the length of `velocity`. A key the map does not list falls
        // back to an established model where one exists — the world
        // pools for `Health`/`Max Health`, the empty world for
        // `Is In Spawn Room` — and is a missing input otherwise.
        name => {
            let Some(read) = crate::state_reads::state_read(name) else {
                unreachable!("call checked the name set");
            };
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            let Some(player) = env.players.get(&id) else {
                return unsupported(format!("`{id}` is not a scenario player"));
            };
            // A declared `state` map opts the player into
            // scenario-supplied reads: a key it does not list is a
            // missing input. Without `state`, the established models
            // still answer where one exists.
            if let Some(state) = &player.state {
                if let Some(value) = state.get(read.key) {
                    return Ok(match (value, read.derived) {
                        (Val::Vector { x, y, z }, true) => {
                            Val::Number((x * x + y * y + z * z).sqrt())
                        }
                        (value, false) => value.clone(),
                        _ => unreachable!("a derived read's key is a vector"),
                    });
                }
                return missing(
                    MissingInputKind::State,
                    Some(id),
                    Some(format!("state.{}", read.key)),
                    format!("value {name}"),
                );
            }
            match read.id {
                "getHealth" | "getMaxHealth" => {
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
                "isInSpawnRoom" => Ok(Val::Bool(false)),
                _ => missing(
                    MissingInputKind::State,
                    Some(id),
                    Some(format!("state.{}", read.key)),
                    format!("value {name}"),
                ),
            }
        }
    }
}
