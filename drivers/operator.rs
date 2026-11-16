//! I-owned mapping of stable operations onto the selected native operator.
use crate::{Probe, content::Action, reference};
use fountain_engine::{traits::DataOperator, types::Operation};
use std::sync::Arc;
include!("selected.rs");
pub fn raw(width: usize) -> Selected {
    Selected::new(width)
}
pub struct Operator(pub Selected);
impl crate::contracts::Subject for Operator {
    fn from_state(width: usize, state: &reference::State) -> Self {
        let mut op = Self::new(width);
        for (id, bytes) in state {
            op.0.insert_vector(bytes, *id);
        }
        op
    }
    fn apply(&mut self, action: &Action, width: usize, seed: u64, direct: bool) {
        self.run(&Program::new(
            std::slice::from_ref(action),
            direct,
            width,
            seed,
        ));
    }
    fn observe(&self, live: &reference::State) -> reference::State {
        self.snapshot(live)
    }
}
pub struct Program {
    actions: Vec<Action>,
    operations: Vec<Option<Operation>>,
    direct: bool,
    input: Vec<u8>,
}
fn convert(a: &Action) -> Option<Operation> {
    use Action::*;
    Some(match a {
        Zero { id } => Operation::EnsureZeroOne { id: *id },
        ZeroList { ids } => Operation::EnsureZero {
            list_id: ids.clone(),
        },
        ZeroRange { start, count } => Operation::EnsureZeroRange {
            start: *start,
            end: start + count,
        },
        Alpha { id } => Operation::MultiplyAlpha { id: *id },
        Scale { id, scalar } => Operation::MultiplyScalar {
            id: *id,
            scalar: *scalar,
        },
        Xor { src, dst } => Operation::AddOneToVector {
            src_id: *src,
            target_id: *dst,
        },
        XorTwo { a, b, dst } => Operation::AddTwoToVector {
            s0: *a,
            s1: *b,
            target_id: *dst,
        },
        XorThree { a, b, c, dst } => Operation::AddThreeToVector {
            s0: *a,
            s1: *b,
            s2: *c,
            target_id: *dst,
        },
        XorMany { sources, dst } => Operation::AddToVector {
            list_id: sources.clone(),
            target_id: *dst,
        },
        Broadcast { src, destinations } => Operation::BroadcastAdd {
            src_id: *src,
            target_ids: destinations.clone(),
        },
        MulAdd { src, scalar, dst } => Operation::MulAdd {
            src_id: *src,
            scalar: *scalar,
            target_id: *dst,
        },
        Copy { src, dst } => Operation::CopyTo {
            src_id: *src,
            target_id: *dst,
        },
        CopyRange { src, dst, count } => Operation::CopyToN {
            src_start: *src,
            dst_start: *dst,
            count: *count,
        },
        Move { src, dst } => Operation::MoveTo {
            src_id: *src,
            target_id: *dst,
        },
        MoveRange { src, dst, count } => Operation::MoveToN {
            src_start: *src,
            dst_start: *dst,
            count: *count,
        },
        Remove { id } => Operation::Remove { id: *id },
        Info { coded, data } => Operation::InfoCodedVector {
            coded_id: *coded,
            data_id: *data,
        },
        Insert { .. } | Read { .. } | Alias { .. } | SharedWrite { .. } => return None,
    })
}
impl Program {
    pub fn new(actions: &[Action], direct: bool, width: usize, seed: u64) -> Self {
        Self {
            operations: actions.iter().map(convert).collect(),
            actions: actions.to_vec(),
            direct,
            input: reference::bytes(width, seed ^ 0xabcd),
        }
    }
}
#[allow(deprecated)]
pub fn dispatch(op: &mut dyn DataOperator, a: &Operation) {
    match a {
        Operation::EnsureZero { list_id } => op.ensure_zero(list_id),
        Operation::EnsureZeroOne { id } => op.ensure_zero_one(*id),
        Operation::EnsureZeroRange { start, end } => op.ensure_zero_range(*start..*end),
        Operation::MultiplyAlpha { id } => op.multiply_alpha(*id),
        Operation::MultiplyScalar { scalar, id } => op.multiply_scalar(*scalar, *id),
        Operation::AddOneToVector { src_id, target_id } => {
            op.add_one_to_vector(*src_id, *target_id)
        }
        Operation::AddTwoToVector { s0, s1, target_id } => {
            op.add_two_to_vector(*s0, *s1, *target_id)
        }
        Operation::AddThreeToVector {
            s0,
            s1,
            s2,
            target_id,
        } => op.add_three_to_vector(*s0, *s1, *s2, *target_id),
        Operation::AddToVector { list_id, target_id } => op.add_to_vector(list_id, *target_id),
        Operation::BroadcastAdd { src_id, target_ids } => op.broadcast_add(*src_id, target_ids),
        Operation::MulAdd {
            src_id,
            scalar,
            target_id,
        } => op.mul_add(*src_id, *scalar, *target_id),
        Operation::CopyTo { src_id, target_id } => op.copy_to(*src_id, *target_id),
        Operation::CopyToN {
            src_start,
            dst_start,
            count,
        } => op.copy_to_n(*src_start, *dst_start, *count),
        Operation::MoveTo { src_id, target_id } => op.move_to(*src_id, *target_id),
        Operation::MoveToN {
            src_start,
            dst_start,
            count,
        } => op.move_to_n(*src_start, *dst_start, *count),
        Operation::Remove { id } => op.remove(*id),
        Operation::InfoCodedVector { coded_id, data_id } => {
            op.info_coded_vector(*coded_id, *data_id)
        }
    }
}
pub fn name(op: &Operation) -> &'static str {
    match op {
        Operation::EnsureZero { .. } => "zero-list",
        Operation::EnsureZeroOne { .. } => "zero-one",
        Operation::EnsureZeroRange { .. } => "zero-range",
        Operation::MultiplyAlpha { .. } => "alpha",
        Operation::MultiplyScalar { .. } => "scale",
        Operation::AddOneToVector { .. } => "xor-one",
        Operation::AddTwoToVector { .. } => "xor-two",
        Operation::AddThreeToVector { .. } => "xor-three",
        Operation::AddToVector { .. } => "xor-many",
        Operation::BroadcastAdd { .. } => "broadcast",
        Operation::MulAdd { .. } => "mul-add",
        Operation::CopyTo { .. } => "copy",
        Operation::CopyToN { .. } => "copy-range",
        Operation::MoveTo { .. } => "move",
        Operation::MoveToN { .. } => "move-range",
        Operation::Remove { .. } => "remove",
        Operation::InfoCodedVector { .. } => "info",
    }
}
impl Operator {
    pub fn new(width: usize) -> Self {
        let mut op = raw(width);
        op.config_finite_field(0x11d);
        Self(op)
    }
    pub fn preload(width: usize, seed: u64) -> Self {
        let mut op = Self::new(width);
        for (id, v) in reference::initial(width, seed) {
            op.0.insert_vector(&v, id);
        }
        op
    }
    pub fn snapshot(&self, expected: &reference::State) -> reference::State {
        expected
            .keys()
            .map(|&id| (id, self.0.get_vector(id).to_vec()))
            .collect()
    }
    pub fn prepare_shared_write(&mut self, actions: &[Action]) {
        for action in actions {
            if let Action::SharedWrite { src, dst } = action {
                assert!(
                    alias_selected(&mut self.0, *src, *dst),
                    "alias capability declined during untimed state preparation"
                );
            }
        }
    }
    pub fn run(&mut self, program: &Program) {
        for (action, operation) in program.actions.iter().zip(&program.operations) {
            if let Some(operation) = operation {
                if program.direct {
                    dispatch(&mut self.0, operation);
                } else {
                    self.0.execute(operation);
                }
                continue;
            }
            match action {
                Action::Insert { id } => self.0.insert_vector(&program.input, *id),
                Action::Read { id } => {
                    std::hint::black_box(self.0.get_vector(*id));
                }
                Action::Alias { src, dst } => assert!(
                    alias_selected(&mut self.0, *src, *dst),
                    "registered alias capability declined"
                ),
                Action::SharedWrite { dst, .. } => self.0.multiply_scalar(7, *dst),
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
        self.probe.hit("drop");
    }
}
macro_rules! observed{($name:ident($($a:ident:$t:ty),*))=>{fn $name(&mut self,$($a:$t),*){self.probe.hit(stringify!($name));self.inner.$name($($a),*);}};}
#[allow(deprecated)]
impl DataOperator for Observed {
    fn insert_vector(&mut self, v: &[u8], id: usize) {
        self.probe.hit("write");
        self.inner.insert_vector(v, id);
    }
    fn get_vector(&self, id: usize) -> &[u8] {
        self.probe.hit("read");
        self.inner.get_vector(id)
    }
    fn config_finite_field(&mut self, p: u16) {
        self.probe.hit("field");
        self.inner.config_finite_field(p);
    }
    fn execute(&mut self, op: &Operation) {
        self.probe.hit(name(op));
        self.inner.execute(op);
    }
    observed!(ensure_zero(list_id:&[usize]));
    observed!(ensure_zero_one(id:usize));
    observed!(ensure_zero_range(range:std::ops::Range<usize>));
    observed!(multiply_alpha(id:usize));
    observed!(multiply_scalar(scalar:u8,id:usize));
    observed!(add_one_to_vector(src_id:usize,target_id:usize));
    observed!(add_two_to_vector(s0:usize,s1:usize,target_id:usize));
    observed!(add_three_to_vector(s0:usize,s1:usize,s2:usize,target_id:usize));
    observed!(add_to_vector(list_id:&[usize],target_id:usize));
    observed!(broadcast_add(src_id:usize,target_ids:&[usize]));
    observed!(mul_add(src_id:usize,scalar:u8,target_id:usize));
    observed!(copy_to(src_id:usize,target_id:usize));
    observed!(copy_to_n(src_start:usize,dst_start:usize,count:usize));
    observed!(move_to(src_id:usize,target_id:usize));
    observed!(move_to_n(src_start:usize,dst_start:usize,count:usize));
    observed!(remove(id:usize));
    observed!(info_coded_vector(coded_id:usize,data_id:usize));
    #[cfg(feature = "engine3")]
    fn alias_to(&mut self, src: usize, dst: usize) -> bool {
        self.probe.hit("alias");
        self.inner.alias_to(src, dst)
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
