//! Hero settings under `settings.heroes` that move the player state table.
//!
//! A player reads the `allTeams` group and its own team group, and inside
//! each the group named by its hero id: `heroes.allTeams.*`,
//! `heroes.<team>.*`, `heroes.allTeams.<hero>.*` and
//! `heroes.<team>.<hero>.*`. Percent scalars stack multiplicatively across
//! every authored occurrence.
//!
//! Applied settings: `health%` scales every pool, `damageDealt%` and
//! `damageReceived%` scale `Damage`, `healingDealt%` and `healingReceived%`
//! scale `Heal`, and the cooldown scalars — team `abilityCooldown%` plus
//! `ability1Cooldown%`, `ability2Cooldown%`, `ability3Cooldown%` and
//! `secondaryFireCooldown%` — scale the reported cooldowns.
//!
//! Settings whose effect stays outside the reported state — movement,
//! projectiles, ammunition, weapon and ability toggles, hero lists, ability
//! and passive parameters — are ignored. An authored setting that could move
//! a reported value but is not applied fails the player's world resolution
//! instead of falling back to the base value:
//!
//! - `ultGen%`, `passiveUltGen%`, `combatUltGen%` move the reported
//!   ultimate charge over time;
//! - `enableSpawningWithUlt` changes the charge a player starts with;
//! - `passiveHealthRegen` and `enableRolePassive` move the health pool over
//!   time;
//! - `enableHeadshotsOnly` gates whether `Damage` lands;
//! - `spawnWithoutMech` changes the hero the pools come from;
//! - `shieldBashCooldown%` scales a reported cooldown but has no public
//!   slot mapping to apply it through.
//!
//! When the player declares a version-2 `state` map, a setting in the
//! first three groups is skipped rather than failed when the map declares
//! every `state` key the setting can move — `ultCharge` for the ultimate
//! settings, `health`, `maxHealth` and `normalizedHealth` for the health
//! ones — since the scenario supplies the value the setting could move
//! (athena#91). A skipped setting means the scenario-supplied value was
//! used, not that the setting was simulated.
//!
//! A `Raw`/`RawValue`/`RawGroup` member is one the semantic catalog cannot
//! fully declare; it is reported the same way, since nothing can show it is
//! inert for player state. No capture backs these rules yet; the trace
//! labels them modeled (`docs/architecture.md`).

use std::collections::BTreeMap;

use workshop_rs::settings::{PathPart, Settings, SettingsNode, definition};

use crate::Val;
use crate::scenario::Team;

/// Per-player multipliers collected from the applied hero settings. Every
/// factor starts at `1.0`; each authored occurrence multiplies in.
#[derive(Debug, Clone)]
pub(crate) struct HeroModifiers {
    /// `health%` scale applied to every pool.
    pub health: f64,
    /// `damageDealt%` when this player deals damage.
    pub damage_dealt: f64,
    /// `damageReceived%` when this player takes damage.
    pub damage_received: f64,
    /// `healingDealt%` when this player heals.
    pub healing_dealt: f64,
    /// `healingReceived%` when this player is healed.
    pub healing_received: f64,
    /// Team-scope `abilityCooldown%` scale shared by every slot.
    pub cooldown_all: f64,
    /// Per-slot `abilityNCooldown%`/`secondaryFireCooldown%` scales by slot.
    pub cooldown_slots: BTreeMap<String, f64>,
}

/// The team groups a player reads: `allTeams` plus its own team.
fn applies_to_team(group: &str, team: Option<Team>) -> bool {
    group == "allTeams" || team.is_some_and(|team| team.as_str() == group)
}

/// The reported cooldown slot a `*Cooldown%` setting scales, or `None` for
/// `abilityCooldown%` (team scope), which scales every slot.
fn cooldown_slot(key: &str) -> Option<Option<&'static str>> {
    match key {
        "abilityCooldown%" => Some(None),
        "ability1Cooldown%" => Some(Some("ability1")),
        "ability2Cooldown%" => Some(Some("ability2")),
        "ability3Cooldown%" => Some(Some("ability3")),
        "secondaryFireCooldown%" => Some(Some("secondaryFire")),
        _ => None,
    }
}

/// Settings that could move a reported value or a `Damage`/`Heal` amount
/// but are not applied; an authored occurrence marks the player
/// `not_modeled` on touch. Each row's second element is the `state` keys
/// the setting can move: when the player's version-2 `state` map declares
/// every one, the scenario supplies the value and the setting is skipped
/// (`state.<key>` stands; the setting is not simulated). An empty list
/// means no `state` keys cover the setting — it always fails.
const UNAPPLIED_STATE: &[(&str, &[&str])] = &[
    ("ultGen%", &["ultCharge"]),
    ("passiveUltGen%", &["ultCharge"]),
    ("combatUltGen%", &["ultCharge"]),
    ("enableSpawningWithUlt", &["ultCharge"]),
    (
        "passiveHealthRegen",
        &["health", "maxHealth", "normalizedHealth"],
    ),
    (
        "enableRolePassive",
        &["health", "maxHealth", "normalizedHealth"],
    ),
    ("enableHeadshotsOnly", &[]),
    ("spawnWithoutMech", &[]),
    ("shieldBashCooldown%", &[]),
];

/// Whether the leaf sits inside the player's hero group or directly under
/// the team group.
#[derive(Clone, Copy)]
enum Scope {
    Team,
    Hero,
}

