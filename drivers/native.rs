//! Native API integration. Semantic stage names are independent of upstream implementation names.
use crate::{Probe, Stages, operator};
use fountain_engine::{
    traits::{CodeScheme, DataOperator},
    types::{DecodeStatus, Operation},
};
use fountain_scheme::rfc::{r10::Raptor10SysCode, rq::RaptorQSysCode};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
pub struct Encoder {
    inner: fountain_engine::Encoder,
    base: usize,
    k: usize,
    block_k: usize,
}
pub struct Decoder {
    inner: fountain_engine::Decoder,
    k: usize,
}
pub struct Plan {
    inner: Encoder,
    operations: Vec<Operation>,
    block_k: usize,
    field: u16,
}

fn encoder_with<S: CodeScheme>(
    scheme: S,
    k: usize,
    width: usize,
    source: &[u8],
    probe: Option<Arc<Probe>>,
    stages: &mut Stages,
) -> Encoder {
    let params = scheme.get_params();
    let mut op = operator::boxed(width, probe);
    stages.mark("scheme-and-storage-setup");
    for (i, bytes) in source.chunks_exact(width).enumerate() {
        op.insert_vector(bytes, i);
    }
    for i in k..params.k {
        op.insert_vector(&vec![0; width], i);
    }
    stages.mark("source-loading-and-padding");
    let inner =
        fountain_engine::Encoder::new_with_operator_execute_only_and_num_source(&scheme, op, k);
    stages.mark("encoder-context-and-plan-application");
    Encoder {
        inner,
        base: params.num_total(),
        k,
        block_k: params.k,
    }
}
impl Encoder {
    pub fn new(
        codec: &str,
        k: usize,
        width: usize,
        source: &[u8],
        probe: Option<Arc<Probe>>,
    ) -> Self {
        Self::new_staged(codec, k, width, source, probe, &mut Stages::disabled())
    }
    pub fn new_staged(
        codec: &str,
        k: usize,
        width: usize,
        source: &[u8],
        probe: Option<Arc<Probe>>,
        stages: &mut Stages,
    ) -> Self {
        match codec {
            "rq" => encoder_with(
                RaptorQSysCode::new_with_default_setting(k),
                k,
                width,
                source,
                probe,
                stages,
            ),
            "r10" => encoder_with(
                Raptor10SysCode::new_with_default_setting(k),
                k,
                width,
                source,
                probe,
                stages,
            ),
            _ => unreachable!(),
        }
    }
    pub fn repair_base(&self) -> usize {
        self.base
    }
    pub fn generate(&mut self, id: usize) -> usize {
        self.inner.encode_coded_vector(id).expect("encode failed")
    }
    pub fn materialize(&self, id: usize) -> Vec<u8> {
        self.inner.get_data_vector(id).to_vec()
    }
    pub fn encode(&mut self, id: usize) -> Vec<u8> {
        let data = self.generate(id);
        self.materialize(data)
    }
    pub fn outputs(&mut self, ids: &[usize], stages: &mut Stages) -> Vec<Vec<u8>> {
        let data: Vec<_> = ids.iter().map(|&id| self.generate(id)).collect();
        stages.mark("symbol-generation");
        let outputs = data.into_iter().map(|id| self.materialize(id)).collect();
        stages.mark("output-materialization");
        outputs
    }
    pub fn diagnostics(&self) -> Value {
        json!({"source_k":self.k,"block_k":self.block_k,"padding_provider":"native test adapter writes zero symbols before construction","num_total":self.base,"combined_stages":[{"stage":"encoding-plan-generation","included_in":"encoder-context-and-plan-application","reason":"execute-only constructor combines symbolic solving and payload application; standalone symbolic planning is measured separately"}]})
    }
}
impl Decoder {
    pub fn new(codec: &str, k: usize, width: usize, probe: Option<Arc<Probe>>) -> Self {
        fn make<S: CodeScheme>(
            scheme: S,
            k: usize,
            width: usize,
            probe: Option<Arc<Probe>>,
        ) -> Decoder {
            Decoder {
                inner: fountain_engine::Decoder::new_with_operator_execute_only_and_num_source(
                    &scheme,
                    operator::boxed(width, probe),
                    k,
                ),
                k,
            }
        }
        match codec {
            "rq" => make(RaptorQSysCode::new_with_default_setting(k), k, width, probe),
            "r10" => make(
                Raptor10SysCode::new_with_default_setting(k),
                k,
                width,
                probe,
            ),
            _ => unreachable!(),
        }
    }
    pub fn add(&mut self, id: usize, bytes: &[u8]) {
        self.inner.add_coded_vector(id, bytes);
    }
    pub fn decoded(&self) -> bool {
        self.inner.decode_status() == DecodeStatus::Decoded
    }
    pub fn result(&self) -> Vec<u8> {
        (0..self.k)
            .flat_map(|id| self.inner.manager.get_data_vector(id).iter().copied())
            .collect()
    }
    pub fn diagnostics(&self) -> Value {
        #[cfg(all(feature = "engine3", feature = "diagnostics"))]
        {
            let t = self.inner.solver_phase_timing();
            let init = self.inner.init_timing();
            return json!({"scope":"inclusive upstream diagnostic counters; children overlap parents and must not be summed","initialization_ns":init.total_ms*1e6,"sparse_input_ns":(t.add_lt_bp_ms+t.add_lt_ge_ms)*1e6,"bp_inactivation_ns":t.bp_inactivation_ms*1e6,"phase_change_ns":t.phase_change_ms*1e6,"back_substitution_ns":t.back_substitution_ms*1e6,"lu_decomposition_ns":t.lu.lu_decomposition_ms*1e6,"lu_solve_ns":t.lu.lu_solve_ms*1e6,"lu_decomposition_calls":t.lu.lu_decomposition_calls});
        }
        #[cfg(not(all(feature = "engine3", feature = "diagnostics")))]
        {
            json!({"status":"unavailable","reason":"This integration has no enabled internal profiling API","included_in":"ingestion-and-solve"})
        }
    }
}
impl Plan {
    pub fn new(codec: &str, k: usize) -> Self {
        fn make<S: CodeScheme>(scheme: S, k: usize) -> Plan {
            let params = scheme.get_params();
            let mut inner = fountain_engine::Encoder::new_with_num_source(&scheme, k);
            let operations = inner.manager.move_new_operations();
            let field = inner
                .manager
                .gf256()
                .map(|gf| gf.primitive_polynomial())
                .unwrap_or(fountain_engine::types::GF2_FIELD_POLY);
            inner.manager.execute_only();
            Plan {
                inner: Encoder {
                    inner,
                    base: params.num_total(),
                    k,
                    block_k: params.k,
                },
                operations,
                block_k: params.k,
                field,
            }
        }
        match codec {
            "rq" => make(RaptorQSysCode::new_with_default_setting(k), k),
            "r10" => make(Raptor10SysCode::new_with_default_setting(k), k),
            _ => unreachable!(),
        }
    }
    pub fn export(&self) -> Vec<Operation> {
        self.operations.clone()
    }
    pub fn load(
        &mut self,
        width: usize,
        source: &[u8],
        probe: Option<Arc<Probe>>,
        stages: &mut Stages,
    ) {
        assert_eq!(source.len(), self.inner.k * width);
        let mut op = operator::boxed(width, probe);
        op.config_finite_field(self.field);
        stages.mark("storage-and-field-setup");
        for (i, bytes) in source.chunks_exact(width).enumerate() {
            op.insert_vector(bytes, i);
        }
        for i in self.inner.k..self.block_k {
            op.insert_vector(&vec![0; width], i);
        }
        stages.mark("source-loading-and-padding");
        for action in &self.operations {
            op.execute(action);
        }
        self.inner.inner.manager.set_operator(op);
        stages.mark("plan-application-and-binding");
    }
    pub fn repair_base(&self) -> usize {
        self.inner.base
    }
    pub fn encode(&mut self, id: usize) -> Vec<u8> {
        self.inner.encode(id)
    }
    pub fn diagnostics(&self) -> Value {
        let mut histogram = std::collections::BTreeMap::new();
        for op in &self.operations {
            *histogram.entry(operator::name(op)).or_insert(0usize) += 1;
        }
        let program = format!("{:?}", self.operations);
        json!({"path":"composition of the public recorded-operation API","source_k":self.inner.k,"block_k":self.block_k,"field":self.field,"operations":self.operations.len(),"operation_histogram":histogram,"program_digest":crate::digest(program.as_bytes()),"operation_vector_capacity_bytes":self.operations.capacity()*std::mem::size_of::<Operation>(),"size_scope":"Operation enum storage only; nested operand vectors excluded"})
    }
}
#[derive(Clone, Debug)]
enum Event {
    Write(usize, Vec<u8>),
    Field(u16),
    Op(Operation, bool),
    Alias(usize, usize),
}
struct Recorder {
    inner: operator::Selected,
    log: Arc<Mutex<Vec<Event>>>,
}
impl Recorder {
    fn direct(&mut self, op: Operation) {
        self.log.lock().unwrap().push(Event::Op(op.clone(), true));
        operator::dispatch(&mut self.inner, &op);
    }
}
#[allow(deprecated)]
impl DataOperator for Recorder {
    fn insert_vector(&mut self, v: &[u8], id: usize) {
        self.log.lock().unwrap().push(Event::Write(id, v.to_vec()));
        self.inner.insert_vector(v, id);
    }
    fn get_vector(&self, id: usize) -> &[u8] {
        self.inner.get_vector(id)
    }
    fn config_finite_field(&mut self, p: u16) {
        self.log.lock().unwrap().push(Event::Field(p));
        self.inner.config_finite_field(p);
    }
    fn execute(&mut self, op: &Operation) {
        self.log.lock().unwrap().push(Event::Op(op.clone(), false));
        self.inner.execute(op);
    }
    fn ensure_zero(&mut self, list_id: &[usize]) {
        self.direct(Operation::EnsureZero {
            list_id: list_id.to_vec(),
        });
    }
    fn ensure_zero_one(&mut self, id: usize) {
        self.direct(Operation::EnsureZeroOne { id });
    }
    fn ensure_zero_range(&mut self, range: std::ops::Range<usize>) {
        self.direct(Operation::EnsureZeroRange {
            start: range.start,
            end: range.end,
        });
    }
    fn multiply_alpha(&mut self, id: usize) {
        self.direct(Operation::MultiplyAlpha { id });
    }
    fn multiply_scalar(&mut self, scalar: u8, id: usize) {
        self.direct(Operation::MultiplyScalar { scalar, id });
    }
    fn add_one_to_vector(&mut self, src_id: usize, target_id: usize) {
        self.direct(Operation::AddOneToVector { src_id, target_id });
    }
    fn add_two_to_vector(&mut self, s0: usize, s1: usize, target_id: usize) {
        self.direct(Operation::AddTwoToVector { s0, s1, target_id });
    }
    fn add_three_to_vector(&mut self, s0: usize, s1: usize, s2: usize, target_id: usize) {
        self.direct(Operation::AddThreeToVector {
            s0,
            s1,
            s2,
            target_id,
        });
    }
    fn add_to_vector(&mut self, list_id: &[usize], target_id: usize) {
        self.direct(Operation::AddToVector {
            list_id: list_id.to_vec(),
            target_id,
        });
    }
    fn broadcast_add(&mut self, src_id: usize, target_ids: &[usize]) {
        self.direct(Operation::BroadcastAdd {
            src_id,
            target_ids: target_ids.to_vec(),
        });
    }
    fn mul_add(&mut self, src_id: usize, scalar: u8, target_id: usize) {
        self.direct(Operation::MulAdd {
            src_id,
            scalar,
            target_id,
        });
    }
    fn copy_to(&mut self, src_id: usize, target_id: usize) {
        self.direct(Operation::CopyTo { src_id, target_id });
    }
    fn copy_to_n(&mut self, src_start: usize, dst_start: usize, count: usize) {
        self.direct(Operation::CopyToN {
            src_start,
            dst_start,
            count,
        });
    }
    fn move_to(&mut self, src_id: usize, target_id: usize) {
        self.direct(Operation::MoveTo { src_id, target_id });
    }
    fn move_to_n(&mut self, src_start: usize, dst_start: usize, count: usize) {
        self.direct(Operation::MoveToN {
            src_start,
            dst_start,
            count,
        });
    }
    fn remove(&mut self, id: usize) {
        self.direct(Operation::Remove { id });
    }
    fn info_coded_vector(&mut self, coded_id: usize, data_id: usize) {
        self.direct(Operation::InfoCodedVector { coded_id, data_id });
    }
    #[cfg(feature = "engine3")]
    fn alias_to(&mut self, src: usize, dst: usize) -> bool {
        let accepted = self.inner.alias_to(src, dst);
        if accepted {
            self.log.lock().unwrap().push(Event::Alias(src, dst));
        }
        accepted
    }
}
pub struct Replay {
    events: Vec<Event>,
    checkpoints: Vec<(usize, usize, Vec<u8>)>,
    details: Value,
    width: usize,
}
impl Replay {
    pub fn capture(
        codec: &str,
        k: usize,
        width: usize,
        source: &[u8],
        packets: &[(usize, Vec<u8>)],
        decode: bool,
    ) -> Self {
        fn make<S: CodeScheme>(
            scheme: S,
            k: usize,
            width: usize,
            source: &[u8],
            packets: &[(usize, Vec<u8>)],
            decode: bool,
        ) -> Replay {
            let log = Arc::new(Mutex::new(Vec::new()));
            let mut op = Box::new(Recorder {
                inner: operator::raw(width),
                log: log.clone(),
            });
            let mut checkpoints = Vec::new();
            let mut consumed = 0;
            let mut decoded = None;
            if decode {
                let mut decoder =
                    fountain_engine::Decoder::new_with_operator_execute_only_and_num_source(
                        &scheme, op, k,
                    );
                for (id, v) in packets {
                    decoder.add_coded_vector(*id, v);
                    consumed += 1;
                    if decoder.decode_status() == DecodeStatus::Decoded {
                        break;
                    }
                }
                let completed = decoder.decode_status() == DecodeStatus::Decoded;
                decoded = Some(completed);
                if completed {
                    let event_count = log.lock().unwrap().len();
                    for (id, expected) in source.chunks_exact(width).enumerate() {
                        let actual = decoder.manager.get_data_vector(id);
                        assert!(actual == expected, "recorded decode output symbol {id}");
                        checkpoints.push((event_count, id, expected.to_vec()));
                    }
                }
            } else {
                for (i, v) in source.chunks_exact(width).enumerate() {
                    op.insert_vector(v, i);
                }
                for i in k..scheme.get_params().k {
                    op.insert_vector(&vec![0; width], i);
                }
                let mut encoder =
                    fountain_engine::Encoder::new_with_operator_execute_only_and_num_source(
                        &scheme, op, k,
                    );
                let base = scheme.get_params().num_total();
                for id in base..base + 8 {
                    let d = encoder.encode_coded_vector(id).unwrap();
                    let actual = encoder.get_data_vector(d);
                    assert_eq!(actual.len(), width, "recorded encoder symbol {id} width");
                    checkpoints.push((log.lock().unwrap().len(), d, actual.to_vec()));
                }
            }
            let events = Arc::try_unwrap(log).unwrap().into_inner().unwrap();
            let mut histogram = std::collections::BTreeMap::new();
            let mut dispatch = std::collections::BTreeMap::new();
            let mut fields = Vec::new();
            for event in &events {
                let name = match event {
                    Event::Write(..) => "input-write",
                    Event::Field(p) => {
                        fields.push(*p);
                        "field"
                    }
                    Event::Op(a, direct) => {
                        *dispatch
                            .entry(if *direct { "direct" } else { "execute" })
                            .or_insert(0usize) += 1;
                        operator::name(a)
                    }
                    Event::Alias(..) => "alias",
                };
                *histogram.entry(name).or_insert(0usize) += 1;
            }
            let details = json!({
                "sequence_digest":crate::digest(format!("{events:?}").as_bytes()),
                "calls":events.len(),"histogram":histogram,"dispatch":dispatch,"fields":fields,
                "consumed":consumed,"capture_decoded":decoded,"verified_output_symbols":checkpoints.len(),
                "scope":"ordered imports, field configuration and recorded operations; excludes solver, capture, and output checks"
            });
            Replay {
                events,
                checkpoints,
                details,
                width,
            }
        }
        match codec {
            "rq" => make(
                RaptorQSysCode::new_with_default_setting(k),
                k,
                width,
                source,
                packets,
                decode,
            ),
            "r10" => make(
                Raptor10SysCode::new_with_default_setting(k),
                k,
                width,
                source,
                packets,
                decode,
            ),
            _ => unreachable!(),
        }
    }
    fn apply(op: &mut operator::Selected, event: &Event) {
        match event {
            Event::Write(id, v) => op.insert_vector(v, *id),
            Event::Field(p) => op.config_finite_field(*p),
            Event::Op(a, direct) => {
                if *direct {
                    operator::dispatch(op, a);
                } else {
                    op.execute(a);
                }
            }
            Event::Alias(src, dst) => {
                #[cfg(feature = "engine3")]
                assert!(op.alias_to(*src, *dst));
                #[cfg(not(feature = "engine3"))]
                {
                    let _ = (src, dst);
                    unreachable!("alias event on non-alias interface");
                }
            }
        }
    }

