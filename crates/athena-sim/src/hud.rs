//! HUD actions: creating, destroying and enumerating HUD elements.

use std::cell::RefCell;

use workshop_rs::Value;

use crate::eval::{Env, eval, viewers};
use crate::{Change, HudArgs, HudElement, HudKind, NotModeled, Stop, Val};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(NotModeled::new(what).into())
}

/// The HUD elements a run created and the text id of the newest one.
#[derive(Default)]
pub(crate) struct Hud {
    /// In creation order; an element's `id` is its index.
    pub elements: Vec<HudElement>,
    /// The `id` of the newest `HudText`/`ProgressBarHud` element; what
    /// `Last Text ID` reads.
    pub last_text_id: Option<u64>,
}

/// An action argument that must be a catalog enum member literal; anything
/// else is un-evaluatable in this layer.
pub(crate) fn enum_member<'a>(
    action: &str,
    param: &str,
    value: &'a Value,
) -> Result<&'a str, Stop> {
    match value {
        Value::Enum { value, .. } => Ok(value.as_str()),
        _ => unsupported(format!("{action} {param}")),
    }
}

/// Record a HUD element. `id` is the creation-order index; the kinds the
/// destroy actions can target are also the ones `Last Text ID` reads.
fn create_element(
    hud: &mut Hud,
    kind: HudKind,
    tick: u64,
    name: &str,
    recipients: Vec<String>,
    args: HudArgs,
    changes: &RefCell<Vec<Change>>,
) {
    let id = hud.elements.len() as u64;
    hud.elements.push(HudElement {
        id,
        kind,
        created_tick: tick,
        destroyed_tick: None,
        recipients,
        construct: format!("action {name}"),
        args,
    });
    if matches!(kind, HudKind::HudText | HudKind::ProgressBarHud) {
        hud.last_text_id = Some(id);
    }
    changes
        .borrow_mut()
        .push(Change::HudCreate { id, element: kind });
}

