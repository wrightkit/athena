//! The `expect` entries of a scenario: the observations checked at the
//! end of a complete run, parsed to [`Expectation`]s.

use crate::scenario::{format_error, value_to_val};
use crate::{ScenarioError, Val};

/// One expected observation checked at the end of a complete run.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expectation {
    Global {
        name: String,
        value: Val,
    },
    PlayerVariable {
        player: String,
        name: String,
        value: Val,
    },
    Log {
        tick: u64,
        text: String,
    },
}

fn named_id(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ScenarioError> {
    match object.get(key).and_then(|value| value.as_str()) {
        Some(text) if !text.is_empty() => Ok(text.to_string()),
        Some(_) => format_error(format!("`{key}` must not be empty")),
        None => format_error(format!("`{key}` is missing or not a text")),
    }
}

pub(crate) fn expectation(
    index: usize,
    value: &serde_json::Value,
) -> Result<Expectation, ScenarioError> {
    let Some(object) = value.as_object() else {
        return format_error(format!("expect[{index}] is not an object"));
    };
    let kind = object.get("kind").and_then(|kind| kind.as_str());
    let allowed: &[&str] = match kind {
        Some("global") => &["kind", "name", "value"],
        Some("playerVariable") => &["kind", "player", "name", "value"],
        Some("log") => &["kind", "tick", "text"],
        other => {
            return format_error(format!(
                "expect[{index}].kind is {other:?}, expected `global`, `playerVariable` or `log`"
            ));
        }
    };
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return format_error(format!("expect[{index}] has an unknown field `{key}`"));
        }
    }
    match kind {
        Some("global") => Ok(Expectation::Global {
            name: named_id(object, "name")?,
            value: value_to_val(
                object
                    .get("value")
                    .ok_or_else(|| ScenarioError::Format("expect value is missing".into()))?,
            )?,
        }),
        Some("playerVariable") => Ok(Expectation::PlayerVariable {
            player: named_id(object, "player")?,
            name: named_id(object, "name")?,
            value: value_to_val(
                object
                    .get("value")
                    .ok_or_else(|| ScenarioError::Format("expect value is missing".into()))?,
            )?,
        }),
        _ => {
            let Some(serde_json::Value::Number(tick)) = object.get("tick") else {
                return format_error(format!("expect[{index}].tick is missing or not an integer"));
            };
            let Some(tick) = tick.as_u64() else {
                return format_error(format!("expect[{index}].tick is missing or not an integer"));
            };
            match object.get("text").and_then(|text| text.as_str()) {
                Some(text) => Ok(Expectation::Log {
                    tick,
                    text: text.to_string(),
                }),
                None => format_error(format!("expect[{index}].text is missing or not a text")),
            }
        }
    }
}
