//! The empty-world model for geometry reads (athena#75, athena#77): the
//! run has no map geometry, so a raycast never hits, line of sight
//! always holds, and every position is walkable, on the ground and in
//! no spawn room. The trace marks the run `geometry: "emptyWorld"` so
//! the viewer can warn that geometry-dependent results are not real.
//!
//! None of it is covered by a client capture — all **empty world,
//! modeled not measured**. What the model cannot answer stays
//! `not_modeled`: `Eye Position` (a hero eye height the catalog does
//! not record), `Apply Impulse` (a velocity change needs physics), and
//! the motion queries `Is Moving`/`Is Standing` (a speed the model
//! does not carry).

use workshop_rs::Value;

use crate::eval::{Env, arguments, eval, player_id};
use crate::vector::{self, Vec3};
use crate::{NotModeled, Stop, Val};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotModeled(NotModeled::new(what)))
}

/// The raycast family's shared argument list — two positions, two
/// player filters and a flag — evaluated left to right. The cast
/// itself is what the empty world answers, so the filters resolve but
/// decide nothing.
fn raycast(name: &str, args: &[Value], env: &Env) -> Result<(Vec3, Vec3), Stop> {
    let [start, end, include, exclude, owned] = arguments::<5>(name, args)?;
    let (start, end) = (
        vector::position_value(eval(start, env)?, env.players, name)?,
        vector::position_value(eval(end, env)?, env.players, name)?,
    );
    eval(include, env)?;
    eval(exclude, env)?;
    eval(owned, env)?;
    Ok((start, end))
}

/// The empty-world geometry reads: the `Value::Call` arms
/// `crate::vector::call` does not handle.
pub(crate) fn call(name: &str, args: &[Value], env: &Env) -> Result<Val, Stop> {
    match name {
        // The catalog's no-hit returns: `Ray Cast Hit Position` yields
        // the end position, `Ray Cast Hit Player` null, `Ray Cast Hit
        // Normal` the direction from the end back to the start — a
        // zero-length cast has no direction.
        "raycastHitPosition" => {
            let (_, end) = raycast(name, args, env)?;
            vector::produced(end, name)
        }
        "raycastHitPlayer" => {
            raycast(name, args, env)?;
            Ok(Val::Null)
        }
        "raycastHitNormal" => {
            let (start, end) = raycast(name, args, env)?;
            let back = start.zip(end, |start, end| start - end);
            let magnitude = back.magnitude();
            if magnitude == 0.0 {
                return unsupported(format!("{name} between identical positions"));
            }
            vector::produced(back.map(|c| c / magnitude), name)
        }
        // Line of sight always holds; the positions still resolve, and
        // the `BarrierLos` argument is read for its side effects.
        "isInLoS" => {
            let [start, end, barriers] = arguments::<3>(name, args)?;
            vector::position_value(eval(start, env)?, env.players, name)?;
            vector::position_value(eval(end, env)?, env.players, name)?;
            eval(barriers, env)?;
            Ok(Val::Bool(true))
        }
        // Every position is walkable.
        "nearestWalkablePosition" => {
            let [position] = arguments::<1>(name, args)?;
            vector::produced(
                vector::position_value(eval(position, env)?, env.players, name)?,
                name,
            )
        }
        // Ground queries: on the ground, never airborne, on no wall and
        // in no spawn room — an empty world has none. The answer does
        // not depend on the player, but one the scenario did not
        // declare is reported, never defaulted.
        "isOnGround" | "isInAir" | "isOnWall" | "isInSpawnRoom" => {
            let [player] = arguments::<1>(name, args)?;
            let id = player_id(player, env)?;
            if !env.players.contains_key(&id) {
                return unsupported(format!("`{id}` is not a scenario player"));
            }
            Ok(Val::Bool(name == "isOnGround"))
        }
        other => unsupported(format!("value {other}")),
    }
}
