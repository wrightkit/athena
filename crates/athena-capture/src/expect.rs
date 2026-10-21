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
    /// The expected message of each line, in order.
    pub lines: Vec<String>,
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
            n => Err(Error::SeveralRuns(n)),
        }
    }

    pub fn judge(&self, run: &[Line]) -> Verdict {
        let same = run.len() == self.lines.len()
            && run
                .iter()
                .zip(&self.lines)
                .all(|(line, expected)| same_message(line.message, expected));
        if same {
            Verdict::Match
        } else {
            Verdict::Mismatch
        }
    }
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
