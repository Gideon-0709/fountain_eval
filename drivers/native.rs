//! Source API binding, independent of the acceptance timing workflow.
use crate::{Probe, operator};
use fountain_engine::{traits::CodeScheme, types::DecodeStatus};
use fountain_scheme::rfc::{r10::Raptor10SysCode, rq::RaptorQSysCode};
use std::sync::Arc;

pub struct Encoder {
    inner: fountain_engine::Encoder,
    base: usize,
}
pub struct Decoder {
    inner: fountain_engine::Decoder,
    k: usize,
}
impl Encoder {
    pub fn new(
        codec: &str,
        k: usize,
        width: usize,
        source: &[u8],
        probe: Option<Arc<Probe>>,
    ) -> Self {
        fn make<S: CodeScheme>(
            scheme: S,
            k: usize,
            width: usize,
            source: &[u8],
            probe: Option<Arc<Probe>>,
        ) -> Encoder {
            let params = scheme.get_params();
            let mut op = operator::boxed(width, probe);
            for (i, bytes) in source.chunks_exact(width).enumerate() {
                op.insert_vector(bytes, i);
            }
            for i in k..params.k {
                op.insert_vector(&vec![0; width], i);
            }
            let inner = fountain_engine::Encoder::new_with_operator_execute_only_and_num_source(
                &scheme, op, k,
            );
            Encoder {
                inner,
                base: params.num_total(),
            }
        }
        match codec {
            "rq" => make(
                RaptorQSysCode::new_with_default_setting(k),
                k,
                width,
                source,
                probe,
            ),
            "r10" => make(
                Raptor10SysCode::new_with_default_setting(k),
                k,
                width,
                source,
                probe,
            ),
            _ => panic!("unsupported codec"),
        }
    }
    pub fn repair_base(&self) -> usize {
        self.base
    }
    pub fn encode(&mut self, id: usize) -> Vec<u8> {
        let data = self.inner.encode_coded_vector(id).expect("encode failed");
        self.inner.get_data_vector(data).to_vec()
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
            _ => panic!("unsupported codec"),
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
}
