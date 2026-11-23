//! T-owned contracts and structured violations shared with isolated drivers.
use crate::content::{Action, Case};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};

#[derive(Clone, Debug)]
pub struct Contract {
    pub id: String,
    pub requirement: String,
}
impl Contract {
    pub fn new(id: impl Into<String>, requirement: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            requirement: requirement.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    ValueMismatch,
    LengthMismatch,
    UnexpectedKeys,
    PathMismatch,
    CallRejected,
    OutputUnreadable,
    ScopeCorruption,
    UnexpectedPanic,
    ExecutionFailure,
    AssertionFailed,
    ReferenceDeviation,
    PrematureCompletion,
    IncompleteRecovery,
    MissingParticipation,
    OwnershipMismatch,
}
impl FailureKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ValueMismatch => "Incorrect values",
            Self::LengthMismatch => "Incorrect output length",
            Self::UnexpectedKeys => "Unexpected stored symbols",
            Self::PathMismatch => "Execution paths disagree",
            Self::CallRejected => "Call rejected",
            Self::OutputUnreadable => "Output unreadable",
            Self::ScopeCorruption => "Unrelated symbols changed",
            Self::UnexpectedPanic => "Unexpected panic",
            Self::ExecutionFailure => "Execution failed",
            Self::AssertionFailed => "Requirement not satisfied",
            Self::ReferenceDeviation => "Boundary reference deviation",
            Self::PrematureCompletion => "Completion reported too early",
            Self::IncompleteRecovery => "Full-rank input did not complete",
            Self::MissingParticipation => "Selected component did not participate",
            Self::OwnershipMismatch => "Ownership or release mismatch",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Difference {
    pub location: String,
    pub left_label: String,
    pub right_label: String,
    pub left: Value,
    pub right: Value,
    pub left_length: Option<usize>,
    pub right_length: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Violation {
    pub contract_id: String,
    pub failure_kind: FailureKind,
    pub requirement: String,
    pub context: Value,
    pub difference: Option<Difference>,
}
impl Violation {
    pub fn validate(&self, case: &str) -> bool {
        !self.contract_id.is_empty()
            && self
                .contract_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
            && !self.requirement.trim().is_empty()
            && self.context.is_object()
            && self.context["case"] == case
    }
}
#[derive(Clone, Debug)]
pub struct Failure {
    pub violation: Violation,
    pub message: String,
}
thread_local! { static ACTIVE: RefCell<Option<(Contract, Value)>> = const { RefCell::new(None) }; }

pub fn case_contract(c: &Case) -> Contract {
    Contract::new(
        format!("case.{}.execution", c.workload),
        "The registered check must execute and produce its required observations.",
    )
}
pub fn case_context(c: &Case) -> Value {
    json!({"case":c.id,"width":c.width,"seed":c.seed,"scheme":c.scheme,"k":c.k,
        "dispatch":if c.actions.is_empty(){None}else{Some(if c.direct{"direct"}else{"execute"})}})
}
fn merged(mut base: Value, extra: Value) -> Value {
    for (key, value) in extra.as_object().into_iter().flatten() {
        base.as_object_mut()
            .unwrap()
            .insert(key.clone(), value.clone());
    }
    base
}
pub fn scope<R>(contract: Contract, context: Value, run: impl FnOnce() -> R) -> R {
    let base = ACTIVE.with(|a| a.borrow().as_ref().map(|(_, old)| old.clone()));
    let context = base.map_or_else(|| context.clone(), |old| merged(old, context.clone()));
    let old = ACTIVE.with(|a| a.replace(Some((contract.clone(), context.clone()))));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
    ACTIVE.with(|a| {
        a.replace(old);
    });
    match result {
        Ok(result) => result,
        Err(error) => std::panic::resume_unwind(Box::new(from_panic(error, contract, context))),
    }
}
pub fn from_panic(
    error: Box<dyn std::any::Any + Send>,
    contract: Contract,
    context: Value,
) -> Failure {
    match error.downcast::<Failure>() {
        Ok(failure) => *failure,
        Err(error) => {
            let message = error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "non-string panic".into());
            let message = context["diagnostic"].as_str().map_or_else(
                || message.clone(),
                |diagnostic| format!("{diagnostic}\nUnexpected panic: {message}"),
            );
            Failure {
                violation: Violation {
                    contract_id: contract.id,
                    requirement: contract.requirement,
                    failure_kind: FailureKind::UnexpectedPanic,
                    context,
                    difference: None,
                },
                message,
            }
        }
    }
}
pub fn panic_message(error: &(dyn std::any::Any + Send)) -> String {
    error
        .downcast_ref::<Failure>()
        .map(|f| f.message.clone())
        .or_else(|| error.downcast_ref::<String>().cloned())
        .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "non-string panic".into())
}
pub fn fail(
    contract: Contract,
    kind: FailureKind,
    extra: Value,
    difference: Option<Difference>,
    message: String,
) -> ! {
    let context = ACTIVE.with(|a| {
        a.borrow()
            .as_ref()
            .map_or_else(|| json!({}), |(_, ctx)| ctx.clone())
    });
    std::panic::resume_unwind(Box::new(Failure {
        violation: Violation {
            contract_id: contract.id,
            requirement: contract.requirement,
            failure_kind: kind,
            context: merged(context, extra),
            difference,
        },
        message,
    }))
}
pub fn require(
    contract: Contract,
    kind: FailureKind,
    condition: bool,
    message: impl FnOnce() -> String,
) {
    if !condition {
        fail(contract, kind, json!({}), None, message());
    }
}
pub fn equal<A: PartialEq<B> + Serialize, B: Serialize>(
    contract: Contract,
    kind: FailureKind,
    actual: &A,
    expected: &B,
    message: impl FnOnce() -> String,
) {
    if actual != expected {
        fail(
            contract,
            kind,
            json!({}),
            Some(Difference {
                location: "Checked value".into(),
                left_label: "Expected".into(),
                right_label: "Observed".into(),
                left: serde_json::to_value(expected).unwrap(),
                right: serde_json::to_value(actual).unwrap(),
                left_length: None,
                right_length: None,
            }),
            message(),
        );
    }
}
pub fn bytes(contract: Contract, expected: &[u8], actual: &[u8], message: impl FnOnce() -> String) {
    if expected == actual {
        return;
    }
    let offset = expected
        .iter()
        .zip(actual)
        .position(|(a, b)| a != b)
        .unwrap_or(expected.len().min(actual.len()));
    fail(
        contract,
        if expected.len() == actual.len() {
            FailureKind::ValueMismatch
        } else {
            FailureKind::LengthMismatch
        },
        json!({}),
        Some(Difference {
            location: format!("Output byte {offset}"),
            left_label: "Expected".into(),
            right_label: "Observed".into(),
            left: json!(expected.get(offset)),
            right: json!(actual.get(offset)),
            left_length: Some(expected.len()),
            right_length: Some(actual.len()),
        }),
        format!(
            "{}; byte {offset}, expected {:?}, actual {:?}; lengths {} and {}",
            message(),
            expected.get(offset),
            actual.get(offset),
            expected.len(),
            actual.len()
        ),
    );
}
pub fn states(
    contract: Contract,
    left: &BTreeMap<usize, Vec<u8>>,
    right: &BTreeMap<usize, Vec<u8>>,
    labels: (&str, &str),
    kind: FailureKind,
    message: &str,
) {
    for id in left.keys().chain(right.keys()) {
        if left.get(id) == right.get(id) {
            continue;
        }
        let a = left.get(id).map(Vec::as_slice).unwrap_or_default();
        let b = right.get(id).map(Vec::as_slice).unwrap_or_default();
        let offset = a
            .iter()
            .zip(b)
            .position(|(a, b)| a != b)
            .unwrap_or(a.len().min(b.len()));
        fail(
            contract,
            if kind == FailureKind::ValueMismatch
                && (left.get(id).is_none() || right.get(id).is_none())
            {
                FailureKind::UnexpectedKeys
            } else {
                kind
            },
            json!({}),
            Some(Difference {
                location: format!("Symbol {id}, byte {offset}"),
                left_label: labels.0.into(),
                right_label: labels.1.into(),
                left: json!(a.get(offset)),
                right: json!(b.get(offset)),
                left_length: left.get(id).map(Vec::len),
                right_length: right.get(id).map(Vec::len),
            }),
            format!(
                "{message}\nFirst difference: symbol {id}, byte {offset}; expected {}; actual {}; expected length {}; actual length {}.",
                a.get(offset).map_or("missing".into(), u8::to_string),
                b.get(offset).map_or("missing".into(), u8::to_string),
                a.len(),
                b.len()
            ),
        );
    }
}
pub fn operator_contract(action: &Action) -> Contract {
    let (name, requirement) = match action {
        Action::MulAdd { scalar: 0, .. } => (
            "mul_add.zero",
            "Multiply-add with scalar 0 must preserve the destination.",
        ),
        Action::MulAdd { .. } => (
            "mul_add",
            "Multiply-add must match the independent GF reference.",
        ),
        Action::Scale { .. } => (
            "scale",
            "Scalar multiplication must match the independent GF reference.",
        ),
        Action::Alpha { .. } => (
            "alpha",
            "Multiplication by alpha must match the independent GF reference.",
        ),
        Action::Xor { .. } => ("xor", "XOR must match the independent byte reference."),
        Action::XorTwo { .. }
        | Action::XorThree { .. }
        | Action::XorMany { .. }
        | Action::Broadcast { .. } => (
            "xor.composition",
            "Multi-source XOR must match the independent byte reference.",
        ),
        Action::Copy { .. } | Action::CopyRange { .. } => (
            "copy",
            "Copy must preserve source bytes and produce the required destination bytes.",
        ),
        Action::Move { .. } | Action::MoveRange { .. } => (
            "move",
            "Move must produce the required destination bytes and support subsequent ID reuse.",
        ),
        Action::Zero { .. } | Action::ZeroList { .. } | Action::ZeroRange { .. } => (
            "zero",
            "Zeroing must clear the requested symbols and preserve unrelated symbols.",
        ),
        Action::Insert { .. } => (
            "insert",
            "Insertion must store the supplied bytes and preserve unrelated symbols.",
        ),
        Action::Read { .. } => (
            "read",
            "Reading must return the stored bytes and preserve storage.",
        ),
        Action::Remove { .. } => (
            "remove",
            "Removal must preserve surviving symbols and support ID reuse.",
        ),
        Action::Info { .. } => (
            "info",
            "Informational association must preserve stored symbol bytes.",
        ),
        Action::Alias { .. } | Action::SharedWrite { .. } => (
            "sharing",
            "Sharing and first writes must preserve the required symbol values and isolation.",
        ),
    };
    Contract::new(format!("operator.{name}"), requirement)
}
pub fn boundary_violation(c: &Case, expected: &Value, actual: &Value) -> Option<Violation> {
    if expected == actual {
        return None;
    }
    let kind = match actual["status"].as_str() {
        Some("rejected_by_panic") => FailureKind::CallRejected,
        Some("output_unreadable_after_boundary_call") => FailureKind::OutputUnreadable,
        _ => FailureKind::ReferenceDeviation,
    };
    Some(Violation {
        contract_id: format!("boundary.{}.reference", c.workload),
        failure_kind: kind,
        requirement: c
            .boundary
            .as_ref()
            .map_or(
                "The boundary observation must match the registered reference.",
                |b| b.expectation.as_str(),
            )
            .into(),
        context: case_context(c),
        difference: None,
    })
}
pub fn verify_boundary(c: &Case, expected: &Value, actual: &Value, value: &Value) -> bool {
    match boundary_violation(c, expected, actual) {
        None => value.is_null(),
        Some(expected) => serde_json::from_value::<Violation>(value.clone()).is_ok_and(|v| {
            v.validate(&c.id)
                && v.contract_id == expected.contract_id
                && v.failure_kind == expected.failure_kind
                && v.requirement == expected.requirement
        }),
    }
}
