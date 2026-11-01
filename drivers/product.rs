//! Safe product facade binding; callbacks delegate to the selected real operator.
use crate::{Probe, hit, operator};
use fountain_engine::traits::DataOperator;
use fountain_raptor_10 as r10;
use fountain_raptor_q as rq;
use std::sync::Arc;
struct Symbols {
    op: operator::Selected,
    width: usize,
    probe: Option<Arc<Probe>>,
}
impl Symbols {
    fn new(width: usize, probe: Option<Arc<Probe>>) -> Self {
        Self {
            op: operator::raw(width),
            width,
            probe,
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
                if bytes.len() != self.width {
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
