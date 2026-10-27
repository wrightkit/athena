//! Entity actions: `Create Effect`, `Create Beam Effect`, `Create Icon`,
//! `Create Projectile Effect`, `Play Effect` and the destroys. Entities
//! are recorded like display elements — evaluated arguments and
//! recipients — but the model has no geometry: the `position`-family
//! arguments are stored as evaluated, and an expression that needs
//! geometry (for example `Position Of`) stops the run as `not_modeled`
//! like any unmodeled construct.
//!
//! The four create actions share the catalog's `EntityId` space: the
//! entity `id` is what `Last Created Entity`, `Entity Exists` and
//! `Entity Count` read and the destroy actions target. The catalog binds
//! each destroy to its creators: `Destroy Effect` covers `createEffect`
//! and `createBeamEffect` ("created by create effect or Create Beam
//! Effect"), `Destroy Icon` covers `createIcon`, and no destroy names a
//! projectile effect entity.

use std::cell::RefCell;

use workshop_rs::Value;

use crate::eval::{Env, eval, viewers};
use crate::hud::enum_member;
use crate::unsupported;
use crate::{Change, EffectArgs, EffectElement, EffectKind, Stop, Val};

/// The entities a run created or played and the `id` of the newest
/// durable one, which `Last Created Entity` reads.
#[derive(Default)]
pub(crate) struct Effects {
    /// In creation order; an entity's `id` is its index.
    pub elements: Vec<EffectElement>,
    /// The `id` of the newest durable entity. A `playEffect` has no
    /// entity id to hand out.
    pub last_created: Option<u64>,
}

/// The kinds `getNumberOfEntityIds` counts per the catalog's sentence —
/// `createEffect`, `createBeamEffect` and `createIcon` entities; a
/// `projectileEffect` is not named.
fn counted_entity_kind(kind: EffectKind) -> bool {
    matches!(
        kind,
        EffectKind::Effect | EffectKind::Beam | EffectKind::Icon
    )
}

impl Effects {
    /// Whether a live entity of `id` exists; what `Entity Exists`
    /// reads of a number.
    pub(crate) fn entity_exists(&self, id: f64) -> bool {
        self.elements
            .iter()
            .any(|element| element.destroyed_tick.is_none() && element.id as f64 == id)
    }

    /// The live `effect`/`beam`/`icon` entities `getNumberOfEntityIds`
    /// counts.
    pub(crate) fn entity_count(&self) -> u64 {
        self.elements
            .iter()
            .filter(|element| counted_entity_kind(element.kind) && element.destroyed_tick.is_none())
            .count() as u64
    }
}

/// Record an entity. `id` is the creation-order index; a durable kind
/// also hands its `id` to `Last Created Entity`.
fn record(
    effects: &RefCell<Effects>,
    kind: EffectKind,
    tick: u64,
    name: &str,
    recipients: Vec<String>,
    args: EffectArgs,
    changes: &RefCell<Vec<Change>>,
) {
    let mut effects = effects.borrow_mut();
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
    if kind != EffectKind::PlayedEffect {
        effects.last_created = Some(id);
    }
    changes
        .borrow_mut()
        .push(Change::EffectCreate { id, element: kind });
}

/// Mark a live entity of one of `kinds` with `id` destroyed on `tick`; an
/// id no live entity of those kinds holds destroys nothing, the same
/// rule a display-element destroy follows.
fn destroy_kinds(
    effects: &RefCell<Effects>,
    kinds: &[EffectKind],
    id: f64,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) {
    let mut effects = effects.borrow_mut();
    if let Some(element) = effects.elements.iter_mut().find(|element| {
        kinds.contains(&element.kind) && element.destroyed_tick.is_none() && element.id as f64 == id
    }) {
        element.destroyed_tick = Some(tick);
        let id = element.id;
        let kind = element.kind;
        changes
            .borrow_mut()
            .push(Change::EffectDestroy { id, element: kind });
    }
}

/// Mark every live entity of one of `kinds` destroyed on `tick`.
fn destroy_all_kinds(
    effects: &RefCell<Effects>,
    kinds: &[EffectKind],
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) {
    let mut destroyed = Vec::new();
    {
        let mut effects = effects.borrow_mut();
        for element in &mut effects.elements {
            if kinds.contains(&element.kind) && element.destroyed_tick.is_none() {
                element.destroyed_tick = Some(tick);
                destroyed.push((element.id, element.kind));
            }
        }
    }
    for (id, kind) in destroyed {
        changes
            .borrow_mut()
            .push(Change::EffectDestroy { id, element: kind });
    }
}

