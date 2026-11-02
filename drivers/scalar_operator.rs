// I-owned DataOperator adapter used only to validate engine-only plans outside timers.
pub struct Selected {
    width: usize,
    values: reference::State,
}
impl Selected {
    pub fn new(width: usize) -> Self {
        Self {
            width,
            values: Default::default(),
        }
    }
}
#[allow(deprecated)]
impl DataOperator for Selected {
    fn insert_vector(&mut self, bytes: &[u8], id: usize) {
        assert_eq!(bytes.len(), self.width);
        self.values.insert(id, bytes.to_vec());
    }
    fn get_vector(&self, id: usize) -> &[u8] {
        &self.values[&id]
    }
    fn config_finite_field(&mut self, polynomial: u16) {
        assert!(
            polynomial == fountain_engine::types::GF2_FIELD_POLY || polynomial == 0x11d,
            "unsupported reference polynomial"
        );
    }
    fn execute(&mut self, op: &Operation) {
        let action = match op {
            Operation::EnsureZero { list_id } => Action::ZeroList {
                ids: list_id.clone(),
            },
            Operation::EnsureZeroOne { id } => Action::Zero { id: *id },
            Operation::EnsureZeroRange { start, end } => Action::ZeroRange {
                start: *start,
                count: end - start,
            },
            Operation::MultiplyAlpha { id } => Action::Alpha { id: *id },
            Operation::MultiplyScalar { id, scalar } => Action::Scale {
                id: *id,
                scalar: *scalar,
            },
            Operation::AddOneToVector { src_id, target_id } => Action::Xor {
                src: *src_id,
                dst: *target_id,
            },
            Operation::AddTwoToVector { s0, s1, target_id } => Action::XorTwo {
                a: *s0,
                b: *s1,
                dst: *target_id,
            },
            Operation::AddThreeToVector {
                s0,
                s1,
                s2,
                target_id,
            } => Action::XorThree {
                a: *s0,
                b: *s1,
                c: *s2,
                dst: *target_id,
            },
            Operation::AddToVector { list_id, target_id } => Action::XorMany {
                sources: list_id.clone(),
                dst: *target_id,
            },
            Operation::BroadcastAdd { src_id, target_ids } => Action::Broadcast {
                src: *src_id,
                destinations: target_ids.clone(),
            },
            Operation::MulAdd {
                src_id,
                scalar,
                target_id,
            } => Action::MulAdd {
                src: *src_id,
                scalar: *scalar,
                dst: *target_id,
            },
            Operation::CopyTo { src_id, target_id } => Action::Copy {
                src: *src_id,
                dst: *target_id,
            },
            Operation::CopyToN {
                src_start,
                dst_start,
                count,
            } => Action::CopyRange {
                src: *src_start,
                dst: *dst_start,
                count: *count,
            },
            Operation::MoveTo { src_id, target_id } => Action::Move {
                src: *src_id,
                dst: *target_id,
            },
            Operation::MoveToN {
                src_start,
                dst_start,
                count,
            } => Action::MoveRange {
                src: *src_start,
                dst: *dst_start,
                count: *count,
            },
            Operation::Remove { id } => Action::Remove { id: *id },
            Operation::InfoCodedVector { coded_id, data_id } => Action::Info {
                coded: *coded_id,
                data: *data_id,
            },
        };
        // Symbolic plans use absent XOR operands for implicit zero rows. Materialize
        // those zeros in this validation adapter; the scalar oracle itself keeps
        // its strict initialized-state contract for ordinary operator tests.
        let zeros = match &action {
            Action::Xor { src, dst } => vec![*src, *dst],
            Action::XorTwo { a, b, dst } => vec![*a, *b, *dst],
            Action::XorThree { a, b, c, dst } => vec![*a, *b, *c, *dst],
            Action::XorMany { sources, dst } => sources.iter().copied().chain([*dst]).collect(),
            Action::MulAdd { dst, .. } => vec![*dst],
            Action::Broadcast { destinations, .. } => destinations.clone(),
            _ => vec![],
        };
        for id in zeros {
            self.values.entry(id).or_insert_with(|| vec![0; self.width]);
        }
        reference::apply(&mut self.values, &action, self.width, 0);
    }
}
fn alias_selected(_op: &mut Selected, _src: usize, _dst: usize) -> bool {
    false
}