/// Whether the settings catalog declares `key` at `scope`; a member
/// outside it cannot be shown inert for player state.
fn catalogued(key: &str, scope: Scope) -> bool {
    let scope_path: &[PathPart<'_>] = match scope {
        Scope::Team => &[PathPart::Team],
        Scope::Hero => &[PathPart::Team, PathPart::Hero],
    };
    let mut path = vec![PathPart::Part("heroes")];
    path.extend_from_slice(scope_path);
    path.push(PathPart::Part(key));
    definition(&path).is_some()
}

/// Read the hero settings a player's scope authors into its modifiers.
/// `state` is the player's version-2 `state` map, when the scenario
/// declares one: a skippable setting whose `state` keys are all declared
/// is skipped. The error names the setting key that is authored but
/// cannot be applied, plus the `state` keys that would clear it.
pub(crate) fn modifiers(
    settings: Option<&Settings>,
    team: Option<Team>,
    hero_id: &str,
    state: Option<&BTreeMap<String, Val>>,
) -> Result<HeroModifiers, String> {
    let mut mods = HeroModifiers {
        health: 1.0,
        damage_dealt: 1.0,
        damage_received: 1.0,
        healing_dealt: 1.0,
        healing_received: 1.0,
        cooldown_all: 1.0,
        cooldown_slots: BTreeMap::new(),
    };
    let Some(settings) = settings else {
        return Ok(mods);
    };
    let Some(heroes) = settings
        .children
        .iter()
        .find(|node| matches!(node, SettingsNode::Group { name, .. } if name == "heroes"))
    else {
        return Ok(mods);
    };
    let SettingsNode::Group {
        children: teams, ..
    } = heroes
    else {
        return Ok(mods);
    };
    for team_group in teams {
        let SettingsNode::Group {
            name: team_name,
            children: members,
            ..
        } = team_group
        else {
            continue;
        };
        if !applies_to_team(team_name, team) {
            continue;
        }
        for member in members {
            match member {
                SettingsNode::Group { name, children, .. } if name == hero_id => {
                    for leaf in children {
                        leaf_modifier(&mut mods, leaf, Scope::Hero, state)?;
                    }
                }
                SettingsNode::Group { .. } => {}
                leaf => leaf_modifier(&mut mods, leaf, Scope::Team, state)?,
            }
        }
    }
    Ok(mods)
}

/// The `not_modeled` message for an authored setting the simulator does
/// not apply, naming the `state` keys that would clear it when the
/// setting has `state` coverage.
fn unapplied(name: &str, missing: &[&str]) -> String {
    let message = format!("hero setting `{name}` is not applied");
    if missing.is_empty() {
        return message;
    }
    let ids = missing
        .iter()
        .map(|key| format!("`state.{key}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{message}; declaring {ids} would clear it")
}

/// Whether the member carries an opaque value the catalog could not type
/// (`Raw`/`RawValue`/`RawGroup`).
fn opaque(leaf: &SettingsNode) -> bool {
    matches!(
        leaf,
        SettingsNode::Raw { .. } | SettingsNode::RawValue { .. } | SettingsNode::RawGroup { .. }
    )
}

/// Fold one authored member into the modifiers, or fail with the key of a
/// setting the simulator cannot apply to a reported value. `state` is the
/// player's version-2 `state` map: an unapplied setting whose `state`
/// keys are all declared is skipped.
fn leaf_modifier(
    mods: &mut HeroModifiers,
    leaf: &SettingsNode,
    scope: Scope,
    state: Option<&BTreeMap<String, Val>>,
) -> Result<(), String> {
    let name = leaf.name();
    let number = || match leaf {
        SettingsNode::Number { value, .. } => Ok(*value / 100.0),
        _ => Err(unapplied(name, &[])),
    };
    match name {
        "health%" => mods.health *= number()?,
        "damageDealt%" => mods.damage_dealt *= number()?,
        "damageReceived%" => mods.damage_received *= number()?,
        "healingDealt%" => mods.healing_dealt *= number()?,
        "healingReceived%" => mods.healing_received *= number()?,
        _ => match cooldown_slot(name) {
            Some(None) if matches!(scope, Scope::Team) => mods.cooldown_all *= number()?,
            Some(Some(slot)) if matches!(scope, Scope::Hero) => {
                *mods.cooldown_slots.entry(slot.to_string()).or_insert(1.0) *= number()?;
            }
            // A cooldown scalar authored at a scope it does not belong
            // to is authored but not applied.
            Some(_) => return Err(unapplied(name, &[])),
            None => match UNAPPLIED_STATE.iter().find(|(key, _)| *key == name) {
                // `Raw*` members and settings no `state` keys cover
                // keep failing exactly as before.
                Some((_, keys)) if keys.is_empty() || opaque(leaf) => {
                    return Err(unapplied(name, &[]));
                }
                Some((_, keys)) => {
                    let missing = keys
                        .iter()
                        .copied()
                        .filter(|key| !state.is_some_and(|map| map.contains_key(*key)))
                        .collect::<Vec<_>>();
                    if !missing.is_empty() {
                        return Err(unapplied(name, &missing));
                    }
                    // The scenario supplies every value the setting
                    // could move: the setting is skipped, not
                    // simulated.
                }
                // An undeclared member cannot be shown inert.
                None if !declared(leaf, scope) => return Err(unapplied(name, &[])),
                None => {}
            },
        },
    }
    Ok(())
}

/// Whether the member's name is one the settings catalog declares at this
/// scope; typed leaves always are.
fn declared(leaf: &SettingsNode, scope: Scope) -> bool {
    if opaque(leaf) {
        return catalogued(leaf.name(), scope);
    }
    true
}
