//! Actions that write a player's world state or steer rule flow:
//! `Damage`, `Heal`, `Set Ability Cooldown`, the athena#58 remainder
//! `Set Status`/`Clear Status`, `Allow Button`/`Disallow Button`,
//! `Teleport` of the empty-world model, `Start Rule` and `Stop Chasing
//! * Variable`. All modeled, not measured — no capture covers them
//! (`docs/architecture.md`).

use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::eval::{button_slot, eval, number, player_id, player_ids};
use crate::load::Instance;
use crate::scenario::EventCtx;
use crate::state;
use crate::step::{World, world_state};
use crate::unsupported;
use crate::{Change, PlayerWorld, Stop, Val};

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
    let env = world.env(
        inst.event.as_ref(),
        inst.each_player.as_deref(),
        &inst.once,
        rule_idx,
        now,
    );
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
            let dealer = match &damager {
                Val::Null => None,
                Val::Player(id) => Some(world_state(world, id)?.damage_dealt),
                other => {
                    return unsupported(format!("{other:?} where a player is needed"));
                }
            };
            for id in ids {
                // A dead target takes no pool change and raises no
                // events — `isDead` decides dead, as `Is Alive` reads
                // it, and the health pool decides where `isDead`
                // cannot answer.
                if dead(world, &id, "action damage")? {
                    continue;
                }
                let (applied, died) = {
                    let target = world_state(world, &id)?;
                    let applied = amount * dealer.unwrap_or(1.0) * target.damage_received;
                    let before = pools(&target.world);
                    let health = target.world.health.current;
                    state::damage(&mut target.world, applied);
                    let died = health > 0.0 && target.world.health.current == 0.0;
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
                    (applied, died)
                };
                // The damage raises the player events the catalog
                // defines for the cause, in occurrence order: the
                // victim's `Player Took Damage`, the dealer's `Player
                // Dealt Damage`, then — when the hit brought health to
                // zero — the victim's `Player Died` and the dealer's
                // `Player Dealt Final Blow`. A `Null` dealer raises no
                // dealer-side event and binds `Attacker` to `Null`; a
                // `Damage` on an already-dead target raises nothing.
                if applied > 0.0 {
                    let values = event_values(&damager, &id, "eventDamage", applied);
                    raise(world, "playerTookDamage", &id, values.clone())?;
                    if let Val::Player(dealer) = &damager {
                        raise(world, "playerDealtDamage", dealer, values.clone())?;
                    }
                    if died {
                        crate::state_writes::write_cell(
                            world,
                            &id,
                            "isDead",
                            Val::Bool(true),
                            "action damage",
                        )?;
                        raise(world, "playerDied", &id, values.clone())?;
                        if let Val::Player(dealer) = &damager {
                            raise(world, "playerDealtFinalBlow", dealer, values)?;
                        }
                    }
                }
            }
        }
        "heal" => {
            let [player, healer, amount] = args else {
                return unsupported("heal arguments");
            };
            // Same shape as `damage`, with `healingDealt%` and
            // `healingReceived%` scales. A positive heal on a living
            // player raises `Player Received Healing` for the healed
            // and `Player Dealt Healing` for a player healer, in that
            // order.
            let ids = player_ids(player, &env)?;
            let healer = eval(healer, &env)?;
            let amount = number(&eval(amount, &env)?)?;
            let dealer = match &healer {
                Val::Null => None,
                Val::Player(id) => Some(world_state(world, id)?.healing_dealt),
                other => {
                    return unsupported(format!("{other:?} where a player is needed"));
                }
            };
            for id in ids {
                // The same death gate as `Damage`: a dead target takes
                // no pool change and raises no healing events.
                if dead(world, &id, "action heal")? {
                    continue;
                }
                let target = world_state(world, &id)?;
                let applied = amount * dealer.unwrap_or(1.0) * target.healing_received;
                let old = target.world.health.current;
                state::heal(&mut target.world, applied);
                let new = target.world.health.current;
                if new != old {
                    world.changes.borrow_mut().push(Change::World {
                        player: id.clone(),
                        field: "health".to_string(),
                        old: Val::Number(old),
                        new: Val::Number(new),
                    });
                }
                if applied > 0.0 {
                    let values = event_values(&healer, &id, "eventHealing", applied);
                    raise(world, "playerReceivedHealing", &id, values.clone())?;
                    if let Val::Player(dealer) = &healer {
                        raise(world, "playerDealtHealing", dealer, values)?;
                    }
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
            let destination =
                crate::vector::position_value(eval(position, &env)?, &world.players, name)?;
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

/// Whether `Damage`/`Heal` treats player `id` as dead: the `isDead`
/// read decides where it answers — a cell an action wrote or the
/// declared `state.isDead`, the same read `Is Alive` makes — and the
/// health pool decides where `isDead` cannot answer. `construct`
/// names the action in the stop a fallback `world_state` touch can
/// raise.
fn dead(world: &mut World, id: &str, construct: &str) -> Result<bool, Stop> {
    let read = crate::state_reads::state_read("isDead").expect("in STATE_READS");
    match crate::player::state_read_value(&world.players, id, read, construct) {
        Ok(Val::Bool(dead)) => Ok(dead),
        _ => Ok(world_state(world, id)?.world.health.current <= 0.0),
    }
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

/// Queue a player event `cause` raised on `world.pending_events` for
/// the tick loop to deliver: `player` is the event player the rule's
/// `Event Player` binds. A raised event counts toward the tick's
/// [`crate::step::TICK_EVENT_LIMIT`]: a triggered rule that damages or
/// heals again can fan the queue out without bound without one.
fn raise(
    world: &mut World,
    kind: &str,
    player: &str,
    values: BTreeMap<String, Val>,
) -> Result<(), Stop> {
    if world.tick_events >= crate::step::TICK_EVENT_LIMIT {
        return unsupported(format!(
            "more than {} player events raised in one tick",
            crate::step::TICK_EVENT_LIMIT
        ));
    }
    world.tick_events += 1;
    world.pending_events.push(EventCtx {
        kind: kind.to_string(),
        player: Some(player.to_string()),
        values,
        raised: true,
    });
    Ok(())
}

/// The `values` a `Damage`/`Heal` event binds: `Attacker` the dealing
/// or healing player (`Null` when the cause names none), `Victim` the
/// player the pool moved on, and `key` (`eventDamage`/`eventHealing`)
/// the applied amount.
fn event_values(cause: &Val, victim: &str, key: &str, amount: f64) -> BTreeMap<String, Val> {
    BTreeMap::from([
        ("attacker".to_string(), cause.clone()),
        ("victim".to_string(), Val::Player(victim.to_string())),
        (key.to_string(), Val::Number(amount)),
    ])
}
