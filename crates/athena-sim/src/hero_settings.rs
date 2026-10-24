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
//! A `Raw`/`RawValue`/`RawGroup` member is one the semantic catalog cannot
//! fully declare; it is reported the same way, since nothing can show it is
//! inert for player state. No capture backs these rules yet; the trace
//! labels them modeled (`docs/architecture.md`).

use std::collections::BTreeMap;

use workshop_rs::settings::{PathPart, Settings, SettingsNode, definition};

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
/// `not_modeled` on touch.
const UNAPPLIED_STATE: &[&str] = &[
    "ultGen%",
    "passiveUltGen%",
    "combatUltGen%",
    "enableSpawningWithUlt",
    "passiveHealthRegen",
    "enableRolePassive",
    "enableHeadshotsOnly",
    "spawnWithoutMech",
    "shieldBashCooldown%",
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
/// The error names the setting key that is authored but cannot be applied.
pub(crate) fn modifiers(
    settings: Option<&Settings>,
    team: Option<Team>,
    hero_id: &str,
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
        let unapplied = |key: String| format!("hero setting `{key}` is not applied");
        for member in members {
            match member {
                SettingsNode::Group { name, children, .. } if name == hero_id => {
                    for leaf in children {
                        leaf_modifier(&mut mods, leaf, Scope::Hero).map_err(unapplied)?;
                    }
                }
                SettingsNode::Group { .. } => {}
                leaf => leaf_modifier(&mut mods, leaf, Scope::Team).map_err(unapplied)?,
            }
        }
    }
    Ok(mods)
}

/// Fold one authored member into the modifiers, or fail with the key of a
/// setting the simulator cannot apply to a reported value.
fn leaf_modifier(
    mods: &mut HeroModifiers,
    leaf: &SettingsNode,
    scope: Scope,
) -> Result<(), String> {
    let name = leaf.name();
    let number = || match leaf {
        SettingsNode::Number { value, .. } => Ok(*value / 100.0),
        _ => Err(name.to_string()),
    };
    match name {
        "health%" => mods.health *= number()?,
        "damageDealt%" => mods.damage_dealt *= number()?,
        "damageReceived%" => mods.damage_received *= number()?,
        "healingDealt%" => mods.healing_dealt *= number()?,
        "healingReceived%" => mods.healing_received *= number()?,
        _ => {
            let unapplied = || Err(name.to_string());
            match cooldown_slot(name) {
                Some(None) if matches!(scope, Scope::Team) => mods.cooldown_all *= number()?,
                Some(Some(slot)) if matches!(scope, Scope::Hero) => {
                    *mods.cooldown_slots.entry(slot.to_string()).or_insert(1.0) *= number()?;
                }
                // A cooldown scalar authored at a scope it does not belong
                // to is authored but not applied.
                Some(_) => return unapplied(),
                None if UNAPPLIED_STATE.contains(&name) => return unapplied(),
                // An undeclared member cannot be shown inert.
                None if !declared(leaf, scope) => return unapplied(),
                None => {}
            }
        }
    }
    Ok(())
}

/// Whether the member's name is one the settings catalog declares at this
/// scope; typed leaves always are.
fn declared(leaf: &SettingsNode, scope: Scope) -> bool {
    match leaf {
        SettingsNode::Raw { .. }
        | SettingsNode::RawValue { .. }
        | SettingsNode::RawGroup { .. } => catalogued(leaf.name(), scope),
        _ => true,
    }
}
