//! Actions that write a player's world state or steer rule flow:
//! `Damage`, `Heal`, `Set Ability Cooldown`, the athena#58 remainder
//! `Set Status`/`Clear Status`, `Allow Button`/`Disallow Button`,
//! `Teleport` of the empty-world model, `Start Rule` and `Stop Chasing
//! * Variable`. All modeled, not measured — no capture covers them
//! (`docs/architecture.md`).

use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::eval::{Env, button_slot, eval, number, player_id, player_ids};
use crate::load::Instance;
use crate::state;
use crate::step::{World, world_state};
use crate::{Change, NotModeled, PlayerWorld, Stop, Val};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotModeled(NotModeled::new(what)))
}

/// Execute a modeled status/button/rule action against `world`; `None`
/// when `name` is not one of them.
pub(crate) fn action(
    name: &str,
    args: &[Value],
    world: &mut World,
    inst: &Instance<'_>,
    rule_idx: usize,
    subroutines: &BTreeMap<String, usize>,
    now: f64,
) -> Option<Result<(), Stop>> {
    if !matches!(
        name,
        "damage"
            | "heal"
            | "setAbilityCooldown"
            | "setStatusEffect"
            | "clearStatusEffect"
            | "allowButton"
            | "disallowButton"
            | "teleport"
            | "startRule"
            | "stopChasingGlobalVariable"
            | "stopChasingPlayerVariable"
    ) {
        return None;
    }
    Some(run(name, args, world, inst, rule_idx, subroutines, now))
}

