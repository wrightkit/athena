//! Jump targets for the block actions (`While`, `For`, `If`) of one rule.

use workshop_rs::Action;

use crate::{NotModeled, Stop};

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotModeled(NotModeled::new(what)))
}

/// Jump targets for the block actions of one rule.
pub(crate) struct Blocks {
    /// `While`/`For` → its `End`; `End` → its opener (a loop head or `If`).
    pub pairs: Vec<Option<usize>>,
    /// `If`/`ElseIf` → the next clause head or `End`; `Else` → the `End`.
    pub next_clause: Vec<Option<usize>>,
    /// `ElseIf`/`Else` → the `End` of the chain, where fall-through exits.
    pub chain_end: Vec<Option<usize>>,
}

/// A block that stays open until its `End` line.
enum Open {
    While(usize),
    For(usize),
    If {
        clauses: Vec<usize>,
        else_seen: bool,
    },
}

pub(crate) fn block_maps(actions: &[Action]) -> Result<Blocks, Stop> {
    let mut pairs = vec![None; actions.len()];
    let mut next_clause = vec![None; actions.len()];
    let mut chain_end = vec![None; actions.len()];
    let mut stack: Vec<Open> = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        match action {
            Action::While { .. } => stack.push(Open::While(index)),
            Action::ForGlobalVariable { .. } | Action::ForPlayerVariable { .. } => {
                stack.push(Open::For(index));
            }
            Action::If { .. } => stack.push(Open::If {
                clauses: vec![index],
                else_seen: false,
            }),
            Action::ElseIf { .. } | Action::Else => match stack.last_mut() {
                Some(Open::If { clauses, else_seen }) if !*else_seen => {
                    next_clause[*clauses.last().expect("an If opens its chain")] = Some(index);
                    clauses.push(index);
                    if matches!(action, Action::Else) {
                        *else_seen = true;
                    }
                }
                Some(Open::If { .. }) => return unsupported("a clause action after the `Else`"),
                _ => return unsupported("a clause action without an `If`"),
            },
            Action::End => match stack.pop() {
                Some(Open::While(open)) | Some(Open::For(open)) => {
                    pairs[open] = Some(index);
                    pairs[index] = Some(open);
                }
                Some(Open::If { clauses, .. }) => {
                    let open = *clauses.first().expect("an If opens its chain");
                    next_clause[*clauses.last().expect("an If opens its chain")] = Some(index);
                    for clause in clauses {
                        chain_end[clause] = Some(index);
                    }
                    pairs[index] = Some(open);
                }
                None => return unsupported("End without an opening action"),
            },
            _ => {}
        }
    }
    match stack.last() {
        Some(Open::While(_)) => unsupported("`While` without an `End`"),
        Some(Open::For(_)) => unsupported("`For` without an `End`"),
        Some(Open::If { .. }) => unsupported("`If` without an `End`"),
        None => Ok(Blocks {
            pairs,
            next_clause,
            chain_end,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_end_without_an_opener_is_not_modeled() {
        let Err(Stop::NotModeled(stop)) = block_maps(&[Action::End]) else {
            panic!("an unpaired `End` must stop not_modeled");
        };
        assert_eq!(stop.construct, "End without an opening action");
    }

    #[test]
    fn a_for_pairs_its_end_like_a_while() {
        let blocks = block_maps(&[
            Action::ForGlobalVariable {
                variable: "i".into(),
                start: workshop_rs::Value::Number(0.0),
                stop: workshop_rs::Value::Number(3.0),
                step: workshop_rs::Value::Number(1.0),
            },
            Action::End,
        ])
        .expect("a paired `For`/`End` maps");
        assert_eq!(blocks.pairs, vec![Some(1), Some(0)]);
    }
}
