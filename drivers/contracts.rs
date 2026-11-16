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
        "mixed-sequence" => mixed_sequence::<S>(c),
        _ => panic!("unknown operator contract {}", c.workload),
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SequenceStep {
    pub action: Action,
    pub seed: u64,
}

/// Build the complete trace before touching the component. No hidden repair calls.
pub fn sequence_steps(c: &Case) -> Vec<SequenceStep> {
    use Action::*;
    let spec = c
        .sequence
        .as_ref()
        .expect("V04 sequence specification missing");
    let mut actions = Vec::new();
    match spec.scenario.as_str() {
        "copy-chain" => {
            for _ in 0..8 {
                actions.extend([
                    Copy { src: 0, dst: 16 },
                    Copy { src: 16, dst: 17 },
                    Scale { id: 16, scalar: 7 },
                    Xor { src: 17, dst: 1 },
                    Scale { id: 0, scalar: 255 },
                    Copy { src: 17, dst: 18 },
                    MulAdd {
                        src: 16,
                        scalar: 7,
                        dst: 18,
                    },
                    Move { src: 18, dst: 32 },
                    Insert { id: 18 },
                    Copy { src: 32, dst: 16 },
                    Scale { id: 32, scalar: 0 },
                    Remove { id: 17 },
                    Insert { id: 17 },
                    Move { src: 16, dst: 0 },
                    Insert { id: 16 },
                    Zero { id: 17 },
                    Alpha { id: 0 },
                    Read { id: 32 },
                ]);
            }
        }
        "move-reuse" => {
            for round in 0..16 {
                let src = 8 + round % 8;
                let dst = 32 + round;
                actions.extend([
                    Insert { id: src },
                    Move { src, dst },
                    Insert { id: src },
                    Xor { src: dst, dst: src },
                    Remove { id: dst },
                    Insert { id: dst },
                    Copy { src, dst },
                    Scale { id: dst, scalar: 7 },
                    Remove { id: src },
                    Insert { id: src },
                    Move { src: dst, dst: src },
                    Insert { id: dst },
                    Remove { id: dst },
                ]);
            }
        }
        "sparse-churn" => {
            let ids: Vec<_> = (16..48).chain((0..32).map(|i| 128 + 61 * i)).collect();
            actions.extend(ids.iter().map(|&id| Insert { id }));
            actions.extend(ids.iter().step_by(2).map(|&id| Remove { id }));
            actions.extend(ids.iter().step_by(2).rev().map(|&id| Insert { id }));
            for &id in &ids[32..] {
                actions.extend([
                    Copy { src: id, dst: 2048 },
                    Remove { id },
                    Insert { id },
                    MulAdd {
                        src: id,
                        scalar: 7,
                        dst: 2048,
                    },
                    Move { src: 2048, dst: id },
                    Insert { id: 2048 },
                    Remove { id: 2048 },
                ]);
            }
        }
        "seeded" => {
            let mut random = c.seed;
            let mut live = reference::initial(c.width, c.seed);
            let pool: Vec<_> = (4..64).chain([127, 255, 511, 1023, 2047]).collect();
            for index in 0..256 {
                let choice = reference::next(&mut random) % 13;
                let mut ids: Vec<_> = live.keys().copied().collect();
                reference::shuffle(&mut ids, reference::next(&mut random));
                let src = ids[0];
                let dst = pool[(reference::next(&mut random) as usize) % pool.len()];
                let other_dst = if dst == src {
                    pool.iter().copied().find(|&i| i != src).unwrap()
                } else {
                    dst
                };
                let mutable = ids.iter().copied().find(|&i| i >= 4);
                let scalar = [0, 1, 7, 255][(reference::next(&mut random) % 4) as usize];
                let action = match choice {
                    0 => Insert { id: dst },
                    1 => Zero { id: dst },
                    2 => Copy {
                        src,
                        dst: other_dst,
                    },
                    3 => match mutable {
                        Some(src) => Move {
                            src,
                            dst: if dst == src { 4 + (src + 1) % 60 } else { dst },
                        },
                        None => Insert { id: dst },
                    },
                    4 => mutable.map_or(Insert { id: dst }, |id| Remove { id }),
                    5 => Scale { id: src, scalar },
                    6 => Alpha { id: src },
                    7 => Xor { src, dst: ids[1] },
                    8 => MulAdd {
                        src,
                        scalar,
                        dst: ids[1],
                    },
                    9 => XorTwo {
                        a: src,
                        b: ids[1],
                        dst: ids[2],
                    },
                    10 => Broadcast {
                        src,
                        destinations: vec![ids[1], ids[2]],
                    },
                    11 => Read { id: src },
                    _ => XorMany {
                        sources: ids[..3].to_vec(),
                        dst: ids[3],
                    },
                };
                reference::apply(&mut live, &action, c.width, c.seed.wrapping_add(index + 1));
                actions.push(action);
            }
        }
        _ => panic!("unknown V04 scenario {}", spec.scenario),
    }
    actions
        .into_iter()
        .enumerate()
        .map(|(index, action)| SequenceStep {
            action,
            seed: c.seed.wrapping_add(index as u64 + 1),
        })
        .collect()
}

fn panic_text(error: Box<dyn std::any::Any + Send>) -> String {
    error
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "non-string panic".into())
}

