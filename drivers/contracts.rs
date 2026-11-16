//! T-owned operator contracts. Value oracles and equivalence relations have separate owners.
use crate::{
    content::{self, Action, Case},
    reference::{self, State},
};
use serde_json::{Value, json};

/// The I adapter supplies execution and safe observations of known-live symbols.
pub trait Subject: Sized {
    fn from_state(width: usize, state: &State) -> Self;
    fn apply(&mut self, action: &Action, width: usize, seed: u64, direct: bool);
    fn observe(&self, live: &State) -> State;
}

fn initial(c: &Case, pattern: usize) -> State {
    let mut state = reference::initial(c.width, c.seed);
    if pattern < 2 {
        for bytes in state.values_mut() {
            bytes.fill(if pattern == 0 { 0 } else { 0xff });
        }
    }
    state
}

fn oracle_step<S: Subject>(
    subject: &mut S,
    state: &mut State,
    action: &Action,
    c: &Case,
    seed: u64,
    context: &str,
) {
    reference::apply(state, action, c.width, seed);
    subject.apply(action, c.width, seed, true);
    assert_eq!(subject.observe(state), *state, "{context}: {action:?}");
}

fn arithmetic<S: Subject>(c: &Case) -> Value {
    let actions: Vec<_> = content::operations()
        .into_iter()
        .filter(|(_, _, a)| {
            matches!(
                a,
                Action::Xor { .. }
                    | Action::Alpha { .. }
                    | Action::Scale { .. }
                    | Action::MulAdd { .. }
            )
        })
        .collect();
    for (name, _, action) in &actions {
        for pattern in 0..3 {
            let mut state = initial(c, pattern);
            let mut subject = S::from_state(c.width, &state);
            oracle_step(
                &mut subject,
                &mut state,
                action,
                c,
                c.seed,
                &format!(
                    "V02 independent arithmetic oracle: {name}, pattern={pattern}, width={}",
                    c.width
                ),
            );
        }
    }
    let mut exhaustive = 0;
    if c.width == 1 {
        for scalar in 0..=255 {
            for byte in 0..=255 {
                let mut state = State::from([(0, vec![byte])]);
                let mut subject = S::from_state(1, &state);
                oracle_step(
                    &mut subject,
                    &mut state,
                    &Action::Scale { id: 0, scalar },
                    c,
                    c.seed,
                    &format!("V02 exhaustive scalar multiplication: byte={byte}, scalar={scalar}"),
                );
                exhaustive += 1;
            }
        }
    }
    json!({"contract":"independent arithmetic values", "dispatch":"direct", "field_polynomial":0x11d,
        "operations":actions.iter().map(|(name,_,_)|name).collect::<Vec<_>>(),
        "patterns":["zero","ff","splitmix64"], "oracle_checks":actions.len()*3,
        "exhaustive_scalar_checks":exhaustive})
}

/// Copy probes include in-place writes and replacement; move/remove probes reuse IDs.
fn followups(action: &Action) -> Vec<Action> {
    use Action::*;
    match action {
        Copy { src, dst } => vec![
            Scale {
                id: *dst,
                scalar: 0,
            },
            Insert { id: *dst },
            Scale {
                id: *src,
                scalar: 0,
            },
            Insert { id: *src },
        ],
        CopyRange { src, dst, count } => (0..*count)
            .flat_map(|i| {
                [
                    Scale {
                        id: dst + i,
                        scalar: 0,
                    },
                    Insert { id: dst + i },
                ]
            })
            .chain((0..*count).flat_map(|i| {
                [
                    Scale {
                        id: src + i,
                        scalar: 0,
                    },
                    Insert { id: src + i },
                ]
            }))
            .collect(),
        Move { src, .. } => vec![Insert { id: *src }],
        MoveRange { src, count, .. } => (0..*count).map(|i| Insert { id: src + i }).collect(),
        Remove { id } => vec![Insert { id: *id }],
        _ => vec![],
    }
}

fn storage<S: Subject>(c: &Case) -> Value {
    let actions: Vec<_> = content::operations()
        .into_iter()
        .filter(|(_, _, a)| {
            matches!(
                a,
                Action::Insert { .. }
                    | Action::Read { .. }
                    | Action::Zero { .. }
                    | Action::Copy { .. }
                    | Action::Move { .. }
                    | Action::Remove { .. }
                    | Action::Info { .. }
            )
        })
        .collect();
    let mut probes = 0;
    for (name, _, action) in &actions {
        for pattern in 0..3 {
            let mut state = initial(c, pattern);
            let mut subject = S::from_state(c.width, &state);
            let context = format!(
                "V03 storage contract: {name}, pattern={pattern}, width={}",
                c.width
            );
            oracle_step(&mut subject, &mut state, action, c, c.seed, &context);
            for (index, next) in followups(action).iter().enumerate() {
                let seed = c.seed.wrapping_add(index as u64 + 1);
                let context = format!("{context}, independence/reuse step={index}");
                if let Action::Scale { id, .. } = next {
                    // A zero-write after copy checks the copied storage state,
                    // including deferred writes, without another GF value matrix.
                    state.get_mut(id).unwrap().fill(0);
                    subject.apply(next, c.width, seed, true);
                    assert_eq!(subject.observe(&state), state, "{context}: {next:?}");
                } else {
                    oracle_step(&mut subject, &mut state, next, c, seed, &context);
                }
                probes += 1;
            }
        }
    }
    json!({"contract":"storage values, copy independence and ID reuse", "dispatch":"direct",
        "operations":actions.iter().map(|(name,_,_)|name).collect::<Vec<_>>(),
        "patterns":["zero","ff","splitmix64"], "value_checks":actions.len()*3, "independence_and_reuse_checks":probes,
        "mutation_scope":"after-copy zero writes check stored zeros and isolation; V02 owns the general GF value matrix",
        "unobservable":["absence of removed/moved IDs: no safe membership API; only surviving bytes and subsequent reuse are checked",
            "informational association mapping: no query API; only preservation of symbol bytes is checked"],
        "sharing":"supported alias/first-write correctness remains in O08 premeasurement checks"})
}