fn run(
    name: &str,
    args: &[Value],
    world: &mut World,
    inst: &Instance<'_>,
    rule_idx: usize,
    subroutines: &BTreeMap<String, usize>,
    now: f64,
) -> Result<(), Stop> {
    // The caller's `Env` cannot cross this call — it shares a borrow of
    // `world.players` — so the `Env` is built here. Each arm evaluates
    // its arguments through `env` first, then mutates `world`.
    let env = Env {
        globals: &world.globals,
        players: &world.players,
        event: inst.event.as_ref(),
        each_player: inst.each_player.as_deref(),
        last_text_id: world.hud.last_text_id,
        element: None,
        once: &inst.once,
        last_entity: world.effects.last_created,
        random: world.random.as_ref(),
        server_load: world.server_load,
        map: world.map.as_deref(),
        objectives: world.objectives.as_deref(),
        rule: rule_idx,
        changes: &world.changes,
        now,
    };
    match name {
        "damage" => {
            let [player, damager, amount] = args else {
                return unsupported("damage arguments");
            };
            // Arguments evaluate left to right. `Null` has no
            // dealer scale; a player deals with its `damageDealt%`.
            let ids = player_ids(player, &env)?;
            let damager = eval(damager, &env)?;
            let amount = number(&eval(amount, &env)?)?;
            let dealt = match damager {
                Val::Null => 1.0,
                Val::Player(id) => world_state(world, &id)?.damage_dealt,
                other => {
                    return unsupported(format!("{other:?} where a player is needed"));
                }
            };
            for id in ids {
                let target = world_state(world, &id)?;
                let before = pools(&target.world);
                state::damage(&mut target.world, amount * dealt * target.damage_received);
                let after = pools(&target.world);
                for ((field, old), (_, new)) in before.into_iter().zip(after) {
                    if old != new {
                        world.changes.borrow_mut().push(Change::World {
                            player: id.clone(),
                            field: field.to_string(),
                            old: old.map_or(Val::Null, Val::Number),
                            new: new.map_or(Val::Null, Val::Number),
                        });
                    }
                }
            }
        }
        "heal" => {
            let [player, healer, amount] = args else {
                return unsupported("heal arguments");
            };
            // Same shape as `damage`, with `healingDealt%` and
            // `healingReceived%` scales.
            let ids = player_ids(player, &env)?;
            let healer = eval(healer, &env)?;
            let amount = number(&eval(amount, &env)?)?;
            let dealt = match healer {
                Val::Null => 1.0,
                Val::Player(id) => world_state(world, &id)?.healing_dealt,
                other => {
                    return unsupported(format!("{other:?} where a player is needed"));
                }
            };
            for id in ids {
                let target = world_state(world, &id)?;
                let old = target.world.health.current;
                state::heal(&mut target.world, amount * dealt * target.healing_received);
                let new = target.world.health.current;
                if new != old {
                    world.changes.borrow_mut().push(Change::World {
                        player: id,
                        field: "health".to_string(),
                        old: Val::Number(old),
                        new: Val::Number(new),
                    });
                }
            }
        }
        "setAbilityCooldown" => {
            let [player, button, cooldown] = args else {
                return unsupported("setAbilityCooldown arguments");
            };
            // `Set Ability Cooldown` writes the reported cooldown for
            // the slot literally; whether `abilityCooldown%` also
            // scales it is what P7 is for.
            let ids = player_ids(player, &env)?;
            let slot = button_slot(button)?;
            let seconds = number(&eval(cooldown, &env)?)?;
            for id in ids {
                let target = world_state(world, &id)?;
                let old = target.world.cooldowns.get(slot).copied();
                target.world.cooldowns.insert(slot.to_string(), seconds);
                world.changes.borrow_mut().push(Change::World {
                    player: id,
                    field: format!("cooldown.{slot}"),
                    old: old.map_or(Val::Null, Val::Number),
                    new: Val::Number(seconds),
                });
            }
        }
        "setStatusEffect" | "clearStatusEffect" => {
            // `Set Status(player, assister, status, duration)`;
            // `Clear Status(player, status)`. The status map holds the
            // `Total Time Elapsed` seconds at which the status ends, so
            // `Has Status` answers by comparing `now`.
            let (ids, status, until) = match (name, args) {
                ("setStatusEffect", [player, assister, status, duration]) => {
                    // Arguments evaluate left to right; the assister
                    // carries no modeled effect but must still resolve
                    // like `damage`'s dealer.
                    let ids = player_ids(player, &env)?;
                    match eval(assister, &env)? {
                        Val::Null | Val::Player(_) => {}
                        other => {
                            return unsupported(format!("{other:?} where a player is needed"));
                        }
                    }
                    (ids, status, Some(now + number(&eval(duration, &env)?)?))
                }
                ("clearStatusEffect", [player, status]) => {
                    (player_ids(player, &env)?, status, None)
                }
                _ => return unsupported(format!("{name} arguments")),
            };
            let Value::Enum { value: status, .. } = status else {
                return unsupported(format!("{name} status {status:?}"));
            };
            for id in ids {
                let target = world_state(world, &id)?;
                let old = match until {
                    Some(until) => target.world.statuses.insert(status.clone(), until),
                    None => target.world.statuses.remove(status.as_str()),
                };
                if old.is_some() || until.is_some() {
                    world.changes.borrow_mut().push(Change::World {
                        player: id,
                        field: format!("status.{}", status.to_lowercase()),
                        old: old.map_or(Val::Null, Val::Number),
                        new: until.map_or(Val::Null, Val::Number),
                    });
                }
            }
        }
        "allowButton" | "disallowButton" => {
            let [player, button] = args else {
                return unsupported(format!("{name} arguments"));
            };
            let ids = player_ids(player, &env)?;
            let slot = button_slot(button)?;
            let allowed = name == "allowButton";
            for id in ids {
                let target = world_state(world, &id)?;
                let old = target.world.buttons.insert(slot.to_string(), allowed);
                world.changes.borrow_mut().push(Change::World {
                    player: id,
                    field: format!("button.{slot}"),
                    old: old.map_or(Val::Null, Val::Bool),
                    new: Val::Bool(allowed),
                });
            }
        }
        "teleport" => {
            let [player, position] = args else {
                return unsupported("teleport arguments");
            };
            // The empty world has no physics: a teleport writes the
            // scenario `position` state, and `Position Of` reads it
            // back. `Null` is the catalog's `zeroAsNull` coercion — a
            // teleport to the origin.
            let ids = player_ids(player, &env)?;
            let destination = match eval(position, &env)? {
                Val::Null => crate::vector::Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                other => crate::vector::position_value(other, &world.players, name)?,
            };
            for id in ids {
                let Some(target) = world.players.get_mut(&id) else {
                    return unsupported(format!("`{id}` is not a scenario player"));
                };
                let old = target.position.map_or(Val::Null, Val::from);
                target.position = Some(destination);
                world.changes.borrow_mut().push(Change::World {
                    player: id,
                    field: "position".to_string(),
                    old,
                    new: destination.into(),
                });
            }
        }
        "startRule" => {
            // `Start Rule(subroutine, ifAlreadyExecuting)` launches the
            // subroutine's own rule instance; the request is applied when
            // the tick loop visits that instance.
            let [subroutine, behavior] = args else {
                return unsupported("startRule arguments");
            };
            let Value::Subroutine(subroutine) = subroutine else {
                return unsupported(format!("startRule subroutine {subroutine:?}"));
            };
            let Some(&target) = subroutines.get(subroutine) else {
                return unsupported(format!("subroutine {subroutine}"));
            };
            let restart = match behavior {
                Value::Enum { value, .. } => match value.as_str() {
                    "RESTART" => true,
                    "NOOP" => false,
                    other => return unsupported(format!("startRule behavior {other}")),
                },
                other => return unsupported(format!("startRule behavior {other:?}")),
            };
            world.pending_starts.push((target, restart));
        }
        _ => {
            // The chase actions are not modeled, so a run never has a
            // chase in progress; stopping one changes nothing. The
            // variable's player still resolves.
            let [variable] = args else {
                return unsupported(format!("{name} arguments"));
            };
            match variable {
                Value::GlobalVariable(_) => {}
                Value::PlayerVariable { player, .. } => {
                    let id = player_id(player, &env)?;
                    if !world.players.contains_key(&id) {
                        return unsupported(format!("player variable of undeclared player `{id}`"));
                    }
                }
                other => return unsupported(format!("{name} variable {other:?}")),
            }
        }
    }
    Ok(())
}

/// The `(field, current)` pairs `Damage` reports for the pools that
/// exist, in the order the pools absorb damage.
fn pools(world: &PlayerWorld) -> [(&'static str, Option<f64>); 3] {
    [
        ("shield", world.shield.as_ref().map(|pool| pool.current)),
        ("armor", world.armor.as_ref().map(|pool| pool.current)),
        ("health", Some(world.health.current)),
    ]
}
