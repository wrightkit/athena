use serde::Deserialize;

use crate::Error;
use crate::log::{Line, lines};

/// Tolerance for numbers in log lines: `Custom String` shows two decimals.
const NUMBER_TOLERANCE: f64 = 0.0051;

/// What a probe is expected to print, from `probes/<id>/expect.json`.
#[derive(Debug, Deserialize)]
pub struct Expect {
    /// Every line of the probe starts with this.
    pub prefix: String,
    /// The last line of a finished run starts with this.
    pub end: String,
    /// The expected message of each line, in order. Empty when `verdict`
    /// judges the run instead.
    #[serde(default)]
    pub lines: Vec<String>,
    /// A verdict computed from the run rather than compared line by line.
    #[serde(default)]
    pub verdict: Option<VerdictSpec>,
}

/// A statistical verdict over the run's lines, for a probe whose expected
/// values cannot be spelled out line by line.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum VerdictSpec {
    /// P5: a start line, then a fixed count of sample lines; the verdict is
    /// how many distinct `pos=` values the samples hold.
    #[serde(rename = "distinct-samples", rename_all = "camelCase")]
    DistinctSamples {
        /// The first line of the run starts with this.
        start_prefix: String,
        /// Every sample line starts with this.
        sample_prefix: String,
        /// The exact number of sample lines a valid run holds.
        expected_samples: usize,
        /// This many distinct `pos=` values or fewer is a `Match`.
        match_if_distinct_at_most: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Match,
    Mismatch,
}

impl Expect {
    /// The probe's lines in `text`, which must hold exactly one finished run.
    pub fn run<'a>(&self, text: &'a str) -> Result<Vec<Line<'a>>, Error> {
        let run: Vec<Line> = lines(text)
            .into_iter()
            .filter(|line| line.message.starts_with(&self.prefix))
            .collect();
        let ends = run
            .iter()
            .filter(|line| line.message.starts_with(&self.end))
            .count();
        match ends {
            0 => Err(Error::Incomplete(self.end.clone())),
            1 if run.last().is_some_and(|l| l.message.starts_with(&self.end)) => Ok(run),
            // A single end line that is not last means extra probe lines
            // followed the finished run — not several runs.
            1 => {
                let end_at = run
                    .iter()
                    .position(|line| line.message.starts_with(&self.end))
                    .expect("one end line");
                Err(Error::EndNotLast {
                    extra: run.len() - end_at - 1,
                })
            }
            n => Err(Error::SeveralRuns(n)),
        }
    }

    /// The verdict of a finished run. A `verdict` spec replaces the
    /// line-by-line comparison; a run that fails its validity rules is
    /// rejected rather than judged.
    pub fn judge(&self, run: &[Line]) -> Result<Verdict, Error> {
        if let Some(verdict) = &self.verdict {
            return verdict.judge(run);
        }
        let same = run.len() == self.lines.len()
            && run
                .iter()
                .zip(&self.lines)
                .all(|(line, expected)| same_message(line.message, expected));
        Ok(if same {
            Verdict::Match
        } else {
            Verdict::Mismatch
        })
    }
}

impl VerdictSpec {
    fn judge(&self, run: &[Line]) -> Result<Verdict, Error> {
        match self {
            VerdictSpec::DistinctSamples {
                start_prefix,
                sample_prefix,
                expected_samples,
                match_if_distinct_at_most,
            } => distinct_samples(
                run,
                start_prefix,
                sample_prefix,
                *expected_samples,
                *match_if_distinct_at_most,
            ),
        }
    }
}

/// The `distinct-samples` verdict: a valid run is the start line plus
/// exactly `expected` sample lines, the start must report a spawned
/// player at a nonzero position, and the sampled positions must change.
fn distinct_samples(
    run: &[Line],
    start_prefix: &str,
    sample_prefix: &str,
    expected: usize,
    at_most: usize,
) -> Result<Verdict, Error> {
    let invalid = |message: String| Error::Invalid(message);
    if run.len() != expected + 1 {
        return Err(invalid(format!(
            "the run holds {} lines, not a start line plus {expected} samples",
            run.len()
        )));
    }
    let start = run[0].message;
    if !start.starts_with(start_prefix) {
        return Err(invalid(format!(
            "the first line {start:?} is not the {start_prefix:?} line"
        )));
    }
    if !start.contains("spawned=真") && !start.contains("spawned=True") {
        return Err(invalid(format!(
            "the start line {start:?} does not report a spawned player"
        )));
    }
    if position(start)?.iter().all(|value| *value == 0.0) {
        return Err(invalid(
            "the start line reports a zero position; the player had not spawned".to_string(),
        ));
    }

    // Positions are distinct when any coordinate differs past the printed
    // tolerance; a stuck counter would mean the player never moved.
    let mut distinct: Vec<Vec<f64>> = Vec::new();
    for line in &run[1..] {
        if !line.message.starts_with(sample_prefix) {
            return Err(invalid(format!(
                "the line {:?} is not a {sample_prefix:?} sample",
                line.message
            )));
        }
        let pos = position(line.message)?;
        if !distinct.iter().any(|seen| {
            seen.iter()
                .zip(&pos)
                .all(|(a, b)| (a - b).abs() <= NUMBER_TOLERANCE)
        }) {
            distinct.push(pos);
        }
    }
    if distinct.len() <= 1 {
        return Err(invalid(
            "the sampled position never changes; the player had not spawned".to_string(),
        ));
    }
    Ok(if distinct.len() <= at_most {
        Verdict::Match
    } else {
        Verdict::Mismatch
    })
}

/// The `pos=(x, y, z)` tuple of a log line, as numbers.
fn position(line: &str) -> Result<Vec<f64>, Error> {
    let invalid = || Error::Invalid(format!("the line {line:?} has no pos=(x, y, z)"));
    let at = line.find("pos=(").ok_or_else(invalid)? + "pos=(".len();
    let end = line[at..].find(')').ok_or_else(invalid)?;
    line[at..at + end]
        .split(',')
        .map(|part| part.trim().parse::<f64>().map_err(|_| invalid()))
        .collect()
}

#[derive(PartialEq)]
enum Token<'a> {
    Text(&'a str),
    Number(f64),
}

fn tokens(text: &str) -> Vec<Token<'_>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let (mut text_start, mut i) = (0, 0);
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        // A number is read starting at its first digit, so the dot belongs
        // to it only when a digit follows (`12.5`), not before (`.5` and
        // `.50` tokenize differently though numerically equal — the client
        // prints two decimals, so this edge is convention-unreachable).
        if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if text_start < start {
            out.push(Token::Text(&text[text_start..start]));
        }
        out.push(Token::Number(text[start..i].parse().unwrap()));
        text_start = i;
    }
    if text_start < bytes.len() {
        out.push(Token::Text(&text[text_start..]));
    }
    out
}

fn same_message(captured: &str, expected: &str) -> bool {
    let (a, b) = (tokens(captured), tokens(expected));
    a.len() == b.len()
        && a.iter().zip(&b).all(|pair| match pair {
            (Token::Text(x), Token::Text(y)) => x == y,
            (Token::Number(x), Token::Number(y)) => (x - y).abs() <= NUMBER_TOLERANCE,
            _ => false,
        })
}
