//! Actions that write a player's `state` cells or store a modifier on
//! the player (athena#116). A cell write defines the `state` key for
//! later reads — the scenario does not have to supply a key a program
//! writes first — and records a `state` change naming the player and
//! the key. A stored modifier records the action, the player and the
//! evaluated arguments in `changes` and on the player's `modifiers`;
//! nothing in the model reads it back and no gameplay effect follows —
//! no damage, healing, movement or physics is simulated. Modeled, not
//! measured — the basis is the catalog sentence per action
//! (`docs/findings.md`).

use workshop_rs::Value;

use crate::eval::{Env, eval, number, player_ids};
use crate::load::Instance;
use crate::player::state_read_value;
use crate::scenario::Team;
use crate::state_reads::state_key;
use crate::step::World;
use crate::vector::{Facing, Vec3};
use crate::{Change, Stop, Val};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(crate::NotModeled::new(what).into())
}

/// The modifier key each stored-modifier action writes on the player.
fn modifier_key(name: &str) -> &'static str {
    match name {
        "setUltEnabled" => "ultEnabled",
        "setAbility1Enabled" => "ability1Enabled",
        "setAbility2Enabled" => "ability2Enabled",
        "setDamageDealt" => "damageDealt",
        "setDamageReceived" => "damageReceived",
        "setHealingDealt" => "healingDealt",
        "setHealingReceived" => "healingReceived",
        "setMoveSpeed" => "moveSpeed",
        "setGravity" => "gravity",
        "setKnockbackReceived" => "knockbackReceived",
        "setAllowedHeroes" => "allowedHeroes",
        "startForcingHero" | "stopForcingCurrentHero" => "forcedHero",
        "startForcingSpawn" => "forcedSpawnRoom",
        _ => unreachable!("dispatch checked the name set"),
    }
}

/// Execute a cell-write or stored-modifier action against `world`;
/// `None` when `name` is not one of them.
pub(crate) fn action(
    name: &str,
    args: &[Value],
    world: &mut World,
    inst: &Instance<'_>,
    rule_idx: usize,
    now: f64,
) -> Option<Result<(), Stop>> {
    if !matches!(
        name,
        "setHealth"
            | "setMaxHealth"
            | "setUltCharge"
            | "kill"
            | "respawn"
            | "resurrect"
            | "setFacing"
            | "setUltEnabled"
            | "setAbility1Enabled"
            | "setAbility2Enabled"
            | "setDamageDealt"
            | "setDamageReceived"
            | "setHealingDealt"
            | "setHealingReceived"
            | "setMoveSpeed"
            | "setGravity"
            | "setKnockbackReceived"
            | "setAllowedHeroes"
            | "startForcingHero"
            | "stopForcingCurrentHero"
            | "startForcingSpawn"
    ) {
        return None;
    }
    Some(run(name, args, world, inst, rule_idx, now))
}

