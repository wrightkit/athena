//! Vector values: the deterministic vector math the catalog defines, and
//! the position/facing reads a scenario's `players[]` supply.
//!
//! The Workshop frame (from the catalog's own descriptions): `x` is left,
//! `y` is up, `z` is forward; a horizontal angle increases as the player
//! rotates left and wraps around at ±180°, a vertical angle increases as
//! the player looks down. A player's rotation is yaw about the world `y`
//! axis by the horizontal angle, then pitch about the local `x` (left)
//! axis by the vertical angle, so `facing = Ryaw(h)·Rpitch(v)·(0,0,1)`.
//! None of it is covered by a client capture — all **modeled, not
//! measured**.

use std::collections::BTreeMap;

use workshop_rs::Value;

use crate::eval::{Env, arguments, eval, number, player_id};
use crate::scenario::Player;
use crate::{MissingInput, MissingInputKind, NotModeled, Stop, Val};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotModeled(NotModeled::new(what)))
}

fn missing<T>(
    input: MissingInputKind,
    player: Option<String>,
    name: Option<String>,
    construct: impl Into<String>,
) -> Result<T, Stop> {
    Err(Stop::MissingInput(MissingInput::new(
        input, player, name, construct,
    )))
}

/// A player's facing as a scenario declares it: the horizontal and
/// vertical angles in degrees `Facing Direction Of` reproduces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Facing {
    /// Degrees turning left from forward; 0 faces `+z`.
    pub horizontal: f64,
    /// Degrees looking down; 0 is level.
    pub vertical: f64,
}

/// A Workshop vector. `Val::Vector` is the serialized form; `Vec3` is the
/// one the math runs on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Vec3 {
    /// Left.
    pub x: f64,
    /// Up.
    pub y: f64,
    /// Forward.
    pub z: f64,
}

impl Vec3 {
    pub(crate) fn map(self, f: impl Fn(f64) -> f64) -> Self {
        Self {
            x: f(self.x),
            y: f(self.y),
            z: f(self.z),
        }
    }

    pub(crate) fn zip(self, other: Self, f: impl Fn(f64, f64) -> f64) -> Self {
        Self {
            x: f(self.x, other.x),
            y: f(self.y, other.y),
            z: f(self.z, other.z),
        }
    }

    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub(crate) fn magnitude(self) -> f64 {
        self.dot(self).sqrt()
    }
}