/// Execute a modeled entity action against `effects`; `None` when `name`
/// is not one of them.
pub(crate) fn action(
    name: &str,
    args: &[Value],
    env: &Env,
    effects: &RefCell<Effects>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Option<Result<(), Stop>> {
    Some(match name {
        "createEffect" => create_effect(args, env, effects, tick, changes),
        "createBeamEffect" => create_beam_effect(args, env, effects, tick, changes),
        "createIcon" => create_icon(args, env, effects, tick, changes),
        "createProjectileEffect" => create_projectile_effect(args, env, effects, tick, changes),
        "playEffect" => play_effect(args, env, effects, tick, changes),
        "destroyEffect" | "destroyIcon" => destroy(name, args, env, effects, tick, changes),
        "destroyAllEffects" | "destroyAllIcons" => destroy_all(name, args, effects, tick, changes),
        _ => return None,
    })
}

fn create_effect(
    args: &[Value],
    env: &Env,
    effects: &RefCell<Effects>,
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
        ..EffectArgs::default()
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

fn create_beam_effect(
    args: &[Value],
    env: &Env,
    effects: &RefCell<Effects>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [
        visible_to,
        effect,
        start_position,
        end_position,
        color,
        reevaluation,
    ] = args
    else {
        return unsupported("createBeamEffect arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let args = EffectArgs {
        effect: Some(enum_member("createBeamEffect", "type", effect)?.to_string()),
        start_position: Some(eval(start_position, env)?),
        end_position: Some(eval(end_position, env)?),
        color: Some(eval(color, env)?),
        reevaluation: Some(
            enum_member("createBeamEffect", "reevaluation", reevaluation)?.to_string(),
        ),
        ..EffectArgs::default()
    };
    record(
        effects,
        EffectKind::Beam,
        tick,
        "createBeamEffect",
        recipients,
        args,
        changes,
    );
    Ok(())
}

fn create_icon(
    args: &[Value],
    env: &Env,
    effects: &RefCell<Effects>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [
        visible_to,
        position,
        icon,
        reevaluation,
        color,
        show_when_offscreen,
    ] = args
    else {
        return unsupported("createIcon arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let args = EffectArgs {
        position: Some(eval(position, env)?),
        icon: Some(enum_member("createIcon", "icon", icon)?.to_string()),
        reevaluation: Some(enum_member("createIcon", "reevaluation", reevaluation)?.to_string()),
        color: Some(eval(color, env)?),
        show_when_offscreen: Some(eval(show_when_offscreen, env)?),
        ..EffectArgs::default()
    };
    record(
        effects,
        EffectKind::Icon,
        tick,
        "createIcon",
        recipients,
        args,
        changes,
    );
    Ok(())
}

fn create_projectile_effect(
    args: &[Value],
    env: &Env,
    effects: &RefCell<Effects>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [
        visible_to,
        effect,
        friendly_to,
        position,
        direction,
        oversize,
        reevaluation,
    ] = args
    else {
        return unsupported("createProjectileEffect arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let friendly_to = viewers(friendly_to, env)?;
    let args = EffectArgs {
        effect: Some(enum_member("createProjectileEffect", "type", effect)?.to_string()),
        friendly_to: Some(friendly_to),
        position: Some(eval(position, env)?),
        direction: Some(eval(direction, env)?),
        oversize: Some(eval(oversize, env)?),
        reevaluation: Some(
            enum_member("createProjectileEffect", "reevaluation", reevaluation)?.to_string(),
        ),
        ..EffectArgs::default()
    };
    record(
        effects,
        EffectKind::ProjectileEffect,
        tick,
        "createProjectileEffect",
        recipients,
        args,
        changes,
    );
    Ok(())
}

fn play_effect(
    args: &[Value],
    env: &Env,
    effects: &RefCell<Effects>,
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

fn destroy(
    name: &str,
    args: &[Value],
    env: &Env,
    effects: &RefCell<Effects>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [entity] = args else {
        return unsupported(format!("{name} arguments"));
    };
    // `Destroy Effect` targets the kinds its catalog sentence names —
    // `createEffect` and `createBeamEffect`; `Destroy Icon` the icon
    // entities. An unknown, `null` or already-destroyed entity id
    // destroys nothing.
    let kinds: &[EffectKind] = match name {
        "destroyEffect" => &[EffectKind::Effect, EffectKind::Beam],
        _ => &[EffectKind::Icon],
    };
    match eval(entity, env)? {
        Val::Null => {}
        Val::Number(id) => destroy_kinds(effects, kinds, id, tick, changes),
        other => return unsupported(format!("{name} of {other:?}")),
    }
    Ok(())
}

fn destroy_all(
    name: &str,
    args: &[Value],
    effects: &RefCell<Effects>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [] = args else {
        return unsupported(format!("{name} arguments"));
    };
    let kinds: &[EffectKind] = match name {
        "destroyAllEffects" => &[EffectKind::Effect, EffectKind::Beam],
        _ => &[EffectKind::Icon],
    };
    destroy_all_kinds(effects, kinds, tick, changes);
    Ok(())
}