/// Execute a modeled HUD action against `hud`; `None` when `name` is not
/// one of them. Element creations and destructions are appended to
/// `changes`.
pub(crate) fn action(
    name: &str,
    args: &[Value],
    env: &Env,
    hud: &mut Hud,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Option<Result<(), Stop>> {
    Some(match name {
        "createHudText" => create_hud_text(args, env, hud, tick, changes),
        "progressBarHud" => progress_bar_hud(args, env, hud, tick, changes),
        "bigMessage" | "smallMessage" => message(name, args, env, hud, tick, changes),
        "setObjectiveDescription" => objective_description(args, env, hud, tick, changes),
        "destroyHudText" | "destroyProgressBarHud" => destroy(name, args, env, hud, tick, changes),
        "destroyAllHudText" | "destroyAllProgressBarHudText" => {
            destroy_all(name, args, hud, tick, changes)
        }
        "enableMessages" | "disableMessages" => message_toggle(name, args, env),
        _ => return None,
    })
}

fn create_hud_text(
    args: &[Value],
    env: &Env,
    hud: &mut Hud,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [
        visible_to,
        header,
        subheader,
        text,
        location,
        sort_order,
        header_color,
        subheader_color,
        text_color,
        reevaluation,
        spectators,
    ] = args
    else {
        return unsupported("createHudText arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let args = HudArgs {
        header: Some(eval(header, env)?),
        subheader: Some(eval(subheader, env)?),
        text: Some(eval(text, env)?),
        location: Some(enum_member("createHudText", "location", location)?.to_string()),
        sort_order: Some(eval(sort_order, env)?),
        header_color: Some(enum_member("createHudText", "headerColor", header_color)?.to_string()),
        subheader_color: Some(
            enum_member("createHudText", "subheaderColor", subheader_color)?.to_string(),
        ),
        text_color: Some(enum_member("createHudText", "textColor", text_color)?.to_string()),
        reevaluation: Some(enum_member("createHudText", "reevaluation", reevaluation)?.to_string()),
        spec_visibility: Some(enum_member("createHudText", "spectators", spectators)?.to_string()),
        ..HudArgs::default()
    };
    create_element(
        hud,
        HudKind::HudText,
        tick,
        "createHudText",
        recipients,
        args,
        changes,
    );
    Ok(())
}

fn progress_bar_hud(
    args: &[Value],
    env: &Env,
    hud: &mut Hud,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [
        visible_to,
        value,
        text,
        location,
        sort_order,
        bar_color,
        text_color,
        reevaluation,
        spectators,
    ] = args
    else {
        return unsupported("progressBarHud arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let args = HudArgs {
        value: Some(eval(value, env)?),
        text: Some(eval(text, env)?),
        location: Some(enum_member("progressBarHud", "location", location)?.to_string()),
        sort_order: Some(eval(sort_order, env)?),
        progress_bar_color: Some(
            enum_member("progressBarHud", "progressBarColor", bar_color)?.to_string(),
        ),
        text_color: Some(enum_member("progressBarHud", "textColor", text_color)?.to_string()),
        reevaluation: Some(
            enum_member("progressBarHud", "reevaluation", reevaluation)?.to_string(),
        ),
        spec_visibility: Some(enum_member("progressBarHud", "spectators", spectators)?.to_string()),
        ..HudArgs::default()
    };
    create_element(
        hud,
        HudKind::ProgressBarHud,
        tick,
        "progressBarHud",
        recipients,
        args,
        changes,
    );
    Ok(())
}

fn message(
    name: &str,
    args: &[Value],
    env: &Env,
    hud: &mut Hud,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [visible_to, header] = args else {
        return unsupported(format!("{name} arguments"));
    };
    let recipients = viewers(visible_to, env)?;
    let args = HudArgs {
        header: Some(eval(header, env)?),
        ..HudArgs::default()
    };
    let kind = if name == "bigMessage" {
        HudKind::BigMessage
    } else {
        HudKind::SmallMessage
    };
    create_element(hud, kind, tick, name, recipients, args, changes);
    Ok(())
}

fn objective_description(
    args: &[Value],
    env: &Env,
    hud: &mut Hud,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [visible_to, text, reevaluation] = args else {
        return unsupported("setObjectiveDescription arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let args = HudArgs {
        text: Some(eval(text, env)?),
        reevaluation: Some(
            enum_member("setObjectiveDescription", "reevaluation", reevaluation)?.to_string(),
        ),
        ..HudArgs::default()
    };
    // The objective description is a singleton per player: a new one
    // replaces earlier descriptions for the same recipients; an element
    // left with no recipient is destroyed.
    for element in &mut hud.elements {
        if element.kind == HudKind::ObjectiveDescription && element.destroyed_tick.is_none() {
            element.recipients.retain(|id| !recipients.contains(id));
            if element.recipients.is_empty() {
                element.destroyed_tick = Some(tick);
                changes.borrow_mut().push(Change::HudDestroy {
                    id: element.id,
                    element: element.kind,
                });
            }
        }
    }
    create_element(
        hud,
        HudKind::ObjectiveDescription,
        tick,
        "setObjectiveDescription",
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
    hud: &mut Hud,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [text_id] = args else {
        return unsupported(format!("{name} arguments"));
    };
    let kind = if name == "destroyHudText" {
        HudKind::HudText
    } else {
        HudKind::ProgressBarHud
    };
    // An unknown, `null` or already-destroyed text id destroys nothing.
    match eval(text_id, env)? {
        Val::Null => {}
        Val::Number(id) => {
            if let Some(element) = hud.elements.iter_mut().find(|element| {
                element.kind == kind && element.destroyed_tick.is_none() && element.id as f64 == id
            }) {
                element.destroyed_tick = Some(tick);
                changes.borrow_mut().push(Change::HudDestroy {
                    id: element.id,
                    element: element.kind,
                });
            }
        }
        other => return unsupported(format!("{name} of {other:?}")),
    }
    Ok(())
}

fn destroy_all(
    name: &str,
    args: &[Value],
    hud: &mut Hud,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [] = args else {
        return unsupported(format!("{name} arguments"));
    };
    let kind = if name == "destroyAllHudText" {
        HudKind::HudText
    } else {
        HudKind::ProgressBarHud
    };
    for element in &mut hud.elements {
        if element.kind == kind && element.destroyed_tick.is_none() {
            element.destroyed_tick = Some(tick);
            changes.borrow_mut().push(Change::HudDestroy {
                id: element.id,
                element: element.kind,
            });
        }
    }
    Ok(())
}

/// Message suppression is client display state, excluded like display
/// duration and stacking; the actions only resolve their player argument.
fn message_toggle(name: &str, args: &[Value], env: &Env) -> Result<(), Stop> {
    let [players] = args else {
        return unsupported(format!("{name} arguments"));
    };
    viewers(players, env)?;
    Ok(())
}
