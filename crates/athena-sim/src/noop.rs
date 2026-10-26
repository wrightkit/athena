//! Actions that change nothing a program can read: the inspector
//! recording, announcer, music, HUD and in-world-UI visibility toggles,
//! the built-in game-mode flags that gate completion, scoring and
//! respawning behavior the simulator does not run, the movement-
//! collision toggles, the camera, forced names, outlines and voice-line
//! pitch, player and barrier scaling, and hero preloading (athena#103).
//! Each still evaluates its arguments left to right, so a read inside an
//! argument stops the run like anywhere else. Modeled, not measured —
//! the basis is the catalog sentence per action (`docs/findings.md`).

use workshop_rs::Value;

use crate::Stop;
use crate::eval::{Env, eval};

/// Execute a modeled no-read action against nothing; `None` when `name`
/// is not one of them. `Ok(())` on success — the action produces no
/// state and records no change.
pub(crate) fn action(name: &str, args: &[Value], env: &Env) -> Option<Result<(), Stop>> {
    if !matches!(
        name,
        "disableInspector"
            | "disableBuiltinGameModeAnnouncer"
            | "disableBuiltinGameModeMusic"
            | "enableBuiltinGameModeMusic"
            | "disableBuiltinGameModeCompletion"
            | "disableBuiltinGameModeScoring"
            | "disableGameModeHud"
            | "disableGameModeInworldUI"
            | "disableHeroHud"
            | "disableMovementCollisionWithEnvironment"
            | "enableMovementCollisionWithEnvironment"
            | "disableMovementCollisionWithPlayers"
            | "disableRespawn"
            | "enableRespawn"
            | "startCamera"
            | "stopCamera"
            | "startForcingName"
            | "startForcingOutlineFor"
            | "stopForcingOutlineFor"
            | "startModifyingVoicelinePitch"
            | "stopModifyingVoicelinePitch"
            | "startScalingBarriers"
            | "stopScalingBarriers"
            | "startScalingSize"
            | "stopScalingSize"
            | "preloadHero"
    ) {
        return None;
    }
    Some(noop(args, env))
}

fn noop(args: &[Value], env: &Env) -> Result<(), Stop> {
    for arg in args {
        eval(arg, env)?;
    }
    Ok(())
}
