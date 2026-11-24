//! Fault injection runs the exact embedded contract checks without a production codec dependency.
pub use fountain_eval::{content, findings};
#[path = "../drivers/contracts.rs"]
mod contracts;
#[allow(dead_code)]
#[path = "../drivers/reference.rs"]
mod reference;

use content::{Action, Case};
use contracts::Subject;
use reference::State;
use std::cell::Cell;

fn digest(bytes: &[u8]) -> String {
    fountain_eval::util::digest(&bytes)
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Fault {
    #[default]
    None,
    ArithmeticBothPaths,
    ExecuteOnly,
    BatchBothPaths,
    CopyAliases,
    CopiedWriteIgnored,
    BatchCopyAliases,
    MoveReuse,
    HistoryLate,
    HistoryPanic,
    HistoryReuse,
}
thread_local! { static FAULT: Cell<Fault> = const { Cell::new(Fault::None) }; }
struct Fixture {
    state: State,
    fault: Fault,
    aliases: Vec<(usize, usize)>,
}
impl Subject for Fixture {
    fn from_state(_width: usize, state: &State) -> Self {
        Self {
            state: state.clone(),
            fault: FAULT.get(),
            aliases: vec![],
        }
    }
    fn apply(&mut self, action: &Action, width: usize, seed: u64, direct: bool) {
        if self.fault == Fault::CopiedWriteIgnored
            && let Action::Scale { id, .. } = action
            && self.aliases.iter().any(|(_, dst)| dst == id)
        {
            return;
        }
        if self.fault == Fault::ExecuteOnly && !direct && matches!(action, Action::Xor { .. }) {
            return;
        }
        if self.fault == Fault::BatchBothPaths
            && let Action::XorTwo { a, dst, .. } = action
        {
            reference::apply(
                &mut self.state,
                &Action::Xor { src: *a, dst: *dst },
                width,
                seed,
            );
            return;
        }
        reference::apply(&mut self.state, action, width, seed);
        if self.fault == Fault::ArithmeticBothPaths
            && let Action::MulAdd { scalar: 0, dst, .. } = action
        {
            self.state.get_mut(dst).unwrap().fill(0);
        }
        match action {
            Action::Copy { src, dst }
                if matches!(self.fault, Fault::CopyAliases | Fault::CopiedWriteIgnored) =>
            {
                self.aliases.push((*src, *dst))
            }
            Action::CopyRange { src, dst, count } if self.fault == Fault::BatchCopyAliases => {
                self.aliases.extend((0..*count).map(|i| (src + i, dst + i)));
            }
            Action::Move { src, dst } if self.fault == Fault::MoveReuse => {
                self.aliases.push((*src, *dst))
            }
            Action::Insert { id } | Action::Scale { id, .. }
                if self.fault != Fault::CopiedWriteIgnored =>
            {
                for &(src, dst) in &self.aliases {
                    if *id == src {
                        self.state.insert(dst, self.state[id].clone());
                    }
                    if *id == dst {
                        self.state.insert(src, self.state[id].clone());
                    }
                }
            }
            _ => {}
        }
    }
    fn observe(&self, live: &State) -> State {
        // An attempt to read a removed ID panics, so passing fixtures also verify safe observations.
        live.keys()
            .map(|id| (*id, self.state[id].clone()))
            .collect()
    }
}
fn case(workload: &str, width: usize) -> Case {
    content::cases()
        .into_iter()
        .find(|c| c.workload == workload && c.width == width)
        .unwrap()
}
fn run(workload: &str, fault: Fault) -> Result<serde_json::Value, String> {
    FAULT.set(fault);
    let result = std::panic::catch_unwind(|| contracts::validate::<Fixture>(&case(workload, 31)));
    FAULT.set(Fault::None);
    result.map_err(|e| findings::panic_message(e.as_ref()))
}

#[test]
fn independent_values_storage_and_equivalence_pass_on_valid_behavior() {
    for work in [
        "operator-reference",
        "storage-ownership",
        "dispatch-equivalence",
    ] {
        assert!(run(work, Fault::None).is_ok());
    }
    let checks = contracts::validate::<Fixture>(&case("operator-reference", 1));
    assert_eq!(checks["exhaustive_scalar_checks"], 65536);
}

#[test]
fn actual_check_sites_emit_the_same_zero_mul_add_contract_across_families() {
    use findings::{Failure, FailureKind};
    FAULT.set(Fault::ArithmeticBothPaths);
    let arithmetic =
        std::panic::catch_unwind(|| contracts::validate::<Fixture>(&case("operator-reference", 1)))
            .unwrap_err()
            .downcast::<Failure>()
            .unwrap()
            .violation;
    let history = content::cases()
        .into_iter()
        .find(|c| {
            c.width == 1
                && c.workload == "mixed-sequence"
                && c.sequence.as_ref().is_some_and(|s| s.scenario == "seeded")
        })
        .unwrap();
    let sequence = std::panic::catch_unwind(|| contracts::validate::<Fixture>(&history))
        .unwrap_err()
        .downcast::<Failure>()
        .unwrap()
        .violation;
    FAULT.set(Fault::None);
    assert_eq!(arithmetic.contract_id, "operator.mul_add.zero");
    assert_eq!(arithmetic.context["dispatch"], "direct");
    assert_eq!(arithmetic.contract_id, sequence.contract_id);
    assert_eq!(sequence.failure_kind, FailureKind::ValueMismatch);
    assert!(sequence.context["step"].as_u64().unwrap() > 0);
    assert_eq!(sequence.context["operation"]["op"], "mul_add");
    assert_eq!(sequence.difference.unwrap().right, serde_json::json!(0));
}

#[test]
fn equal_wrong_arithmetic_is_owned_by_v02_not_a_path_disagreement() {
    let error = run("operator-reference", Fault::ArithmeticBothPaths).unwrap_err();
    assert!(error.contains("V02") && error.contains("MulAdd") && error.contains("scalar: 0"));
    assert!(run("storage-ownership", Fault::ArithmeticBothPaths).is_ok());
    assert!(run("dispatch-equivalence", Fault::ArithmeticBothPaths).is_ok());
}

#[test]
fn copy_isolation_and_move_reuse_are_owned_by_v03() {
    for fault in [
        Fault::CopyAliases,
        Fault::CopiedWriteIgnored,
        Fault::MoveReuse,
    ] {
        assert!(run("operator-reference", fault).is_ok());
        let error = run("storage-ownership", fault).unwrap_err();
        assert!(error.contains("V03") && error.contains("independence/reuse"));
    }
}

#[test]
fn execute_only_defects_are_owned_by_v05() {
    assert!(run("operator-reference", Fault::ExecuteOnly).is_ok());
    assert!(run("storage-ownership", Fault::ExecuteOnly).is_ok());
    assert!(
        run("dispatch-equivalence", Fault::ExecuteOnly)
            .unwrap_err()
            .contains("V05 direct versus execute")
    );
}

#[test]
fn batch_defects_cannot_hide_behind_identical_dispatch_paths() {
    for fault in [Fault::BatchBothPaths, Fault::BatchCopyAliases] {
        assert!(run("operator-reference", fault).is_ok());
        assert!(run("storage-ownership", fault).is_ok());
        let error = run("dispatch-equivalence", fault).unwrap_err();
        assert!(error.contains("V05 batch versus elementary direct calls"));
    }
}

struct HistoryFixture {
    inner: Fixture,
    calls: usize,
    removed: std::collections::BTreeSet<usize>,
}
impl Subject for HistoryFixture {
    fn from_state(width: usize, state: &State) -> Self {
        Self {
            inner: Fixture::from_state(width, state),
            calls: 0,
            removed: Default::default(),
        }
    }
    fn apply(&mut self, action: &Action, width: usize, seed: u64, direct: bool) {
        self.calls += 1;
        if self.inner.fault == Fault::HistoryPanic && self.calls == 73 {
            panic!("injected delayed failure");
        }
        self.inner.apply(action, width, seed, direct);
        if self.inner.fault == Fault::HistoryLate && self.calls == 73 {
            self.inner.state.get_mut(&0).unwrap()[0] ^= 1;
        }
        if let Action::Remove { id } = action {
            self.removed.insert(*id);
        }
        if self.inner.fault == Fault::HistoryReuse
            && let Action::Insert { id } = action
            && *id >= 128
            && self.removed.contains(id)
        {
            self.inner.state.get_mut(id).unwrap()[0] ^= 1;
        }
    }
    fn observe(&self, live: &State) -> State {
        self.inner.observe(live)
    }
}
fn history_case(scenario: &str, dispatch: content::SequenceDispatch) -> Case {
    content::cases()
        .into_iter()
        .find(|c| {
            c.width == 31
                && c.sequence
                    .as_ref()
                    .is_some_and(|s| s.scenario == scenario && s.dispatch == dispatch)
        })
        .unwrap()
}
fn history(c: &Case, fault: Fault) -> Result<serde_json::Value, String> {
    FAULT.set(fault);
    let result = std::panic::catch_unwind(|| contracts::validate::<HistoryFixture>(c));
    FAULT.set(Fault::None);
    result.map_err(|e| findings::panic_message(e.as_ref()))
}
#[test]
fn histories_are_reproducible_valid_and_identical_across_routes() {
    use content::SequenceDispatch::*;
    for scenario in ["copy-chain", "move-reuse", "sparse-churn", "seeded"] {
        let mut fingerprint = None;
        for dispatch in [Direct, Execute, Mixed] {
            let c = history_case(scenario, dispatch);
            let result = history(&c, Fault::None).unwrap();
            let steps = contracts::sequence_steps(&c);
            assert_eq!(result["checked_prefixes"], steps.len());
            assert_eq!(result["public_calls"], steps.len());
            assert_eq!(
                result["direct_calls"].as_u64().unwrap()
                    + result["execute_calls"].as_u64().unwrap(),
                steps.len() as u64
            );
            if let Some(expected) = &fingerprint {
                assert_eq!(expected, &result["trace_sha256"]);
            }
            fingerprint = Some(result["trace_sha256"].clone());
            assert_eq!(
                serde_json::to_value(&steps).unwrap(),
                serde_json::to_value(contracts::sequence_steps(&c)).unwrap()
            );
            if scenario == "sparse-churn" {
                assert_eq!(result["maximum_id"], 2048);
                assert!(result["peak_live_symbols"].as_u64().unwrap() >= 80);
            }
        }
    }
    // Every registered generated seed reaches valid states with no removed-ID reads.
    for c in content::cases()
        .into_iter()
        .filter(|c| c.width == 1 && c.sequence.is_some())
    {
        history(&c, Fault::None).unwrap();
    }
    let a = history_case("seeded", Direct);
    let mut b = a.clone();
    b.seed = content::SEEDS[1];
    assert_ne!(
        serde_json::to_value(contracts::sequence_steps(&a)).unwrap(),
        serde_json::to_value(contracts::sequence_steps(&b)).unwrap()
    );
}
#[test]
fn sequence_failures_preserve_first_step_byte_and_replay_context() {
    let c = history_case("copy-chain", content::SequenceDispatch::Mixed);
    let error = history(&c, Fault::HistoryLate).unwrap_err();
    for text in [
        "V04 copy-chain",
        "width=31",
        "route=mixed",
        "step=73/144",
        "dispatch=direct",
        "input_seed=",
        "action=",
        "Trace SHA-256",
        "pre-state SHA-256",
        "Previous steps:",
        "First difference: symbol 0, byte 0",
        "expected",
        "actual",
    ] {
        assert!(error.contains(text), "missing {text}: {error}");
    }
    assert!(error.len() < 2500);
    let error = history(&c, Fault::HistoryPanic).unwrap_err();
    assert!(error.contains("step=73/144") && error.contains("injected delayed failure"));
}
#[test]
fn histories_detect_sparse_reuse_and_route_specific_corruption() {
    use content::SequenceDispatch::*;
    let error = history(&history_case("sparse-churn", Direct), Fault::HistoryReuse).unwrap_err();
    assert!(error.contains("V04 sparse-churn") && error.contains("action=Insert"));
    assert!(history(&history_case("copy-chain", Direct), Fault::ExecuteOnly).is_ok());
    for route in [Execute, Mixed] {
        assert!(
            history(&history_case("copy-chain", route), Fault::ExecuteOnly)
                .unwrap_err()
                .contains("dispatch=execute")
        );
    }
}

fn boundary_case(name: &str) -> Case {
    content::cases()
        .into_iter()
        .find(|c| c.category == "correctness" && c.workload == name && c.width == 31 && c.direct)
        .unwrap()
}
#[test]
fn boundary_policy_is_explicit_and_shared_with_measurement_cases() {
    let recipe = fountain_eval::catalog::recipe(&fountain_eval::protocol::TestReference {
        version: "0.1.0".into(),
    })
    .unwrap();
    let mut validation_count = 0;
    let mut measurement_count = 0;
    for check in recipe.checks.values().filter(|c| c.spec.family == "V06") {
        let defined = matches!(check.spec.workload.as_str(), "self-copy" | "empty-xor");
        let policy = check.spec.boundary.as_ref().unwrap();
        assert_eq!(policy.defined, defined);
        assert_eq!(check.spec.warning, !defined);
        assert_eq!(
            check.severity == fountain_eval::validation::Severity::Fatal,
            defined
        );
        assert!(!policy.expectation.is_empty() && !policy.basis.is_empty());
        if check.measured {
            assert!(check.spec.direct);
            measurement_count += 1;
        } else {
            validation_count += 1;
        }
    }
    assert_eq!(
        validation_count,
        9 * content::correctness_widths().len() * 2
    );
    assert_eq!(measurement_count, 9 * content::SYMBOL_WIDTHS.len() * 2);
}
#[test]
fn boundary_deviations_have_distinct_fatal_and_warning_outcomes() {
    FAULT.set(Fault::None);
    for b in content::boundaries() {
        let c = boundary_case(b.name);
        let initial = reference::initial(c.width, c.seed);
        let mut expected = initial.clone();
        reference::apply(&mut expected, &b.action, c.width, c.seed);
        let good = Fixture::from_state(c.width, &expected);
        assert_eq!(
            contracts::boundary_observation(&good, &c, &initial, &expected, false),
            reference::compact(&expected)
        );
        let mut bad = Fixture::from_state(c.width, &expected);
        let id = *b
            .contract
            .affected_ids
            .iter()
            .find(|id| expected.contains_key(id))
            .unwrap();
        bad.state.get_mut(&id).unwrap()[0] ^= 1;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contracts::boundary_observation(&bad, &c, &initial, &expected, false)
        }));
        if b.contract.defined {
            assert!(result.is_err());
        } else {
            assert_ne!(result.unwrap(), reference::compact(&expected));
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contracts::boundary_observation(&good, &c, &initial, &expected, true)
        }));
        if b.contract.defined {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap()["status"], "rejected_by_panic");
        }
        bad.state.remove(&id);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contracts::boundary_observation(&bad, &c, &initial, &expected, false)
        }));
        if b.contract.defined {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result.unwrap()["status"],
                "output_unreadable_after_boundary_call"
            );
        }
    }
}
#[test]
fn ambiguous_boundaries_never_excuse_unrelated_symbol_corruption() {
    FAULT.set(Fault::None);
    for b in content::boundaries()
        .into_iter()
        .filter(|b| !b.contract.defined)
    {
        let c = boundary_case(b.name);
        let initial = reference::initial(c.width, c.seed);
        let mut expected = initial.clone();
        reference::apply(&mut expected, &b.action, c.width, c.seed);
        for rejected in [false, true] {
            let mut bad = Fixture::from_state(c.width, &expected);
            bad.state.get_mut(&15).unwrap()[0] ^= 1;
            let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                contracts::boundary_observation(&bad, &c, &initial, &expected, rejected)
            }))
            .unwrap_err();
            let text = findings::panic_message(error.as_ref());
            assert!(
                text.contains("Protected-symbol corruption") && text.contains("symbol 15, byte 0")
            );
        }
    }
}

#[test]
fn shared_rq_sizes_have_independent_reference_geometry() {
    // RFC 6330, Table 2, mapped to the registered source counts.
    let expected = [
        (69, 92),
        (138, 167),
        (257, 296),
        (526, 577),
        (1032, 1101),
        (2070, 2170),
        (4112, 4260),
        (8194, 8416),
    ];
    assert_eq!(
        content::SOURCE_COUNTS.map(|k| reference::geometry("rq", k)),
        expected,
    );
}