/// Only ID liveness is needed for equivalence; no scalar values are computed here.
fn observe_after(live: &mut State, action: &Action) {
    use Action::*;
    let mut add = |id| {
        live.insert(id, Vec::new());
    };
    match action {
        Insert { id } | Zero { id } => add(*id),
        ZeroList { ids } => {
            for id in ids {
                add(*id);
            }
        }
        ZeroRange { start, count } => {
            for id in *start..start + count {
                add(id);
            }
        }
        Copy { dst, .. } | Alias { dst, .. } => add(*dst),
        CopyRange { dst, count, .. } => {
            for id in *dst..dst + count {
                add(id);
            }
        }
        Move { src, dst } => {
            live.remove(src);
            live.insert(*dst, Vec::new());
        }
        MoveRange { src, dst, count } => {
            for id in *src..src + count {
                live.remove(&id);
            }
            for id in *dst..dst + count {
                live.insert(id, Vec::new());
            }
        }
        Remove { id } => {
            live.remove(id);
        }
        _ => {}
    }
}

fn elementary(action: &Action) -> Option<Vec<Action>> {
    use Action::*;
    Some(match action {
        ZeroList { ids } => ids.iter().map(|&id| Zero { id }).collect(),
        ZeroRange { start, count } => (*start..start + count).map(|id| Zero { id }).collect(),
        XorTwo { a, b, dst } => [*a, *b]
            .into_iter()
            .map(|src| Xor { src, dst: *dst })
            .collect(),
        XorThree { a, b, c, dst } => [*a, *b, *c]
            .into_iter()
            .map(|src| Xor { src, dst: *dst })
            .collect(),
        XorMany { sources, dst } => sources.iter().map(|&src| Xor { src, dst: *dst }).collect(),
        Broadcast { src, destinations } => destinations
            .iter()
            .map(|&dst| Xor { src: *src, dst })
            .collect(),
        CopyRange { src, dst, count } => (0..*count)
            .map(|i| Copy {
                src: src + i,
                dst: dst + i,
            })
            .collect(),
        MoveRange { src, dst, count } => (0..*count)
            .map(|i| Move {
                src: src + i,
                dst: dst + i,
            })
            .collect(),
        _ => return None,
    })
}

fn equivalent<S: Subject>(c: &Case, action: &Action, pattern: usize, singles: Option<&[Action]>) {
    let state = initial(c, pattern);
    let mut left = S::from_state(c.width, &state);
    let mut right = S::from_state(c.width, &state);
    left.apply(action, c.width, c.seed, true);
    if let Some(singles) = singles {
        for single in singles {
            right.apply(single, c.width, c.seed, true);
        }
    } else {
        right.apply(action, c.width, c.seed, false);
    }
    let relation = if singles.is_some() {
        "batch versus elementary direct calls"
    } else {
        "direct versus execute"
    };
    let context = format!(
        "V05 {relation}: {action:?}, pattern={pattern}, width={}",
        c.width
    );
    let mut live: State = state.keys().map(|&id| (id, Vec::new())).collect();
    observe_after(&mut live, action);
    assert_eq!(left.observe(&live), right.observe(&live), "{context}");
    for (index, next) in followups(action).iter().enumerate() {
        let seed = c.seed.wrapping_add(index as u64 + 1);
        left.apply(next, c.width, seed, true);
        right.apply(next, c.width, seed, true);
        observe_after(&mut live, next);
        assert_eq!(
            left.observe(&live),
            right.observe(&live),
            "{context}, independence/reuse step={index}"
        );
    }
}

fn dispatch<S: Subject>(c: &Case) -> Value {
    let mut pairs = 0;
    let mut compositions = 0;
    for (_, _, action) in content::operations() {
        if matches!(
            action,
            Action::Insert { .. }
                | Action::Read { .. }
                | Action::Alias { .. }
                | Action::SharedWrite { .. }
        ) {
            continue;
        }
        for pattern in 0..3 {
            equivalent::<S>(c, &action, pattern, None);
            pairs += 1;
            if let Some(singles) = elementary(&action) {
                equivalent::<S>(c, &action, pattern, Some(&singles));
                compositions += 1;
            }
        }
    }
    json!({"contract":"dispatch and batch composition equivalence", "oracle":"relational; no scalar oracle",
        "patterns":["zero","ff","splitmix64"], "dispatch_pairs":pairs, "batch_composition_pairs":compositions,
        "preconditions":"distinct XOR operands and disjoint copy/move ranges; V06 owns ambiguous boundaries",
        "storage_followups":"copy independence and ID reuse compared on both paths",
        "unobservable":["absence of removed/moved IDs", "informational association mapping"],
        "interpretation":"equivalence alone does not prove correct values; V02 and V03 establish the primitive contracts"})
}

pub fn validate<S: Subject>(c: &Case) -> Value {
    match c.workload.as_str() {
        "operator-reference" => arithmetic::<S>(c),
        "storage-ownership" => storage::<S>(c),
        "dispatch-equivalence" => dispatch::<S>(c),
        _ => panic!("unknown operator contract {}", c.workload),
    }
}
