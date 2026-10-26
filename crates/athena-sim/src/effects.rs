//! Effect actions: `Create Effect`, `Play Effect` and `Destroy Effect`.
//! Entities are recorded like HUD elements — evaluated arguments and
//! recipients — but the model has no geometry: the `position` argument is
//! stored as evaluated, and an expression that needs geometry (for example
//! `Position Of`) stops the run as `not_modeled` like any unmodeled
//! construct.

use std::cell::RefCell;

use workshop_rs::Value;

use crate::eval::{Env, eval, viewers};
use crate::hud::enum_member;
use crate::{Change, EffectArgs, EffectElement, EffectKind, NotModeled, Stop, Val};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(NotModeled::new(what).into())
}

/// The effect entities a run created or played and the `id` of the newest
/// `createEffect` entity, which `Last Created Entity` reads.
#[derive(Default)]
pub(crate) struct Effects {
    /// In creation order; an entity's `id` is its index.
    pub elements: Vec<EffectElement>,
    /// The `id` of the newest `createEffect` entity. A `playEffect` has no
    /// entity id to hand out.
    pub last_created: Option<u64>,
}

/// Record an effect entity. `id` is the creation-order index.
fn record(
    effects: &mut Effects,
    kind: EffectKind,
    tick: u64,
    name: &str,
    recipients: Vec<String>,
    args: EffectArgs,
    changes: &RefCell<Vec<Change>>,
) {
    let id = effects.elements.len() as u64;
    effects.elements.push(EffectElement {
        id,
        kind,
        created_tick: tick,
        destroyed_tick: (kind == EffectKind::PlayedEffect).then_some(tick),
        recipients,
        construct: format!("action {name}"),
        args,
    });
    if kind == EffectKind::Effect {
        effects.last_created = Some(id);
    }
    changes
        .borrow_mut()
        .push(Change::EffectCreate { id, element: kind });
}

/// Execute a modeled effect action against `effects`; `None` when `name`
/// is not one of them.
pub(crate) fn action(
    name: &str,
    args: &[Value],
    env: &Env,
    effects: &mut Effects,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Option<Result<(), Stop>> {
    Some(match name {
        "createEffect" => create_effect(args, env, effects, tick, changes),
        "playEffect" => play_effect(args, env, effects, tick, changes),
        "destroyEffect" => destroy_effect(args, env, effects, tick, changes),
        "destroyAllEffects" => destroy_all_effects(args, effects, tick, changes),
        _ => return None,
    })
}

fn create_effect(
    args: &[Value],
    env: &Env,
    effects: &mut Effects,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [visible_to, effect, color, position, radius, reevaluation] = args else {
        return unsupported("createEffect arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let args = EffectArgs {
        effect: Some(enum_member("createEffect", "type", effect)?.to_string()),
        color: Some(eval(color, env)?),
        position: Some(eval(position, env)?),
        radius: Some(eval(radius, env)?),
        reevaluation: Some(enum_member("createEffect", "reevaluation", reevaluation)?.to_string()),
    };
    record(
        effects,
        EffectKind::Effect,
        tick,
        "createEffect",
        recipients,
        args,
        changes,
    );
    Ok(())
}

fn play_effect(
    args: &[Value],
    env: &Env,
    effects: &mut Effects,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [visible_to, effect, color, position, radius] = args else {
        return unsupported("playEffect arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let args = EffectArgs {
        effect: Some(enum_member("playEffect", "type", effect)?.to_string()),
        color: Some(eval(color, env)?),
        position: Some(eval(position, env)?),
        radius: Some(eval(radius, env)?),
        ..EffectArgs::default()
    };
    record(
        effects,
        EffectKind::PlayedEffect,
        tick,
        "playEffect",
        recipients,
        args,
        changes,
    );
    Ok(())
}

fn destroy_effect(
    args: &[Value],
    env: &Env,
    effects: &mut Effects,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [entity] = args else {
        return unsupported("destroyEffect arguments");
    };
    // An unknown, `null` or already-destroyed entity id destroys nothing,
    // the same rule a HUD destroy follows.
    match eval(entity, env)? {
        Val::Null => {}
        Val::Number(id) => {
            if let Some(element) = effects.elements.iter_mut().find(|element| {
                element.kind == EffectKind::Effect
                    && element.destroyed_tick.is_none()
                    && element.id as f64 == id
            }) {
                element.destroyed_tick = Some(tick);
                changes.borrow_mut().push(Change::EffectDestroy {
                    id: element.id,
                    element: element.kind,
                });
            }
        }
        other => return unsupported(format!("destroyEffect of {other:?}")),
    }
    Ok(())
}

/// `Destroy All Effects` removes every live `createEffect` entity, the
/// same rule the HUD `Destroy All` actions follow. A played effect is
/// already destroyed on its tick.
fn destroy_all_effects(
    args: &[Value],
    effects: &mut Effects,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [] = args else {
        return unsupported("destroyAllEffects arguments");
    };
    for element in &mut effects.elements {
        if element.kind == EffectKind::Effect && element.destroyed_tick.is_none() {
            element.destroyed_tick = Some(tick);
            changes.borrow_mut().push(Change::EffectDestroy {
                id: element.id,
                element: element.kind,
            });
        }
    }
    Ok(())
}