fn run(
    name: &str,
    args: &[Value],
    world: &mut World,
    inst: &Instance<'_>,
    rule_idx: usize,
    now: f64,
) -> Result<(), Stop> {
    // The caller's `Env` cannot cross this call — it shares a borrow of
    // `world.players` — so the `Env` is built here, as in `actions.rs`.
    // Each arm evaluates its arguments left to right first, then
    // mutates `world` per target player.
    let env = Env {
        globals: &world.globals,
        players: &world.players,
        event: inst.event.as_ref(),
        each_player: inst.each_player.as_deref(),
        hud: &world.hud,
        element: None,
        once: &inst.once,
        effects: &world.effects,
        random: world.random.as_ref(),
        server_load: world.server_load,
        map: world.map.as_deref(),
        match_phase: world.match_phase,
        game_mode: world.game_mode.as_deref(),
        objectives: world.objectives.as_deref(),
        rule: rule_idx,
        changes: &world.changes,
        now,
    };
    match name {
        "setHealth" => {
            let [player, amount] = args else {
                return unsupported("setHealth arguments");
            };
            let ids = player_ids(player, &env)?;
            let amount = number(&eval(amount, &env)?)?;
            for id in ids {
                // The catalog: "only has an effect on living players" —
                // the gate reads `isDead` the way a program read would,
                // so a scenario that cannot answer it stops the run.
                let dead = read_key(world, &id, "isDead", "action setHealth")?;
                if dead == Val::Bool(true) {
                    continue;
                }
                write_cell(
                    world,
                    &id,
                    "health",
                    Val::Number(amount),
                    "action setHealth",
                )?;
            }
        }
        "setMaxHealth" => {
            let [player, percent] = args else {
                return unsupported("setMaxHealth arguments");
            };
            let ids = player_ids(player, &env)?;
            let percent = number(&eval(percent, &env)?)?;
            for id in ids {
                // "a percentage of their max health": the current value
                // the same read would report — cell, declared `state`,
                // or the no-`state` pool fallback.
                let max = number(&read_key(world, &id, "maxHealth", "action setMaxHealth")?)?;
                let max = max * percent / 100.0;
                // "will ensure that a player's current health will not
                // exceed the new max health".
                let health = number(&read_key(world, &id, "health", "action setMaxHealth")?)?;
                write_cell(
                    world,
                    &id,
                    "maxHealth",
                    Val::Number(max),
                    "action setMaxHealth",
                )?;
                if health > max {
                    write_cell(
                        world,
                        &id,
                        "health",
                        Val::Number(max),
                        "action setMaxHealth",
                    )?;
                }
            }
        }
        "setUltCharge" => {
            let [player, percent] = args else {
                return unsupported("setUltCharge arguments");
            };
            let ids = player_ids(player, &env)?;
            // `Number|Boolean`: `False` writes 0, `True` writes 1.
            let charge = number(&eval(percent, &env)?)?;
            for id in ids {
                write_cell(
                    world,
                    &id,
                    "ultCharge",
                    Val::Number(charge),
                    "action setUltCharge",
                )?;
            }
        }
        "kill" => {
            let [player, killer] = args else {
                return unsupported("kill arguments");
            };
            let ids = player_ids(player, &env)?;
            // The killer resolves like `Damage`'s dealer: a player or
            // `Null`; it carries no modeled effect.
            match eval(killer, &env)? {
                Val::Null | Val::Player(_) => {}
                other => return unsupported(format!("{other:?} where a player is needed")),
            }
            for id in ids {
                write_cell(world, &id, "isDead", Val::Bool(true), "action kill")?;
            }
        }
        "respawn" | "resurrect" => {
            let [player] = args else {
                return unsupported(format!("{name} arguments"));
            };
            let ids = player_ids(player, &env)?;
            for id in ids {
                if name == "respawn" {
                    // "with full health": `Health` becomes what
                    // `Max Health` reports. The relocation to a spawn
                    // point is a gameplay effect the model does not
                    // simulate, so `position` is not written.
                    let full = read_key(world, &id, "maxHealth", "action respawn")?;
                    write_cell(world, &id, "isDead", Val::Bool(false), "action respawn")?;
                    write_cell(world, &id, "health", full, "action respawn")?;
                } else {
                    write_cell(world, &id, "isDead", Val::Bool(false), "action resurrect")?;
                }
            }
        }
        "setFacing" => {
            let [player, direction, relativity] = args else {
                return unsupported("setFacing arguments");
            };
            let ids = player_ids(player, &env)?;
            let direction = crate::vector::vec(&eval(direction, &env)?, "setFacing")?;
            // `To World` writes the direction's facing outright; the
            // catalog does not settle how `To Player` composes it with
            // the current facing, so that relativity is not modeled.
            match relativity {
                Value::Enum { value, .. } if value == "TO_WORLD" => {}
                other => return unsupported(format!("setFacing relativity {other:?}")),
            }
            let facing = facing_of(direction)?;
            for id in ids {
                let Some(target) = world.players.get_mut(&id) else {
                    return unsupported(format!("`{id}` is not a scenario player"));
                };
                let old = target
                    .facing
                    .map(|facing| crate::vector::direction(facing).into())
                    .unwrap_or(Val::Null);
                target.facing = Some(facing);
                world.changes.borrow_mut().push(Change::World {
                    player: id,
                    field: "facing".to_string(),
                    old,
                    new: crate::vector::direction(facing).into(),
                });
            }
        }
        "startForcingSpawn" => {
            let [team, room] = args else {
                return unsupported("startForcingSpawn arguments");
            };
            // The `Team` argument is an enum literal in practice; an
            // evaluated member-id text names the same member. The
            // modifier lands on every player of the team — the spawn
            // room itself is not a per-player target.
            let team = match team {
                Value::Enum { value, .. } => value.clone(),
                other => match eval(other, &env)? {
                    Val::Text(member) => member,
                    other => return unsupported(format!("{name} on {other:?}")),
                },
            };
            let room = eval(room, &env)?;
            for (id, player) in &mut world.players {
                let on_team = match team.as_str() {
                    "ALL" => true,
                    "TEAM_1" => player.team == Some(Team::Team1),
                    "TEAM_2" => player.team == Some(Team::Team2),
                    other => return unsupported(format!("{name} on {other}")),
                };
                if on_team {
                    player
                        .modifiers
                        .insert(modifier_key(name).to_string(), room.clone());
                    world.changes.borrow_mut().push(Change::Modifier {
                        player: id.clone(),
                        action: name.to_string(),
                        args: vec![room.clone()],
                    });
                }
            }
        }
        "stopForcingCurrentHero" => {
            let [player] = args else {
                return unsupported("stopForcingCurrentHero arguments");
            };
            for id in player_ids(player, &env)? {
                let Some(target) = world.players.get_mut(&id) else {
                    return unsupported(format!("`{id}` is not a scenario player"));
                };
                target.modifiers.remove(modifier_key(name));
                world.changes.borrow_mut().push(Change::Modifier {
                    player: id,
                    action: name.to_string(),
                    args: Vec::new(),
                });
            }
        }
        _ => {
            // The remaining actions share the `(Player|Array, value)`
            // shape: evaluate both arguments left to right, then store
            // the modifier on each target and record it.
            let [player, value] = args else {
                return unsupported(format!("{name} arguments"));
            };
            let ids = player_ids(player, &env)?;
            let value = eval(value, &env)?;
            for id in ids {
                let Some(target) = world.players.get_mut(&id) else {
                    return unsupported(format!("`{id}` is not a scenario player"));
                };
                target
                    .modifiers
                    .insert(modifier_key(name).to_string(), value.clone());
                world.changes.borrow_mut().push(Change::Modifier {
                    player: id,
                    action: name.to_string(),
                    args: vec![value.clone()],
                });
            }
        }
    }
    Ok(())
}

