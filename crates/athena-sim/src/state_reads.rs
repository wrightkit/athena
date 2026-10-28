//! The version-2 `state` map's read table: the no-extra-argument
//! player reads a scenario player's `state` object answers, and the
//! per-key `state` JSON value to `Val` mapping.

use crate::{ScenarioError, Val};

/// The JSON type a `state` key takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateType {
    Bool,
    Number,
    Vector,
}

impl StateType {
    /// The article-and-name a malformed-scenario error reports.
    fn desc(self) -> &'static str {
        match self {
            Self::Bool => "a boolean",
            Self::Number => "a number",
            Self::Vector => "a vector",
        }
    }
}

/// One row of the player-state table (athena#89): the no-extra-argument
/// player reads a version-2 `state` map answers. `id` is the catalog
/// value id the program spells, `key` the `state` map key — the `id`
/// minus a `get` prefix — and `ty` the key's JSON type. A scenario
/// supplies every value; nothing is derived or defaulted except
/// `Speed Of`, which is the length of `velocity` (`derived`), while
/// `Is Moving` is supplied, not derived.
pub(crate) struct StateRead {
    /// The catalog value id the program spells (`getHealth`).
    pub id: &'static str,
    /// The `state` key it reads (`health`).
    pub key: &'static str,
    /// The key's JSON type.
    pub ty: StateType,
    /// `getSpeed` alone: the read is the key's vector length, not the
    /// stored value.
    pub derived: bool,
}

/// The player-state table: one row per modeled no-extra-argument
/// player read. Adding a read is one row plus a test.
pub(crate) const STATE_READS: &[StateRead] = &[
    StateRead {
        id: "getHealth",
        key: "health",
        ty: StateType::Number,
        derived: false,
    },
    StateRead {
        id: "getMaxHealth",
        key: "maxHealth",
        ty: StateType::Number,
        derived: false,
    },
    StateRead {
        id: "getNormalizedHealth",
        key: "normalizedHealth",
        ty: StateType::Number,
        derived: false,
    },
    StateRead {
        id: "getVelocity",
        key: "velocity",
        ty: StateType::Vector,
        derived: false,
    },
    StateRead {
        id: "getSpeed",
        key: "velocity",
        ty: StateType::Number,
        derived: true,
    },
    StateRead {
        id: "getEyePosition",
        key: "eyePosition",
        ty: StateType::Vector,
        derived: false,
    },
    StateRead {
        id: "getUltCharge",
        key: "ultCharge",
        ty: StateType::Number,
        derived: false,
    },
    StateRead {
        id: "isDead",
        key: "isDead",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isUsingUltimate",
        key: "isUsingUltimate",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isMeleeing",
        key: "isMeleeing",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isFiringSecondary",
        key: "isFiringSecondary",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isInAlternateForm",
        key: "isInAlternateForm",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isCommunicatingEmote",
        key: "isCommunicatingEmote",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isInSpawnRoom",
        key: "isInSpawnRoom",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isDummy",
        key: "isDummy",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isMoving",
        key: "isMoving",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isUsingAbility1",
        key: "isUsingAbility1",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isUsingAbility2",
        key: "isUsingAbility2",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isFiringPrimaryFire",
        key: "isFiringPrimaryFire",
        ty: StateType::Bool,
        derived: false,
    },
    StateRead {
        id: "isDuplicatingAHero",
        key: "isDuplicatingAHero",
        ty: StateType::Bool,
        derived: false,
    },
];

/// The read a value `id` names, when `id` is a player-state value.
pub(crate) fn state_read(id: &str) -> Option<&'static StateRead> {
    STATE_READS.iter().find(|read| read.id == id)
}

/// The table row a `state` map `key` declares, when `key` is a known
/// `state` key. `derived` rows name no key of their own.
pub(crate) fn state_key(key: &str) -> Option<&'static StateRead> {
    STATE_READS
        .iter()
        .find(|read| read.key == key && !read.derived)
}

/// The `{ "x": n, "y": n, "z": n }` a `state` vector takes; anything
/// else — a missing or extra key, a non-number component — is malformed.
fn state_vector(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<crate::vector::Vec3> {
    if object.len() != 3 {
        return None;
    }
    Some(crate::vector::Vec3 {
        x: object.get("x")?.as_f64()?,
        y: object.get("y")?.as_f64()?,
        z: object.get("z")?.as_f64()?,
    })
}

/// One `state` map entry to its `Val`; a wrong JSON type is malformed
/// naming the key.
pub(crate) fn state_value(
    read: &StateRead,
    player: &str,
    value: &serde_json::Value,
) -> Result<Val, ScenarioError> {
    let mistyped = || {
        ScenarioError::Format(format!(
            "player `{player}` `state.{}` must be {}",
            read.key,
            read.ty.desc()
        ))
    };
    Ok(match (read.ty, value) {
        (StateType::Bool, serde_json::Value::Bool(b)) => Val::Bool(*b),
        (StateType::Number, serde_json::Value::Number(n)) => Val::Number(crate::Finite::new(
            n.as_f64()
                .filter(|number| number.is_finite())
                .ok_or_else(|| ScenarioError::Format(format!("number {n} is too large")))?,
        )),
        (StateType::Vector, serde_json::Value::Object(object)) => match state_vector(object) {
            Some(vector) => vector.into(),
            None => return Err(mistyped()),
        },
        _ => return Err(mistyped()),
    })
}
