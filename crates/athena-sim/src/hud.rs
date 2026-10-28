//! Display-element actions: creating, destroying and enumerating the
//! screen elements. The four text actions — `createHudText`,
//! `progressBarHud`, `createInWorldText`, `createProgressBarInWorldText`
//! — share the catalog's `TextId` space: the element `id` is the text id
//! `Last Text ID` reads and the destroy actions target. The message and
//! objective-description actions create no text id.

use std::cell::RefCell;

use workshop_rs::Value;

use crate::eval::{Env, eval, viewers};
use crate::unsupported;
use crate::{Change, HudArgs, HudElement, HudKind, Stop, Val};

/// The display elements a run created and the text id of the newest one.
#[derive(Default)]
pub(crate) struct Hud {
    /// In creation order; an element's `id` is its index.
    pub elements: Vec<HudElement>,
    /// The `id` of the newest text element — `hudText`, `progressBarHud`,
    /// `inWorldText` or `progressBarInWorldText`; what `Last Text ID`
    /// reads.
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

/// The kinds that hold a `TextId` — the four text creates per the
/// catalog's `lastTextId` and `getNumberOfTextIds` descriptions.
pub(crate) fn text_kind(kind: HudKind) -> bool {
    matches!(
        kind,
        HudKind::HudText
            | HudKind::ProgressBarHud
            | HudKind::InWorldText
            | HudKind::ProgressBarInWorldText
    )
}

impl Hud {
    /// The live text elements `getNumberOfTextIds` counts.
    pub(crate) fn text_count(&self) -> u64 {
        self.elements
            .iter()
            .filter(|element| text_kind(element.kind) && element.destroyed_tick.is_none())
            .count() as u64
    }
}

/// Record a display element. `id` is the creation-order index; a text
/// kind also hands its `id` to `Last Text ID`.
fn create_element(
    hud: &RefCell<Hud>,
    kind: HudKind,
    tick: u64,
    name: &str,
    recipients: Vec<String>,
    args: HudArgs,
    changes: &RefCell<Vec<Change>>,
) {
    let mut hud = hud.borrow_mut();
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
    if text_kind(kind) {
        hud.last_text_id = Some(id);
    }
    changes
        .borrow_mut()
        .push(Change::HudCreate { id, element: kind });
}

/// Mark a live element of `kind` with `id` destroyed on `tick`; an id no
/// live element holds destroys nothing.
fn destroy_element(
    hud: &RefCell<Hud>,
    kind: HudKind,
    id: f64,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) {
    let mut hud = hud.borrow_mut();
    if let Some(element) = hud.elements.iter_mut().find(|element| {
        element.kind == kind && element.destroyed_tick.is_none() && element.id as f64 == id
    }) {
        element.destroyed_tick = Some(tick);
        let id = element.id;
        changes
            .borrow_mut()
            .push(Change::HudDestroy { id, element: kind });
    }
}

/// Execute a modeled display action against `hud`; `None` when `name` is
/// not one of them. Element creations and destructions are appended to
/// `changes`.
pub(crate) fn action(
    name: &str,
    args: &[Value],
    env: &Env,
    hud: &RefCell<Hud>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Option<Result<(), Stop>> {
    Some(match name {
        "createHudText" => create_hud_text(args, env, hud, tick, changes),
        "progressBarHud" => progress_bar_hud(args, env, hud, tick, changes),
        "createInWorldText" => create_in_world_text(args, env, hud, tick, changes),
        "createProgressBarInWorldText" => progress_bar_in_world_text(args, env, hud, tick, changes),
        "bigMessage" | "smallMessage" => message(name, args, env, hud, tick, changes),
        "setObjectiveDescription" => objective_description(args, env, hud, tick, changes),
        "destroyHudText"
        | "destroyProgressBarHud"
        | "destroyInWorldText"
        | "destroyProgressBarInWorldText" => destroy(name, args, env, hud, tick, changes),
        "destroyAllHudText"
        | "destroyAllProgressBarHudText"
        | "destroyAllInWorldText"
        | "destroyAllProgressBarInWorldText" => destroy_all(name, args, hud, tick, changes),
        "enableMessages" | "disableMessages" => message_toggle(name, args, env),
        _ => return None,
    })
}

fn create_hud_text(
    args: &[Value],
    env: &Env,
    hud: &RefCell<Hud>,
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
        header_color: Some(eval(header_color, env)?),
        subheader_color: Some(eval(subheader_color, env)?),
        text_color: Some(eval(text_color, env)?),
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
    hud: &RefCell<Hud>,
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
        progress_bar_color: Some(eval(bar_color, env)?),
        text_color: Some(eval(text_color, env)?),
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

fn create_in_world_text(
    args: &[Value],
    env: &Env,
    hud: &RefCell<Hud>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [
        visible_to,
        text,
        position,
        scale,
        clipping,
        reevaluation,
        color,
        spec_visibility,
    ] = args
    else {
        return unsupported("createInWorldText arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let args = HudArgs {
        text: Some(eval(text, env)?),
        position: Some(eval(position, env)?),
        scale: Some(eval(scale, env)?),
        clipping: Some(enum_member("createInWorldText", "clipping", clipping)?.to_string()),
        reevaluation: Some(
            enum_member("createInWorldText", "reevaluation", reevaluation)?.to_string(),
        ),
        color: Some(eval(color, env)?),
        spec_visibility: Some(
            enum_member("createInWorldText", "specVisibility", spec_visibility)?.to_string(),
        ),
        ..HudArgs::default()
    };
    create_element(
        hud,
        HudKind::InWorldText,
        tick,
        "createInWorldText",
        recipients,
        args,
        changes,
    );
    Ok(())
}

fn progress_bar_in_world_text(
    args: &[Value],
    env: &Env,
    hud: &RefCell<Hud>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [
        visible_to,
        value,
        text,
        position,
        scale,
        clipping,
        bar_color,
        text_color,
        reevaluation,
        spec_visibility,
    ] = args
    else {
        return unsupported("createProgressBarInWorldText arguments");
    };
    let recipients = viewers(visible_to, env)?;
    let action = "createProgressBarInWorldText";
    let args = HudArgs {
        value: Some(eval(value, env)?),
        text: Some(eval(text, env)?),
        position: Some(eval(position, env)?),
        scale: Some(eval(scale, env)?),
        clipping: Some(enum_member(action, "clipping", clipping)?.to_string()),
        progress_bar_color: Some(eval(bar_color, env)?),
        text_color: Some(eval(text_color, env)?),
        reevaluation: Some(enum_member(action, "reevaluation", reevaluation)?.to_string()),
        spec_visibility: Some(enum_member(action, "specVisibility", spec_visibility)?.to_string()),
        ..HudArgs::default()
    };
    create_element(
        hud,
        HudKind::ProgressBarInWorldText,
        tick,
        action,
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
    hud: &RefCell<Hud>,
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
    hud: &RefCell<Hud>,
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
    let mut destroyed = Vec::new();
    {
        let mut hud_mut = hud.borrow_mut();
        for element in &mut hud_mut.elements {
            if element.kind == HudKind::ObjectiveDescription && element.destroyed_tick.is_none() {
                element.recipients.retain(|id| !recipients.contains(id));
                if element.recipients.is_empty() {
                    element.destroyed_tick = Some(tick);
                    destroyed.push(element.id);
                }
            }
        }
    }
    for id in destroyed {
        changes.borrow_mut().push(Change::HudDestroy {
            id,
            element: HudKind::ObjectiveDescription,
        });
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
    hud: &RefCell<Hud>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [text_id] = args else {
        return unsupported(format!("{name} arguments"));
    };
    let kind = match name {
        "destroyHudText" => HudKind::HudText,
        "destroyProgressBarHud" => HudKind::ProgressBarHud,
        "destroyInWorldText" => HudKind::InWorldText,
        _ => HudKind::ProgressBarInWorldText,
    };
    // An unknown, `null` or already-destroyed text id destroys nothing.
    match eval(text_id, env)? {
        Val::Null => {}
        Val::Number(id) => destroy_element(hud, kind, id.get(), tick, changes),
        other => return unsupported(format!("{name} of {other:?}")),
    }
    Ok(())
}

fn destroy_all(
    name: &str,
    args: &[Value],
    hud: &RefCell<Hud>,
    tick: u64,
    changes: &RefCell<Vec<Change>>,
) -> Result<(), Stop> {
    let [] = args else {
        return unsupported(format!("{name} arguments"));
    };
    let kind = match name {
        "destroyAllHudText" => HudKind::HudText,
        "destroyAllProgressBarHudText" => HudKind::ProgressBarHud,
        "destroyAllInWorldText" => HudKind::InWorldText,
        _ => HudKind::ProgressBarInWorldText,
    };
    let mut destroyed = Vec::new();
    {
        let mut hud_mut = hud.borrow_mut();
        for element in &mut hud_mut.elements {
            if element.kind == kind && element.destroyed_tick.is_none() {
                element.destroyed_tick = Some(tick);
                destroyed.push(element.id);
            }
        }
    }
    for id in destroyed {
        changes
            .borrow_mut()
            .push(Change::HudDestroy { id, element: kind });
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
