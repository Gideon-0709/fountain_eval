//! T-owned workload definitions shared verbatim with the isolated native driver.
//! This module has no component bindings, filesystem access or environment-dependent expansion.
use serde::{Deserialize, Serialize};

pub const WORKERS: usize = 3;
pub const SAMPLES_PER_WORKER: usize = 5;
pub const REPETITIONS: usize = WORKERS * SAMPLES_PER_WORKER;
pub const SEEDS: [u64; 3] = [0x19_71, 0x20_03, 0x50_08];
pub const WIDTHS: [usize; 8] = [16, 31, 32, 33, 128, 1016, 4096, 16384];
pub const CORRECTNESS_WIDTHS: [usize; 19] = [
    1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 1016, 1023, 1024, 1025, 4096, 16384,
];
pub const SOURCE_COUNTS: [usize; 15] = [
    1, 4, 9, 10, 11, 63, 64, 65, 511, 512, 513, 5007, 5008, 5009, 5120,
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Action {
    Insert {
        id: usize,
    },
    Read {
        id: usize,
    },
    Zero {
        id: usize,
    },
    ZeroList {
        ids: Vec<usize>,
    },
    ZeroRange {
        start: usize,
        count: usize,
    },
    Alpha {
        id: usize,
    },
    Scale {
        id: usize,
        scalar: u8,
    },
    Xor {
        src: usize,
        dst: usize,
    },
    XorTwo {
        a: usize,
        b: usize,
        dst: usize,
    },
    XorThree {
        a: usize,
        b: usize,
        c: usize,
        dst: usize,
    },
    XorMany {
        sources: Vec<usize>,
        dst: usize,
    },
    Broadcast {
        src: usize,
        destinations: Vec<usize>,
    },
    MulAdd {
        src: usize,
        scalar: u8,
        dst: usize,
    },
    Copy {
        src: usize,
        dst: usize,
    },
    CopyRange {
        src: usize,
        dst: usize,
        count: usize,
    },
    Move {
        src: usize,
        dst: usize,
    },
    MoveRange {
        src: usize,
        dst: usize,
        count: usize,
    },
    Remove {
        id: usize,
    },
    Info {
        coded: usize,
        data: usize,
    },
    Alias {
        src: usize,
        dst: usize,
    },
    SharedWrite {
        src: usize,
        dst: usize,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SequenceDispatch {
    Direct,
    Execute,
    Mixed,
}
impl SequenceDispatch {
    pub fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Execute => "execute",
            Self::Mixed => "mixed",
        }
    }
    pub fn direct(self, index: usize) -> bool {
        self == Self::Direct || (self == Self::Mixed && index % 2 == 0)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SequenceSpec {
    pub scenario: String,
    pub dispatch: SequenceDispatch,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundaryContract {
    /// A requirement of this T, distinguished from an observational reference.
    pub defined: bool,
    pub expectation: String,
    pub basis: String,
    /// Only these IDs may differ for an ambiguous call. Other live bytes are protected.
    pub affected_ids: Vec<usize>,
}
pub struct Boundary {
    pub name: &'static str,
    pub action: Action,
    pub contract: BoundaryContract,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Case {
    pub id: String,
    pub category: String,
    pub family: String,
    pub workload: String,
    pub scheme: Option<String>,
    pub k: usize,
    pub width: usize,
    pub seed: u64,
    pub missing: usize,
    pub repairs: usize,
    pub order: String,
    pub blocks: usize,
    pub direct: bool,
    pub instances: usize,
    pub actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<SequenceSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary: Option<BoundaryContract>,
    pub warning: bool,
    pub measured: bool,
    pub fresh_process: bool,
    pub requires: Vec<String>,
}
impl Case {
    fn new(category: &str, family: &str, workload: &str) -> Self {
        Self {
            id: String::new(),
            category: category.into(),
            family: family.into(),
            workload: workload.into(),
            scheme: None,
            k: 64,
            width: 128,
            seed: SEEDS[0],
            missing: 0,
            repairs: 8,
            order: "source-first".into(),
            blocks: 1,
            direct: false,
            instances: 1,
            actions: Vec::new(),
            sequence: None,
            boundary: None,
            warning: false,
            measured: category != "correctness",
            fresh_process: false,
            requires: Vec::new(),
        }
    }
    fn identify(mut self, suffix: &str) -> Self {
        self.id = format!("{}/{}/{suffix}", self.category, self.workload);
        self
    }
    pub fn payload_bytes(&self) -> u64 {
        if matches!(self.category.as_str(), "workflow" | "chain")
            && !matches!(
                self.workload.as_str(),
                "plan-build" | "plan-export" | "encode-replay" | "decode-replay"
            )
        {
            (self.k * self.width * self.blocks) as u64
        } else {
            0
        }
    }
    pub fn calls(&self) -> u64 {
        if self.category == "operation" {
            self.instances as u64
        } else {
            0
        }
    }
    pub fn budget(&self) -> usize {
        128.max(2 * self.missing + 64)
    }
}

/// Stable operation semantics, including individually named arity/scalar variants.
pub fn operations() -> Vec<(String, &'static str, Action)> {
    use Action::*;
    let mut out = vec![
        ("insert-new".into(), "O01", Insert { id: 32 }),
        ("insert-existing".into(), "O01", Insert { id: 1 }),
        ("read".into(), "O02", Read { id: 1 }),
        ("zero-one".into(), "O03", Zero { id: 1 }),
        (
            "zero-list".into(),
            "O03",
            ZeroList {
                ids: vec![1, 3, 5, 7],
            },
        ),
        ("zero-range".into(), "O03", ZeroRange { start: 4, count: 4 }),
        ("xor-one".into(), "O04", Xor { src: 0, dst: 8 }),
        ("xor-two".into(), "O04", XorTwo { a: 0, b: 1, dst: 8 }),
        (
            "xor-three".into(),
            "O04",
            XorThree {
                a: 0,
                b: 1,
                c: 2,
                dst: 8,
            },
        ),
        ("multiply-alpha".into(), "O06", Alpha { id: 1 }),
        ("copy-one".into(), "O07", Copy { src: 0, dst: 8 }),
        (
            "copy-range".into(),
            "O07",
            CopyRange {
                src: 0,
                dst: 8,
                count: 4,
            },
        ),
        ("move-one".into(), "O07", Move { src: 0, dst: 8 }),
        (
            "move-range".into(),
            "O07",
            MoveRange {
                src: 0,
                dst: 8,
                count: 4,
            },
        ),
        ("alias".into(), "O08", Alias { src: 0, dst: 8 }),
        (
            "shared-first-write".into(),
            "O08",
            SharedWrite { src: 0, dst: 8 },
        ),
        ("remove".into(), "O09", Remove { id: 1 }),
        (
            "info".into(),
            "O09",
            Info {
                coded: 1024,
                data: 1,
            },
        ),
    ];
    for arity in [4, 8] {
        out.push((
            format!("xor-many-{arity}"),
            "O04",
            XorMany {
                sources: (0..arity).collect(),
                dst: 15,
            },
        ));
        out.push((
            format!("broadcast-{arity}"),
            "O05",
            Broadcast {
                src: 0,
                destinations: (1..=arity).collect(),
            },
        ));
    }
    for scalar in [0, 1, 7, 255] {
        out.push((format!("scale-{scalar}"), "O06", Scale { id: 1, scalar }));
        out.push((
            format!("mul-add-{scalar}"),
            "O06",
            MulAdd {
                src: 0,
                scalar,
                dst: 8,
            },
        ));
    }
    out
}
/// Per-case policy is T-owned and shared by correctness and performance checks.
/// Sources: DataOperator and Operation in registered engine 2.1 and engine 3.
pub fn boundaries() -> Vec<Boundary> {
    use Action::*;
    [
        ("self-xor", Xor { src: 0, dst: 0 }, false,
         "The original value XOR itself is zero.",
         "Same-ID source/target aliasing is unspecified; retained as a warning by the agreed T policy.", vec![0]),
        ("duplicate-source-xor", XorTwo { a: 0, b: 0, dst: 1 }, false,
         "Two occurrences of the same source cancel; the target is unchanged.",
         "The API does not specify duplicate-source handling; retained as a warning by the agreed T policy.", vec![1]),
        ("target-in-sources", XorTwo { a: 0, b: 1, dst: 1 }, false,
         "Read sources from the pre-call state; the target becomes the original source 0.",
         "The API does not define source snapshots when the target also occurs among sources.", vec![1]),
        ("duplicate-broadcast-target", Broadcast { src: 0, destinations: vec![1, 1] }, false,
         "Apply each list occurrence; two XOR writes leave the target unchanged.",
         "The API does not specify whether repeated broadcast destinations are deduplicated.", vec![1]),
        ("self-copy", Copy { src: 0, dst: 0 }, true,
         "The existing symbol remains readable with identical bytes.",
         "Operation::CopyTo requires keeping the source intact; coincident source and destination do not conflict with that requirement.", vec![0]),
        ("self-move", Move { src: 0, dst: 0 }, false,
         "A move to the same ID preserves the symbol.",
         "Operation::MoveTo invalidates the source; precedence when source equals destination is unspecified.", vec![0]),
        ("empty-xor", XorMany { sources: vec![], dst: 1 }, true,
         "The existing target remains readable with identical bytes.",
         "T defines the empty fold of Operation::AddToVector as zero added to the target; this is a derived arithmetic contract, not an explicit upstream empty-list promise.", vec![1]),
        ("overlapping-copy", CopyRange { src: 0, dst: 1, count: 3 }, false,
         "Snapshot original IDs 0..2 and write their bytes to IDs 1..3.",
         "Operation::CopyToN does not specify overlapping-range order or snapshot semantics.", vec![1, 2, 3]),
        ("overlapping-move", MoveRange { src: 0, dst: 1, count: 3 }, false,
         "Snapshot original IDs 0..2, remove the source range, then write IDs 1..3.",
         "Operation::MoveToN does not specify overlapping-range order; source absence is unobservable through the current safe API.", vec![0, 1, 2, 3]),
    ].into_iter().map(|(name, action, defined, expectation, basis, affected_ids)| Boundary {
        name, action, contract: BoundaryContract { defined, expectation: expectation.into(), basis: basis.into(), affected_ids }
    }).collect()
}

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for width in CORRECTNESS_WIDTHS {
        for (work, family) in [
            ("operator-reference", "V02"),
            ("storage-ownership", "V03"),
            ("dispatch-equivalence", "V05"),
        ] {
            let mut c = Case::new("correctness", family, work);
            c.width = width;
            c.requires.push("operator_surface".into());
            out.push(c.identify(&format!("b{width}")));
        }
        // Directed histories use one payload seed; generated histories vary all three.
        for scenario in ["copy-chain", "move-reuse", "sparse-churn", "seeded"] {
            let seeds: &[u64] = if scenario == "seeded" {
                &SEEDS
            } else {
                &SEEDS[..1]
            };
            for &seed in seeds {
                for dispatch in [
                    SequenceDispatch::Direct,
                    SequenceDispatch::Execute,
                    SequenceDispatch::Mixed,
                ] {
                    let mut c = Case::new("correctness", "V04", "mixed-sequence");
                    c.width = width;
                    c.seed = seed;
                    c.sequence = Some(SequenceSpec {
                        scenario: scenario.into(),
                        dispatch,
                    });
                    c.requires.push("operator_surface".into());
                    out.push(
                        c.identify(&format!("{scenario}/b{width}/s{seed}/{}", dispatch.name())),
                    );
                }
            }
        }
        for boundary in boundaries() {
            for direct in [false, true] {
                let mut c = Case::new("correctness", "V06", boundary.name);
                c.width = width;
                c.warning = !boundary.contract.defined;
                c.direct = direct;
                c.actions.push(boundary.action.clone());
                c.boundary = Some(boundary.contract.clone());
                c.requires.push("operator_surface".into());
                out.push(c.identify(&format!(
                    "b{width}/{}",
                    if direct { "direct" } else { "execute" }
                )));
            }
        }
    }
    let operation_controls = [
        (
            "xor-list-2".into(),
            "O04",
            Action::XorMany {
                sources: vec![0, 1],
                dst: 8,
            },
        ),
        (
            "xor-list-3".into(),
            "O04",
            Action::XorMany {
                sources: vec![0, 1, 2],
                dst: 8,
            },
        ),
        (
            "unshared-first-write".into(),
            "O08",
            Action::Scale { id: 8, scalar: 7 },
        ),
    ];
    for (name, family, action, boundary) in operations()
        .into_iter()
        .chain(operation_controls)
        .map(|(n, f, a)| (n, f, a, None))
        .chain(
            boundaries()
                .into_iter()
                .map(|b| (b.name.into(), "V06", b.action, Some(b.contract))),
        )
    {
        for width in WIDTHS {
            for direct in [false, true] {
                // Insertion, lookup and aliasing have no Operation-enum equivalent.
                if !direct
                    && (name == "unshared-first-write"
                        || matches!(
                            action,
                            Action::Insert { .. }
                                | Action::Read { .. }
                                | Action::Alias { .. }
                                | Action::SharedWrite { .. }
                        ))
                {
                    continue;
                }
                for (set, maximum) in [("small", 64), ("large", 512)] {
                    let mut c = Case::new("operation", family, &name);
                    c.width = width;
                    c.direct = direct;
                    c.actions.push(action.clone());
                    c.instances = maximum.min((64 * 1024 * 1024 / (width * 16)).max(1));
                    c.warning = boundary.as_ref().is_some_and(|b| !b.defined);
                    c.boundary = boundary.clone();
                    c.requires.push("operator_surface".into());
                    if family == "O08" {
                        c.requires.push("alias".into());
                    }
                    out.push(c.identify(&format!(
                        "b{width}/{}/{set}",
                        if direct { "direct" } else { "execute" }
                    )));
                }
            }
        }
    }
    for scheme in ["rq", "r10"] {
        for (work, family) in [
            ("participation", "V01"),
            ("plan-reuse-contract", "V08"),
            ("linear-reference", "V09"),
            ("receive-contract", "V10"),
            ("lifecycle-contract", "V11"),
        ] {
            for seed in SEEDS {
                let mut c = Case::new("correctness", family, work);
                c.scheme = Some(scheme.into());
                c.seed = seed;
                if work == "plan-reuse-contract" {
                    c.requires.push("reuse".into());
                }
                if work == "linear-reference" {
                    c.k = 8;
                    c.width = 31;
                }
                out.push(c.identify(&format!("{scheme}/s{seed}")));
            }
        }
        for k in SOURCE_COUNTS {
            for width in [31, 128] {
                let mut c = Case::new("correctness", "V07", "scheme-padding");
                c.scheme = Some(scheme.into());
                c.k = k;
                c.width = width;
                out.push(c.identify(&format!("{scheme}/k{k}/b{width}")));
            }
        }
        for k in [64, 512, 5008] {
            for work in ["plan-build", "plan-export"] {
                let mut c = Case::new(
                    "chain",
                    if work == "plan-build" { "L02" } else { "L03" },
                    work,
                );
                c.scheme = Some(scheme.into());
                c.k = k;
                c.width = 0;
                c.requires.push("plan".into());
                out.push(c.identify(&format!("{scheme}/k{k}")));
            }
            for width in [128, 1016] {
                let geometry = format!("{scheme}/k{k}/b{width}");
                for (work, family) in [("encode-replay", "L11"), ("decode-replay", "L12")] {
                    let mut c = Case::new("chain", family, work);
                    c.scheme = Some(scheme.into());
                    c.k = k;
                    c.width = width;
                    if work == "decode-replay" {
                        c.missing = k.div_ceil(10);
                    }
                    c.requires.push("plan".into());
                    out.push(c.identify(&geometry));
                }
                for (work, family) in [
                    ("first-encode", "W01"),
                    ("encode-sources", "W02,W08"),
                    ("round-trip", "W06"),
                    ("continuous-rebuild", "W07"),
                    ("continuous-reuse", "W03,W07"),
                ] {
                    let mut c = Case::new("workflow", family, work);
                    c.scheme = Some(scheme.into());
                    c.k = k;
                    c.width = width;
                    if work == "round-trip" {
                        c.missing = k.div_ceil(10);
                        c.order = "mixed".into();
                    }
                    c.fresh_process = work == "first-encode";
                    if work.starts_with("continuous") {
                        c.blocks = 32;
                    }
                    if work == "continuous-reuse" {
                        c.requires.push("reuse".into());
                    }
                    out.push(c.identify(&geometry));
                }
                let mut repairs = vec![1, 8, k.div_ceil(10)];
                repairs.sort_unstable();
                repairs.dedup();
                for r in repairs {
                    for (work, family) in [("encode-repairs", "W02"), ("reuse-encode", "W03")] {
                        let family = if work == "encode-repairs" && r == 8 {
                            "W02,W08,L01,L04,L05,L06,L09,L10"
                        } else {
                            family
                        };
                        let mut c = Case::new("workflow", family, work);
                        c.scheme = Some(scheme.into());
                        c.k = k;
                        c.width = width;
                        c.repairs = r;
                        if work == "reuse-encode" {
                            c.requires.push("reuse".into());
                        }
                        out.push(c.identify(&format!("{geometry}/r{r}")));
                    }
                }
                let mut omissions = vec![0, 1, k.div_ceil(100), k.div_ceil(10)];
                omissions.sort_unstable();
                omissions.dedup();
                for missing in omissions {
                    for order in ["source-first", "mixed"] {
                        for seed in SEEDS {
                            let mut c = Case::new(
                                "workflow",
                                if missing == 0 {
                                    "W04,W08"
                                } else if missing == k.div_ceil(10)
                                    && order == "source-first"
                                    && seed == SEEDS[0]
                                {
                                    "W05,W08,L01,L07,L08,L09,L10"
                                } else {
                                    "W05,W08"
                                },
                                "decode",
                            );
                            c.scheme = Some(scheme.into());
                            c.k = k;
                            c.width = width;
                            c.missing = missing;
                            c.order = order.into();
                            c.seed = seed;
                            out.push(c.identify(&format!("{geometry}/m{missing}/{order}/s{seed}")));
                        }
                    }
                }
            }
        }
    }
    out
}
