//! Safe product facade binding; callbacks delegate to the selected real operator.
use crate::Stages;
use crate::{Probe, hit, operator};
use fountain_engine::traits::DataOperator;
use fountain_raptor_10 as r10;
use fountain_raptor_q as rq;
use serde_json::{Value, json};
use std::sync::Arc;
struct Symbols {
    op: operator::Selected,
    width: usize,
    probe: Option<Arc<Probe>>,
    fail_writes: bool,
}
impl Symbols {
    fn new(width: usize, probe: Option<Arc<Probe>>) -> Self {
        Self {
            op: operator::raw(width),
            width,
            probe,
            fail_writes: false,
        }
    }
}
impl Drop for Symbols {
    fn drop(&mut self) {
        if let Some(p) = &self.probe {
            hit(&p.drops);
        }
    }
}
macro_rules! callbacks {
    ($api:ident) => {
        impl $api::SymbolOps for Symbols {
            fn write(&mut self, id: usize, bytes: &[u8]) -> Result<(), $api::Error> {
                if self.fail_writes || bytes.len() != self.width {
                    return Err($api::Error::InvalidArg);
                }
                if let Some(p) = &self.probe {
                    hit(&p.writes);
                }
                self.op.insert_vector(bytes, id);
                Ok(())
            }
            fn read(&mut self, id: usize, out: &mut [u8]) -> Result<(), $api::Error> {
                if let Some(p) = &self.probe {
                    hit(&p.reads);
                }
                let bytes = self.op.get_vector(id);
                if bytes.len() != out.len() {
                    return Err($api::Error::InvalidArg);
                }
                out.copy_from_slice(bytes);
                Ok(())
            }
            fn xor_into(&mut self, dst: usize, src: usize) -> Result<(), $api::Error> {
                if let Some(p) = &self.probe {
                    hit(&p.arithmetic);
                }
                self.op.add_one_to_vector(src, dst);
                Ok(())
            }
            fn copy(&mut self, dst: usize, src: usize) -> Result<(), $api::Error> {
                if let Some(p) = &self.probe {
                    hit(&p.arithmetic);
                }
                self.op.copy_to(src, dst);
                Ok(())
            }
        }
    };
}
callbacks!(rq);
callbacks!(r10);
pub enum Encoder {
    Rq(rq::Encoder),
    R10(r10::Encoder),
}
pub enum Decoder {
    Rq(rq::Decoder),
    R10(r10::Decoder),
}
impl Encoder {
    pub fn new_staged(
        codec: &str,
        k: usize,
        width: usize,
        source: &[u8],
        probe: Option<Arc<Probe>>,
        stages: &mut Stages,
    ) -> Self {
        let value = Self::new(codec, k, width, source, probe);
        stages.mark("product-construction-loading-and-plan-application");
        value
    }
    pub fn outputs(&mut self, ids: &[usize], stages: &mut Stages) -> Vec<Vec<u8>> {
        let result = ids.iter().map(|&id| self.encode(id)).collect();
        stages.mark("product-generation-and-output");
        result
    }
    pub fn diagnostics(&self) -> Value {
        json!({"combined_stages":[{"stage":"plan-generation,source-loading,plan-application","included_in":"product-construction-loading-and-plan-application"},{"stage":"symbol-generation,output-materialization","included_in":"product-generation-and-output"}],"reason":"Product public calls do not expose these internal boundaries"})
    }
    pub fn new(
        codec: &str,
        k: usize,
        width: usize,
        source: &[u8],
        probe: Option<Arc<Probe>>,
    ) -> Self {
        match codec {
            "rq" => Self::Rq(
                rq::Encoder::with_ops(
                    &rq::ObjectTransmissionInformation::new(k, width),
                    Symbols::new(width, probe),
                    source,
                )
                .expect("product encoder"),
            ),
            "r10" => Self::R10(
                r10::Encoder::with_ops(
                    &r10::ObjectTransmissionInformation::new(k, width),
                    Symbols::new(width, probe),
                    source,
                )
                .expect("product encoder"),
            ),
            _ => panic!("unsupported codec"),
        }
    }
    pub fn repair_base(&self) -> usize {
        match self {
            Self::Rq(e) => e.num_total(),
            Self::R10(e) => e.num_total(),
        }
    }
    pub fn encode(&mut self, id: usize) -> Vec<u8> {
        match self {
            Self::Rq(e) => e.encode(id as u32).expect("encode"),
            Self::R10(e) => e.encode(id as u32).expect("encode"),
        }
    }
}
impl Decoder {
    pub fn diagnostics(&self) -> Value {
        json!({"status":"unavailable","reason":"No product solver profiling API","included_in":"ingestion-and-solve"})
    }
    pub fn new(codec: &str, k: usize, width: usize, probe: Option<Arc<Probe>>) -> Self {
        match codec {
            "rq" => Self::Rq(
                rq::Decoder::with_ops(
                    &rq::ObjectTransmissionInformation::new(k, width),
                    Symbols::new(width, probe),
                )
                .expect("product decoder"),
            ),
            "r10" => Self::R10(
                r10::Decoder::with_ops(
                    &r10::ObjectTransmissionInformation::new(k, width),
                    Symbols::new(width, probe),
                )
                .expect("product decoder"),
            ),
            _ => panic!("unsupported codec"),
        }
    }
    pub fn add(&mut self, id: usize, bytes: &[u8]) {
        match self {
            Self::Rq(d) => {
                d.add_symbol(id as u32, bytes).expect("decode");
            }
            Self::R10(d) => {
                d.add_symbol(id as u32, bytes).expect("decode");
            }
        }
    }
    pub fn decoded(&self) -> bool {
        match self {
            Self::Rq(d) => d.decoded(),
            Self::R10(d) => d.decoded(),
        }
    }
    pub fn result(&self) -> Vec<u8> {
        match self {
            Self::Rq(d) => d.get_result().expect("result"),
            Self::R10(d) => d.get_result().expect("result"),
        }
    }
}
pub struct Plan {
    codec: String,
    k: usize,
    encoder: Option<Encoder>,
}
impl Plan {
    pub fn new(codec: &str, k: usize) -> Self {
        Self {
            codec: codec.into(),
            k,
            encoder: None,
        }
    }
    pub fn load(
        &mut self,
        width: usize,
        source: &[u8],
        probe: Option<Arc<Probe>>,
        stages: &mut Stages,
    ) {
        if let Some(e) = &mut self.encoder {
            match e {
                Encoder::Rq(e) => e.set_source(source).expect("set_source"),
                Encoder::R10(e) => e.set_source(source).expect("set_source"),
            }
        } else {
            self.encoder = Some(Encoder::new(&self.codec, self.k, width, source, probe));
        }
        stages.mark("product-source-replacement-and-plan-application");
    }
    pub fn encode(&mut self, id: usize) -> Vec<u8> {
        self.encoder.as_mut().unwrap().encode(id)
    }
    pub fn repair_base(&self) -> usize {
        self.encoder.as_ref().unwrap().repair_base()
    }
    pub fn diagnostics(&self) -> Value {
        json!({"path":"same product encoder updated through set_source; internal plan reconstruction is unobservable","operations":"unavailable","plan_size":"unavailable","combined_stages":["loading and plan application are one public call"]})
    }
    pub fn export(&self) -> Vec<()> {
        panic!("product plan export is not an available capability")
    }
}
pub struct Replay;
impl Replay {
    pub fn capture(_: &str, _: usize, _: usize, _: &[u8], _: &[(usize, Vec<u8>)], _: bool) -> Self {
        panic!("product replay is not an available capability")
    }
    pub fn execute(&self) -> (u64, Value, Value) {
        unreachable!()
    }
}