    pub fn execute(&self) -> (u64, Value, Value) {
        let mut op = operator::raw(self.width);
        let resources_start = crate::telemetry::start();
        let start = std::time::Instant::now();
        for event in &self.events {
            Self::apply(&mut op, event);
        }
        let elapsed = crate::elapsed(start);
        let resources = crate::telemetry::finish(resources_start);
        let check = |op: &operator::Selected, id: usize, expected: &[u8]| {
            let actual = op.get_vector(id);
            assert!(
                actual == expected,
                "replay output at recorded boundary, symbol {id}"
            );
        };
        if self
            .checkpoints
            .iter()
            .all(|(at, _, _)| *at == self.events.len())
        {
            for (_, id, expected) in &self.checkpoints {
                check(&op, *id, expected);
            }
        } else {
            // Earlier outputs can have reused IDs by the end of the program.
            let mut verifier = operator::raw(self.width);
            let mut cursor = 0;
            for (at, id, expected) in &self.checkpoints {
                for event in &self.events[cursor..*at] {
                    Self::apply(&mut verifier, event);
                }
                check(&verifier, *id, expected);
                cursor = *at;
            }
        }
        (elapsed, self.details.clone(), resources)
    }
}

pub fn error_contract(_codec: &str, _k: usize, _width: usize, _source: &[u8]) -> Value {
    json!({"status":"not_observable","reason":"Native trait methods do not expose a recoverable invalid-input error contract; intentionally invalid IDs or vector lengths are not submitted","covered":"normal destruction, ownership transfer, byte-verified repeated instances"})
}
