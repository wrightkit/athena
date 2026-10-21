//! The player state table: catalog-derived world state for scenario
//! players, and the `Damage`/`Heal` rules that move it.
//!
//! The table is built at the start of a run. Each player's state comes from
//! `workshop_rs::hero_values` at the pinned version; a hero the catalog
//! cannot supply leaves the row unresolved, and touching that player's world
//! state (`Damage`, `Heal`) stops the run as `not_modeled`. Pools, cooldown
//! seconds and ultimate cost are never defaulted or invented. No capture
//! backs the pool rules; `docs/architecture.md` marks them **modeled**.

use workshop_rs::gameplay;
use workshop_rs::hero_values::{HealthPool, HeroValues};

use crate::{NotModeled, PlayerWorld, PoolState, UltimateState};

/// The resolved world state of a scenario player.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WorldSlot {
    /// Loaded but never resolved; `run` resolves every player before the
    /// first tick, so a run never observes this.
    Unresolved,
    /// The player's world state.
    Resolved(PlayerWorld),
    /// The catalog could not supply the state; a `Damage`/`Heal` on the
    /// player stops the run with this error.
    Failed(NotModeled),
}

/// The gameplay-dataset hero id behind a scenario hero: catalog `Hero`
/// members and gameplay heroes join on the en-US name, so the member's
/// en-US spellings (as captured in `Player::hero_spellings`) name it.
fn hero_id(spellings: &[String], gameplay: &gameplay::GameplayCatalog) -> Option<String> {
    gameplay
        .heroes()
        .iter()
        .find(|known| {
            known.name().value().get("en-US").is_some_and(|name| {
                spellings
                    .iter()
                    .any(|spelling| spelling.eq_ignore_ascii_case(name))
            })
        })
        .map(|known| known.id().as_str().to_string())
}

/// Build a player's world state from the hero-values catalog. `player` is
/// the scenario `id` and `hero` the scenario hero name, used only in error
/// text.
pub(crate) fn resolve(
    player: &str,
    hero: &str,
    spellings: &[String],
    values: &HeroValues,
) -> Result<PlayerWorld, NotModeled> {
    let unmodeled = |what: String| NotModeled::new(format!("player `{player}`: {what}"));
    let gameplay = gameplay::data::builtin_ref()
        .map_err(|error| unmodeled(format!("gameplay dataset unavailable: {error}")))?;
    let hero_id = hero_id(spellings, gameplay)
        .ok_or_else(|| unmodeled(format!("hero `{hero}` is not in the gameplay dataset")))?;

    // Every hero has a `health` pool and an ultimate cost; `shield` and
    // `armor` exist only where the catalog records them (a hero without the
    // pool has no entry, never a zero).
    let pool = |kind: HealthPool, field: &str| {
        values
            .health(&hero_id, kind)
            .map(|max| PoolState { current: max, max })
            .ok_or_else(|| unmodeled(format!("hero `{hero}` has no `{field}` hero value")))
    };
    let health = pool(HealthPool::Health, "health")?;
    let shield = values
        .health(&hero_id, HealthPool::Shield)
        .map(|max| PoolState { current: max, max });
    let armor = values
        .health(&hero_id, HealthPool::Armor)
        .map(|max| PoolState { current: max, max });
    let cost = values
        .ultimate_cost(&hero_id)
        .ok_or_else(|| unmodeled(format!("hero `{hero}` has no `ultimateCost` hero value")))?;

    // Cooldowns are reported for the ability slots the catalog records;
    // an ability without an entry has no cooldown and is simply absent.
    let mut cooldowns = std::collections::BTreeMap::new();
    let record = gameplay
        .hero_by_id(&hero_id)
        .expect("hero_id comes from the gameplay dataset");
    for ability in record.abilities() {
        let slot = ability.slot().as_str();
        let variant = ability.variant().map(|variant| variant.as_str());
        if let Some(seconds) = values.cooldown(&hero_id, slot, variant) {
            let key = variant.map_or_else(|| slot.to_string(), |v| format!("{slot}/{v}"));
            cooldowns.insert(key, seconds);
        }
    }

    Ok(PlayerWorld {
        health,
        shield,
        armor,
        cooldowns,
        ultimate: UltimateState { charge: 0.0, cost },
    })
}

/// `Damage(player, damager, amount)`: the pools take the amount in shield,
/// armor, health order; each pool absorbs up to its `current` and the rest
/// spills into the next. Health floors at zero; a negative amount does
/// nothing. `damager` has no model.
pub(crate) fn damage(world: &mut PlayerWorld, amount: f64) {
    let mut rest = amount.max(0.0);
    for pool in [world.shield.as_mut(), world.armor.as_mut()] {
        let Some(pool) = pool else { continue };
        let taken = pool.current.min(rest);
        pool.current -= taken;
        rest -= taken;
    }
    world.health.current = (world.health.current - rest).max(0.0);
}

/// `Heal(player, healer, amount)`: the `health` pool rises by the amount,
/// capped at `max`; a negative amount does nothing. `healer` has no model.
pub(crate) fn heal(world: &mut PlayerWorld, amount: f64) {
    world.health.current = (world.health.current + amount.max(0.0)).min(world.health.max);
}