pub fn error_contract(codec: &str, k: usize, width: usize, source: &[u8]) -> Value {
    macro_rules! check {
        ($api:ident) => {{
            let oti = $api::ObjectTransmissionInformation::new(k, width);
            let probe = Arc::new(Probe::default());
            let mut failing = Symbols::new(width, Some(probe.clone()));
            failing.fail_writes = true;
            assert!(
                $api::Encoder::with_ops(&oti, failing, source).is_err(),
                "callback error was swallowed"
            );
            assert_eq!(
                probe.snapshot()["drops"],
                1,
                "failed product constructor did not release callbacks"
            );
            let mut encoder =
                $api::Encoder::with_ops(&oti, Symbols::new(width, None), source).unwrap();
            assert_eq!(
                encoder.set_source(&source[..source.len() - 1]),
                Err($api::Error::InvalidArg)
            );
            let next: Vec<_> = source.iter().map(|byte| byte ^ 0xff).collect();
            encoder
                .set_source(&next)
                .expect("valid replacement after rejected input");
            assert_eq!(encoder.encode(0).unwrap(), next[..width]);
            let mut decoder = $api::Decoder::with_ops(&oti, Symbols::new(width, None)).unwrap();
            assert_eq!(
                decoder.add_symbol(0, &source[..width - 1]),
                Err($api::Error::InvalidArg)
            );
            assert!(!decoder.decoded(), "invalid symbol advanced completion");
            for (id, block) in source.chunks_exact(width).enumerate() {
                decoder
                    .add_symbol(id as u32, block)
                    .expect("valid source after rejected input");
            }
            assert!(
                decoder.decoded(),
                "valid source stream did not complete after rejection"
            );
            assert!(
                decoder.get_result().unwrap() == source,
                "receive recovery after rejection"
            );
        }};
    }
    if codec == "r10" {
        assert!(
            r10::Decoder::with_ops(
                &r10::ObjectTransmissionInformation::new(1, width),
                Symbols::new(width, None)
            )
            .is_err(),
            "R10 below-domain source count accepted"
        );
    }
    match codec {
        "rq" => check!(rq),
        "r10" => check!(r10),
        _ => unreachable!(),
    };
    json!({"status":"passed","checked":["callback failure propagation","failed constructor releases callback","invalid set_source length","invalid decoder symbol width","successful replacement and reception after invalid input"]})
}
