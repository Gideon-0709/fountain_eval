//! Fault injection runs the exact embedded contract checks without a production codec dependency.
pub use fountain_eval::content;
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
    result.map_err(|e| {
        e.downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap()
    })
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
