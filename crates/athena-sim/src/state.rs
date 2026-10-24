//! The player state table: catalog-derived world state for scenario
//! players, and the `Damage`/`Heal` rules that move it.
//!
//! The table is built at the start of a run. Each player's state comes from
//! `workshop_rs::hero_values` at the pinned version, scaled by the hero
//! settings the program authors (`src/hero_settings.rs`); a hero the
//! catalog cannot supply — or a hero setting the simulator does not apply —
//! leaves the row unresolved, and touching that player's world state
//! (`Damage`, `Heal`) stops the run as `not_modeled`. Pools, cooldown
//! seconds and ultimate cost are never defaulted or invented. No capture
//! backs the pool rules; `docs/architecture.md` marks them **modeled**.

use workshop_rs::gameplay;
use workshop_rs::hero_values::{HealthPool, HeroValues};
use workshop_rs::settings::Settings;

use crate::hero_settings;
use crate::scenario::Team;
use crate::{NotModeled, PlayerWorld, PoolState, UltimateState};

/// The resolved world state of a scenario player.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WorldSlot {
    /// Loaded but never resolved; `run` resolves every player before the
    /// first tick, so a run never observes this.
    Unresolved,
    /// The player's world state.
    Resolved(Resolved),
    /// The catalog could not supply the state, or a hero setting the
    /// simulator does not apply was authored for the player; a
    /// `Damage`/`Heal` on the player stops the run with this error.
    Failed(NotModeled),
}

/// A resolved player: the reported world state plus the hero-setting
/// scales `Damage`/`Heal` consult for the dealer and the target.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Resolved {
    /// The reported world state.
    pub world: PlayerWorld,
    /// `damageDealt%` scale when this player deals damage.
    pub damage_dealt: f64,
    /// `damageReceived%` scale when this player takes damage.
    pub damage_received: f64,
    /// `healingDealt%` scale when this player heals.
    pub healing_dealt: f64,
    /// `healingReceived%` scale when this player is healed.
    pub healing_received: f64,
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

/// Build a player's world state from the hero-values catalog and the
/// program's hero settings. `player` is the scenario `id` and `hero` the
/// scenario hero name, used only in error text.
pub(crate) fn resolve(
    player: &str,
    hero: &str,
    spellings: &[String],
    team: Option<Team>,
    settings: Option<&Settings>,
    values: &HeroValues,
) -> Result<Resolved, NotModeled> {
    let unmodeled = |what: String| NotModeled::new(format!("player `{player}`: {what}"));
    let gameplay = gameplay::data::builtin_ref()
        .map_err(|error| unmodeled(format!("gameplay dataset unavailable: {error}")))?;
    let hero_id = hero_id(spellings, gameplay)
        .ok_or_else(|| unmodeled(format!("hero `{hero}` is not in the gameplay dataset")))?;
    let mods = hero_settings::modifiers(settings, team, &hero_id).map_err(unmodeled)?;

    // Every hero has a `health` pool and an ultimate cost; `shield` and
    // `armor` exist only where the catalog records them (a hero without the
    // pool has no entry, never a zero). `health%` scales every pool.
    let pool = |kind: HealthPool, field: &str| {
        values
            .health(&hero_id, kind)
            .map(|max| PoolState {
                current: max * mods.health,
                max: max * mods.health,
            })
            .ok_or_else(|| unmodeled(format!("hero `{hero}` has no `{field}` hero value")))
    };
    let health = pool(HealthPool::Health, "health")?;
    let shield = values
        .health(&hero_id, HealthPool::Shield)
        .map(|max| PoolState {
            current: max * mods.health,
            max: max * mods.health,
        });
    let armor = values
        .health(&hero_id, HealthPool::Armor)
        .map(|max| PoolState {
            current: max * mods.health,
            max: max * mods.health,
        });
    let cost = values
        .ultimate_cost(&hero_id)
        .ok_or_else(|| unmodeled(format!("hero `{hero}` has no `ultimateCost` hero value")))?;

    // Cooldowns are reported for the ability slots the catalog records;
    // an ability without an entry has no cooldown and is simply absent.
    // Team `abilityCooldown%` scales every slot; a hero-scope
    // `*Cooldown%` scales only its own.
    let mut cooldowns = std::collections::BTreeMap::new();
    let record = gameplay
        .hero_by_id(&hero_id)
        .expect("hero_id comes from the gameplay dataset");
    for ability in record.abilities() {
        let slot = ability.slot().as_str();
        let variant = ability.variant().map(|variant| variant.as_str());
        if let Some(seconds) = values.cooldown(&hero_id, slot, variant) {
            let scale = mods.cooldown_all * mods.cooldown_slots.get(slot).copied().unwrap_or(1.0);
            let key = variant.map_or_else(|| slot.to_string(), |v| format!("{slot}/{v}"));
            cooldowns.insert(key, seconds * scale);
        }
    }

    Ok(Resolved {
        world: PlayerWorld {
            health,
            shield,
            armor,
            cooldowns,
            ultimate: UltimateState { charge: 0.0, cost },
        },
        damage_dealt: mods.damage_dealt,
        damage_received: mods.damage_received,
        healing_dealt: mods.healing_dealt,
        healing_received: mods.healing_received,
    })
}

/// `Damage(player, damager, amount)`: the pools take the amount in shield,
/// armor, health order; each pool absorbs up to its `current` and the rest
/// spills into the next. Health floors at zero; a negative amount does
/// nothing.
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
/// capped at `max`; a negative amount does nothing.
pub(crate) fn heal(world: &mut PlayerWorld, amount: f64) {
    world.health.current = (world.health.current + amount.max(0.0)).min(world.health.max);
}