impl From<Vec3> for Val {
    fn from(v: Vec3) -> Self {
        Val::Vector {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

/// The `Vector` enum's members: the six directional constants of the
/// documented frame (`x` left, `y` up, `z` forward). The catalog records
/// the members but not their components, so the table stands here once.
pub(crate) fn constant(member: &str) -> Option<Vec3> {
    Some(match member {
        "LEFT" => Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        "RIGHT" => Vec3 {
            x: -1.0,
            y: 0.0,
            z: 0.0,
        },
        "UP" => Vec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
        "DOWN" => Vec3 {
            x: 0.0,
            y: -1.0,
            z: 0.0,
        },
        "FORWARD" => Vec3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        "BACKWARD" => Vec3 {
            x: 0.0,
            y: 0.0,
            z: -1.0,
        },
        _ => return None,
    })
}

fn vec(value: &Val, construct: &str) -> Result<Vec3, Stop> {
    match value {
        Val::Vector { x, y, z } => Ok(Vec3 {
            x: *x,
            y: *y,
            z: *z,
        }),
        other => unsupported(format!("{construct} on {other:?}")),
    }
}

/// A produced vector stays a value only while every component is finite;
/// a `NaN` or infinity would serialize as `null` and lie about the result,
/// so the run stops instead.
pub(crate) fn produced(v: Vec3, construct: &str) -> Result<Val, Stop> {
    if v.x.is_finite() && v.y.is_finite() && v.z.is_finite() {
        Ok(v.into())
    } else {
        unsupported(format!("{construct} produced a non-finite vector"))
    }
}

fn produced_number(n: f64, construct: &str) -> Result<Val, Stop> {
    if n.is_finite() {
        Ok(Val::Number(n))
    } else {
        unsupported(format!("{construct} produced a non-finite number"))
    }
}

/// The scenario player's `position`; a live input the run cannot guess.
fn position(players: &BTreeMap<String, Player>, id: &str, construct: &str) -> Result<Vec3, Stop> {
    let Some(player) = players.get(id) else {
        return unsupported(format!("`{id}` is not a scenario player"));
    };
    match player.position {
        Some(position) => Ok(position),
        None => missing(
            MissingInputKind::Position,
            Some(id.to_string()),
            Some("position".to_string()),
            construct,
        ),
    }
}

/// The scenario player's `facing`; likewise a live input.
fn facing(players: &BTreeMap<String, Player>, id: &str, construct: &str) -> Result<Facing, Stop> {
    let Some(player) = players.get(id) else {
        return unsupported(format!("`{id}` is not a scenario player"));
    };
    match player.facing {
        Some(facing) => Ok(facing),
        None => missing(
            MissingInputKind::Facing,
            Some(id.to_string()),
            Some("facing".to_string()),
            construct,
        ),
    }
}

/// What a position argument reads as: a vector as itself, a player as the
/// scenario's `position` for it. Anything else — an array included — is
/// not a position the simulator can name.
pub(crate) fn position_value(
    value: Val,
    players: &BTreeMap<String, Player>,
    construct: &str,
) -> Result<Vec3, Stop> {
    match value {
        Val::Player(id) => position(players, &id, &format!("value {construct}")),
        other => vec(&other, construct),
    }
}

/// A raycast or line-of-sight start/end argument: as `position_value`,
/// except the catalog resolves a `Player` to the position two meters
/// above the player's feet.
pub(crate) fn raycast_position_value(
    value: Val,
    players: &BTreeMap<String, Player>,
    construct: &str,
) -> Result<Vec3, Stop> {
    match value {
        Val::Player(id) => {
            let feet = position(players, &id, &format!("value {construct}"))?;
            Ok(Vec3 {
                y: feet.y + 2.0,
                ..feet
            })
        }
        other => vec(&other, construct),
    }
}

/// An operand of `Add`/`Subtract`/`Multiply`/`Divide`: a number, boolean
/// or `Null` under the number coercion; a vector as itself; a player as
/// its `position`.
pub(crate) enum Operand {
    Scalar(f64),
    Vector(Vec3),
}

pub(crate) fn operand(
    value: Val,
    players: &BTreeMap<String, Player>,
    construct: &str,
) -> Result<Operand, Stop> {
    Ok(match value {
        Val::Vector { x, y, z } => Operand::Vector(Vec3 { x, y, z }),
        Val::Player(id) => Operand::Vector(position(players, &id, &format!("value {construct}"))?),
        other => Operand::Scalar(number(&other)?),
    })
}

/// `Add`/`Subtract`/`Multiply`/`Divide` on numbers and vectors: a number
/// against a vector broadcasts to every component, two vectors combine
/// component-wise. The `Modify` operations of the same names run this.
pub(crate) fn arithmetic(
    name: &str,
    a: Val,
    b: Val,
    players: &BTreeMap<String, Player>,
) -> Result<Val, Stop> {
    let (a, b) = (operand(a, players, name)?, operand(b, players, name)?);
    let op: fn(f64, f64) -> f64 = match name {
        "add" => |a, b| a + b,
        "subtract" => |a, b| a - b,
        "multiply" => |a, b| a * b,
        _ => |a, b| a / b,
    };
    match (a, b) {
        (Operand::Scalar(a), Operand::Scalar(b)) => Ok(Val::Number(op(a, b))),
        (Operand::Vector(a), Operand::Vector(b)) => produced(a.zip(b, op), name),
        (Operand::Vector(v), Operand::Scalar(n)) => produced(v.map(|c| op(c, n)), name),
        (Operand::Scalar(n), Operand::Vector(v)) => produced(v.map(|c| op(n, c)), name),
    }
}

/// The unit vector of a facing: `Ryaw(h)·Rpitch(v)·(0,0,1)` — horizontal
/// 0 is forward (`+z`), positive horizontal turns left (`+x`), positive
/// vertical looks down (`-y`).
fn direction(facing: Facing) -> Vec3 {
    let h = facing.horizontal.to_radians();
    let v = facing.vertical.to_radians();
    Vec3 {
        x: h.sin() * v.cos(),
        y: -v.sin(),
        z: h.cos() * v.cos(),
    }
}

/// `Ryaw(h)·Rpitch(v)·local`: pitch about the local `x` axis, then yaw
/// about the world `y` axis.
fn local_to_world(v: Vec3, facing: Facing) -> Vec3 {
    let (sh, ch) = facing.horizontal.to_radians().sin_cos();
    let (sv, cv) = facing.vertical.to_radians().sin_cos();
    let pitched = Vec3 {
        x: v.x,
        y: v.y * cv - v.z * sv,
        z: v.y * sv + v.z * cv,
    };
    Vec3 {
        x: pitched.x * ch + pitched.z * sh,
        y: pitched.y,
        z: -pitched.x * sh + pitched.z * ch,
    }
}

/// The inverse: `Rpitch(-v)·Ryaw(-h)·world`.
fn world_to_local(v: Vec3, facing: Facing) -> Vec3 {
    let (sh, ch) = facing.horizontal.to_radians().sin_cos();
    let (sv, cv) = facing.vertical.to_radians().sin_cos();
    let unyawed = Vec3 {
        x: v.x * ch - v.z * sh,
        y: v.y,
        z: v.x * sh + v.z * ch,
    };
    Vec3 {
        x: unyawed.x,
        y: unyawed.y * cv + unyawed.z * sv,
        z: -unyawed.y * sv + unyawed.z * cv,
    }
}

/// The `Transform` member a `Local`/`World Vector Of` argument evaluates
/// to — an enum literal or an evaluated member-id text.
fn transform(value: &Value, env: &Env) -> Result<String, Stop> {
    match value {
        Value::Enum { value, .. } => Ok(value.clone()),
        other => match eval(other, env)? {
            Val::Text(member) => Ok(member),
            other => unsupported(format!("transform {other:?}")),
        },
    }
}

/// The vector-family `Value::Call` values.
pub(crate) fn call(name: &str, args: &[Value], env: &Env) -> Result<Val, Stop> {
    match name {
        "vector" => {
            let [x, y, z] = arguments::<3>(name, args)?;
            let v = Vec3 {
                x: number(&eval(x, env)?)?,
                y: number(&eval(y, env)?)?,
                z: number(&eval(z, env)?)?,
            };
            produced(v, name)
        }
        "__xComponentOf__" | "__yComponentOf__" | "__zComponentOf__" => {
            let [arg] = arguments::<1>(name, args)?;
            let v = vec(&eval(arg, env)?, name)?;
            Ok(Val::Number(match name {
                "__xComponentOf__" => v.x,
                "__yComponentOf__" => v.y,
                _ => v.z,
            }))
        }
        // `Vector Towards` is the raw displacement, `Direction Towards`
        // its unit direction — the same operand pair `Distance Between`
        // measures.
        "vectorTowards" => {
            let [start, end] = arguments::<2>(name, args)?;
            let (a, b) = (
                position_value(eval(start, env)?, env.players, name)?,
                position_value(eval(end, env)?, env.players, name)?,
            );
            produced(b.zip(a, |b, a| b - a), name)
        }
        "distance" => {
            let [start, end] = arguments::<2>(name, args)?;
            let (a, b) = (
                position_value(eval(start, env)?, env.players, name)?,
                position_value(eval(end, env)?, env.players, name)?,
            );
            produced_number(b.zip(a, |b, a| b - a).magnitude(), name)
        }
        "directionTowards" => {
            let [start, end] = arguments::<2>(name, args)?;
            let (a, b) = (
                position_value(eval(start, env)?, env.players, name)?,
                position_value(eval(end, env)?, env.players, name)?,
            );
            let displacement = b.zip(a, |b, a| b - a);
            let magnitude = displacement.magnitude();
            if magnitude == 0.0 {
                return unsupported(format!("{name} between identical positions"));
            }
            produced(displacement.map(|c| c / magnitude), name)
        }
        "directionFromAngles" => {
            let [h, v] = arguments::<2>(name, args)?;
            produced(
                direction(Facing {
                    horizontal: number(&eval(h, env)?)?,
                    vertical: number(&eval(v, env)?)?,
                }),
                name,
            )
        }
        "dotProduct" => {
            let [a, b] = arguments::<2>(name, args)?;
            produced_number(
                vec(&eval(a, env)?, name)?.dot(vec(&eval(b, env)?, name)?),
                name,
            )
        }
        "crossProduct" => {
            let [a, b] = arguments::<2>(name, args)?;
            let (a, b) = (vec(&eval(a, env)?, name)?, vec(&eval(b, env)?, name)?);
            produced(
                Vec3 {
                    x: a.y * b.z - a.z * b.y,
                    y: a.z * b.x - a.x * b.z,
                    z: a.x * b.y - a.y * b.x,
                },
                name,
            )
        }
        "normalize" => {
            let [arg] = arguments::<1>(name, args)?;
            let v = vec(&eval(arg, env)?, name)?;
            let magnitude = v.magnitude();
            if magnitude == 0.0 {
                return unsupported(format!("{name} of a zero-length vector"));
            }
            produced(v.map(|c| c / magnitude), name)
        }
        "magnitude" => {
            let [arg] = arguments::<1>(name, args)?;
            produced_number(vec(&eval(arg, env)?, name)?.magnitude(), name)
        }
        // Degrees, `acos` of the unit directions' dot product; a
        // zero-length vector has no direction, so the angle is not
        // modeled.
        "angleBetweenVectors" => {
            let [a, b] = arguments::<2>(name, args)?;
            let (a, b) = (vec(&eval(a, env)?, name)?, vec(&eval(b, env)?, name)?);
            let magnitudes = a.magnitude() * b.magnitude();
            if magnitudes == 0.0 {
                return unsupported(format!("{name} on a zero-length vector"));
            }
            let cosine = (a.dot(b) / magnitudes).clamp(-1.0, 1.0);
            produced_number(cosine.acos().to_degrees(), name)
        }
        "horizontalAngleFromDirection" | "verticalAngleFromDirection" => {
            let [arg] = arguments::<1>(name, args)?;
            let v = vec(&eval(arg, env)?, name)?;
            let angle = if name == "horizontalAngleFromDirection" {
                v.x.atan2(v.z)
            } else {
                (-v.y).atan2(v.x.hypot(v.z))
            };
            produced_number(angle.to_degrees(), name)
        }
        // A position read is a live input the scenario supplies per
        // player; `Position Of` takes `Player|Array`, and an array is not
        // a single position the simulator can name.
        "getPosition" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            position(env.players, &id, &format!("value {name}")).map(Val::from)
        }
        // `Eye Position` adds a hero-specific eye height the catalog does
        // not record, so it cannot follow from `position` alone.
        "getEyePosition" => unsupported("value getEyePosition: eye height is not cataloged"),
        // `Objective Position` reads the scenario's `objectives` list at
        // the evaluated argument — the catalog's "index of the objective
        // to consider, starting at 0 and counting up" — under the number
        // coercion, so `False`/`True` read as 0/1. The map and mode
        // decide the positions outside the script; an argument naming no
        // entry — the field absent, or the index out of range, negative,
        // fractional or non-finite — is a missing input, never a guess.
        "getObjectivePosition" => {
            let [index] = arguments::<1>(name, args)?;
            let index = number(&eval(index, env)?)?;
            let entry = match env.objectives {
                Some(objectives) if index >= 0.0 && index.fract() == 0.0 => {
                    objectives.get(index as usize)
                }
                _ => None,
            };
            match entry {
                Some(position) => Ok(Val::from(*position)),
                None => missing(
                    MissingInputKind::Objectives,
                    None,
                    Some(format!("objectives[{index}]")),
                    format!("value {name}"),
                ),
            }
        }
        "getFacingDirection" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            produced(
                direction(facing(env.players, &id, &format!("value {name}"))?),
                name,
            )
        }
        // `Local Vector Of`/`World Vector Of` transform a vector into or
        // out of a player's facing frame: `ROTATION` needs `facing`,
        // `ROTATION_AND_TRANSLATION` needs `facing` and `position` —
        // all of it plain math on declared inputs, no geometry.
        "localVector" | "worldVector" => {
            let [vector, player, transformation] = arguments::<3>(name, args)?;
            let (v, id, transform) = (
                vec(&eval(vector, env)?, name)?,
                player_id(player, env)?,
                transform(transformation, env)?,
            );
            let facing = facing(env.players, &id, &format!("value {name}"))?;
            let rotated = match (name, transform.as_str()) {
                ("localVector", "ROTATION") => world_to_local(v, facing),
                ("worldVector", "ROTATION") => local_to_world(v, facing),
                ("localVector", "ROTATION_AND_TRANSLATION") => {
                    let origin = position(env.players, &id, &format!("value {name}"))?;
                    world_to_local(v.zip(origin, |v, o| v - o), facing)
                }
                ("worldVector", "ROTATION_AND_TRANSLATION") => {
                    let origin = position(env.players, &id, &format!("value {name}"))?;
                    origin.zip(local_to_world(v, facing), |o, v| o + v)
                }
                (_, other) => return unsupported(format!("{name} transform {other}")),
            };
            produced(rotated, name)
        }
        // The empty-world geometry reads have their own dispatcher; it
        // ends unknown names in `not_modeled` the same way.
        other => crate::geometry::call(other, args, env),
    }
}