/// Bounded, exact first-difference diagnostics; never dump the whole state.
fn require_state(actual: &State, expected: &State, context: &str) {
    for (id, bytes) in expected {
        if actual.get(id) == Some(bytes) {
            continue;
        }
        let observed = actual.get(id);
        let offset = observed.map_or(0, |v| {
            v.iter()
                .zip(bytes)
                .position(|(a, b)| a != b)
                .unwrap_or(v.len().min(bytes.len()))
        });
        panic!(
            "{context}\nFirst difference: symbol {id}, byte {offset}; expected {}; actual {}; expected length {}; actual length {}.",
            bytes.get(offset).map_or("missing".into(), u8::to_string),
            observed
                .and_then(|v| v.get(offset))
                .map_or("missing".into(), u8::to_string),
            bytes.len(),
            observed.map_or("missing".into(), |v| v.len().to_string())
        );
    }
    assert_eq!(
        actual.len(),
        expected.len(),
        "{context}: unexpected observation keys"
    );
}

fn mixed_sequence<S: Subject>(c: &Case) -> Value {
    let spec = c
        .sequence
        .as_ref()
        .expect("V04 sequence specification missing");
    let steps = sequence_steps(c);
    let trace_digest = crate::digest(&serde_json::to_vec(&steps).unwrap());
    let mut state = reference::initial(c.width, c.seed);
    let mut subject = S::from_state(c.width, &state);
    let mut peak_live = state.len();
    let mut max_id = *state.keys().next_back().unwrap();
    let mut paths = [0usize; 2];
    for (index, step) in steps.iter().enumerate() {
        let direct = spec.dispatch.direct(index);
        // Insert/read have only a direct public entry, regardless of the trace route.
        let direct = direct || matches!(step.action, Action::Insert { .. } | Action::Read { .. });
        let before = crate::digest(&serde_json::to_vec(&state).unwrap());
        let recent = steps[index.saturating_sub(8)..index]
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "{}: {:?} (seed={})",
                    index.saturating_sub(8) + i + 1,
                    s.action,
                    s.seed
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let context = format!(
            "V04 {}: width={}, seed={}, route={}, step={}/{}, dispatch={}, input_seed={}, action={:?}\nTrace SHA-256: {trace_digest}; pre-state SHA-256: {before}.\nPrevious steps: {recent}",
            spec.scenario,
            c.width,
            c.seed,
            spec.dispatch.name(),
            index + 1,
            steps.len(),
            if direct { "direct" } else { "execute" },
            step.seed,
            step.action
        );
        reference::apply(&mut state, &step.action, c.width, step.seed);
        let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            subject.apply(&step.action, c.width, step.seed, direct);
            subject.observe(&state)
        }))
        .unwrap_or_else(|e| {
            panic!(
                "{context}\nComponent call or observation panicked: {}",
                panic_text(e)
            )
        });
        require_state(&observed, &state, &context);
        peak_live = peak_live.max(state.len());
        max_id = max_id.max(*state.keys().next_back().unwrap());
        paths[usize::from(!direct)] += 1;
    }
    json!({"contract":"defined-domain state history", "scenario":spec.scenario, "seed":c.seed,
        "route":spec.dispatch.name(), "trace_sha256":trace_digest, "checked_prefixes":steps.len(),
        "public_calls":steps.len(), "direct_calls":paths[0], "execute_calls":paths[1],
        "peak_live_symbols":peak_live, "maximum_id":max_id,
        "unobservable":["absence of removed IDs and physical allocation/reclamation; only live bytes and subsequent reuse are checked"],
        "state":reference::compact(&state)})
}

/// Apply the boundary's scope rule even when its semantic outcome is only a warning.
/// This is called outside the timer, for validation and every measured instance.
pub fn boundary_observation<S: Subject>(
    subject: &S,
    c: &Case,
    initial: &State,
    expected: &State,
    rejected: bool,
) -> Value {
    let contract = c.boundary.as_ref().expect("V06 contract missing");
    assert_eq!(
        c.warning, !contract.defined,
        "V06 severity/contract mismatch"
    );
    let context = format!(
        "V06 {}: width={}, dispatch={}; {} Basis: {}",
        c.workload,
        c.width,
        if c.direct { "direct" } else { "execute" },
        contract.expectation,
        contract.basis
    );
    let protected: State = initial
        .iter()
        .filter(|(id, _)| !contract.affected_ids.contains(id))
        .map(|(&id, bytes)| (id, bytes.clone()))
        .collect();
    let actual =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| subject.observe(&protected)))
            .unwrap_or_else(|e| {
                panic!(
                    "{context}\nProtected symbols became unreadable: {}",
                    panic_text(e)
                )
            });
    require_state(
        &actual,
        &protected,
        &format!("{context}\nProtected-symbol corruption (fatal even for an ambiguous boundary)."),
    );
    if rejected {
        assert!(
            !contract.defined,
            "{context}\nDefined boundary call was rejected."
        );
        return json!({"status":"rejected_by_panic","scope":"boundary call rejection latency; no completed operation"});
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| subject.observe(expected))) {
        Ok(actual) => {
            if contract.defined {
                require_state(&actual, expected, &context);
            }
            reference::compact(&actual)
        }
        Err(error) => {
            assert!(
                !contract.defined,
                "{context}\nDefined boundary output unreadable: {}",
                panic_text(error)
            );
            json!({"status":"output_unreadable_after_boundary_call"})
        }
    }
}
