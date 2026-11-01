//! Native operator binding. This source participates in I's content identity.
use crate::{Action, Probe, hit};
use fountain_engine::{traits::DataOperator, types::Operation};
use std::sync::Arc;
include!("selected.rs");
pub fn raw(width: usize) -> Selected {
    Selected::new(width)
}
pub struct Operator(Selected);
pub struct Program {
    operations: Vec<Operation>,
    direct: bool,
}
impl Program {
    pub fn new(actions: &[Action], direct: bool) -> Self {
        Self {
            direct,
            operations: actions
                .iter()
                .map(|a| match *a {
                    Action::Zero(id) => Operation::EnsureZeroOne { id },
                    Action::Range(start, end) => Operation::EnsureZeroRange { start, end },
                    Action::Copy(src_id, target_id) => Operation::CopyTo { src_id, target_id },
                    Action::Move(src_id, target_id) => Operation::MoveTo { src_id, target_id },
                    Action::Xor(src_id, target_id) => {
                        Operation::AddOneToVector { src_id, target_id }
                    }
                    Action::Scale(scalar, id) => Operation::MultiplyScalar { scalar, id },
                    Action::MulAdd(src_id, scalar, target_id) => Operation::MulAdd {
                        src_id,
                        scalar,
                        target_id,
                    },
                    Action::Remove(id) => Operation::Remove { id },
                })
                .collect(),
        }
    }
}
impl Operator {
    pub fn new(width: usize) -> Self {
        let mut op = raw(width);
        op.config_finite_field(0x11d);
        Self(op)
    }
    pub fn insert(&mut self, id: usize, bytes: &[u8]) {
        self.0.insert_vector(bytes, id)
    }
    pub fn read(&self, id: usize) -> Vec<u8> {
        self.0.get_vector(id).to_vec()
    }
    pub fn run(&mut self, program: &Program) {
        for operation in &program.operations {
            if !program.direct {
                self.0.execute(operation);
                continue;
            }
            match *operation {
                Operation::EnsureZeroOne { id } => self.0.ensure_zero_one(id),
                Operation::EnsureZeroRange { start, end } => self.0.ensure_zero_range(start..end),
                Operation::CopyTo { src_id, target_id } => self.0.copy_to(src_id, target_id),
                Operation::MoveTo { src_id, target_id } => self.0.move_to(src_id, target_id),
                Operation::AddOneToVector { src_id, target_id } => {
                    self.0.add_one_to_vector(src_id, target_id)
                }
                Operation::MultiplyScalar { scalar, id } => self.0.multiply_scalar(scalar, id),
                Operation::MulAdd {
                    src_id,
                    scalar,
                    target_id,
                } => self.0.mul_add(src_id, scalar, target_id),
                Operation::Remove { id } => self.0.remove(id),
                _ => unreachable!(),
            }
        }
    }
}
struct Observed {
    inner: Selected,
    probe: Arc<Probe>,
}
impl Drop for Observed {
    fn drop(&mut self) {
        hit(&self.probe.drops);
    }
}
impl DataOperator for Observed {
    fn insert_vector(&mut self, v: &[u8], i: usize) {
        hit(&self.probe.writes);
        self.inner.insert_vector(v, i)
    }
    fn get_vector(&self, i: usize) -> &[u8] {
        hit(&self.probe.reads);
        self.inner.get_vector(i)
    }
    fn config_finite_field(&mut self, p: u16) {
        self.inner.config_finite_field(p)
    }
    fn execute(&mut self, op: &Operation) {
        hit(&self.probe.arithmetic);
        self.inner.execute(op)
    }
}
pub fn boxed(width: usize, probe: Option<Arc<Probe>>) -> Box<dyn DataOperator> {
    match probe {
        Some(probe) => Box::new(Observed {
            inner: raw(width),
            probe,
        }),
        None => Box::new(raw(width)),
    }
}