/// The value a `state` key of player `id` currently reads — the cell,
/// the declared `state` entry or the no-`state` fallback — for a
/// write's own input (the `Set Health` gate, `Set Max Health`'s
/// percentage base, `Respawn`'s full health).
fn read_key(world: &World, id: &str, key: &str, construct: &str) -> Result<Val, Stop> {
    let read = state_key(key).expect("a declared `state` key");
    state_read_value(&world.players, id, read, construct)
}

/// Write `key`'s cell on player `id` and record the `state` change:
/// `old` is what the key read before the write, `null` when nothing
/// answered it.
fn write_cell(
    world: &mut World,
    id: &str,
    key: &str,
    new: Val,
    construct: &str,
) -> Result<(), Stop> {
    let old = read_key(world, id, key, construct).unwrap_or(Val::Null);
    let Some(target) = world.players.get_mut(id) else {
        return unsupported(format!("`{id}` is not a scenario player"));
    };
    target.cells.insert(key.to_string(), new.clone());
    world.changes.borrow_mut().push(Change::State {
        player: id.to_string(),
        key: key.to_string(),
        old,
        new,
    });
    Ok(())
}

/// The facing a `Set Facing` direction points: the same
/// horizontal/vertical degrees `Horizontal`/`Vertical Angle From
/// Direction` derive. A zero-length or non-finite direction names no
/// facing and is `not_modeled`.
fn facing_of(v: Vec3) -> Result<Facing, Stop> {
    if v.magnitude() == 0.0 {
        return unsupported("setFacing of a zero-length direction");
    }
    let facing = Facing {
        horizontal: v.x.atan2(v.z).to_degrees(),
        vertical: (-v.y).atan2(v.x.hypot(v.z)).to_degrees(),
    };
    if !facing.horizontal.is_finite() || !facing.vertical.is_finite() {
        return unsupported("setFacing produced a non-finite facing");
    }
    Ok(facing)
}
