//! The finite-number boundary `Val::Number` and every numeric store
//! build on (athena#133).

use std::fmt;

use serde::Serialize;

use crate::{NotModeled, Stop, Val};

/// A finite `f64`: the only payload a `Val::Number` accepts, so a
/// serialized number is never `null` (athena#133). A computed scalar
/// reaches it through [`Finite::produced`] — the one checked
/// construction — and a finite input that produces a non-finite
/// result stops the run `not_modeled`, naming the construct. Integer
/// counts and ids convert through `From`; [`Finite::new`] backs the
/// scenario-JSON boundary and test construction.
#[derive(Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Finite(f64);

impl Finite {
    /// Wrap `value`, asserting it is finite. Test and boundary
    /// construction only — a computed value belongs to
    /// [`Finite::produced`].
    pub fn new(value: f64) -> Self {
        assert!(value.is_finite(), "a non-finite number is not a value");
        Self(value)
    }

    /// The checked construction a produced scalar takes: a finite
    /// result wraps; a non-finite one is the run's `not_modeled`,
    /// naming `construct`.
    pub(crate) fn produced(value: f64, construct: &str) -> Result<Self, NotModeled> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(NotModeled::new(format!(
                "{construct} produced a non-finite number"
            )))
        }
    }

    /// The `f64` inside.
    pub fn get(self) -> f64 {
        self.0
    }
}

impl fmt::Debug for Finite {
    /// The wrapped number, so a `Val` debugs like the value it holds.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

// Counts and ids fit in an `f64` exactly and are always finite.
impl From<usize> for Finite {
    fn from(value: usize) -> Self {
        Self(value as f64)
    }
}

impl From<u64> for Finite {
    fn from(value: u64) -> Self {
        Self(value as f64)
    }
}

impl From<u8> for Finite {
    fn from(value: u8) -> Self {
        Self(f64::from(value))
    }
}

impl From<i32> for Finite {
    fn from(value: i32) -> Self {
        Self(f64::from(value))
    }
}

/// The single checked construction a produced scalar's `Val` takes:
/// finite input that produces a non-finite `f64` stops the run
/// `not_modeled`, naming `construct` — such a value would serialize as
/// `{"type":"number","value":null}` while in-world arithmetic still
/// saw the real infinity (athena#133).
pub(crate) fn produced_number(value: f64, construct: &str) -> Result<Val, Stop> {
    Ok(Val::Number(Finite::produced(value, construct)?))
}
