//! Diagnostic-only allocator accounting. The ordinary binary has no wrapper allocator.
use serde_json::{Value, json};
#[cfg(feature = "diagnostics")]
mod allocation {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicU64, Ordering},
    };
    pub struct Allocator;
    pub static CALLS: AtomicU64 = AtomicU64::new(0);
    pub static BYTES: AtomicU64 = AtomicU64::new(0);
    pub static LIVE: AtomicU64 = AtomicU64::new(0);
    pub static PEAK: AtomicU64 = AtomicU64::new(0);
    fn add(n: usize) {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(n as u64, Ordering::Relaxed);
        let v = LIVE.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
        PEAK.fetch_max(v, Ordering::Relaxed);
    }
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            let p = unsafe { System.alloc(l) };
            if !p.is_null() {
                add(l.size());
            }
            p
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            let p = unsafe { System.alloc_zeroed(l) };
            if !p.is_null() {
                add(l.size());
            }
            p
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            LIVE.fetch_sub(l.size() as u64, Ordering::Relaxed);
            unsafe { System.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            let q = unsafe { System.realloc(p, l, n) };
            if !q.is_null() {
                LIVE.fetch_sub(l.size() as u64, Ordering::Relaxed);
                add(n);
            }
            q
        }
    }
}
#[cfg(feature = "diagnostics")]
#[global_allocator]
static ALLOCATOR: allocation::Allocator = allocation::Allocator;
#[derive(Clone, Copy)]
pub struct Snapshot {
    calls: u64,
    bytes: u64,
    live: u64,
}
pub fn start() -> Snapshot {
    #[cfg(feature = "diagnostics")]
    {
        use allocation::*;
        use std::sync::atomic::Ordering::Relaxed;
        let live = LIVE.load(Relaxed);
        PEAK.store(live, Relaxed);
        Snapshot {
            calls: CALLS.load(Relaxed),
            bytes: BYTES.load(Relaxed),
            live,
        }
    }
    #[cfg(not(feature = "diagnostics"))]
    {
        Snapshot {
            calls: 0,
            bytes: 0,
            live: 0,
        }
    }
}
pub fn finish(before: Snapshot) -> Value {
    #[cfg(feature = "diagnostics")]
    {
        use allocation::*;
        use std::sync::atomic::Ordering::Relaxed;
        json!({"scope":if crate::config::FAMILY=="product-abi"{"driver and callback allocator only; shared-library allocations unobserved"}else{"generated driver and native Rust dependencies"},"allocation_calls":CALLS.load(Relaxed)-before.calls,"allocated_bytes":BYTES.load(Relaxed)-before.bytes,"peak_live_growth_bytes":PEAK.load(Relaxed).saturating_sub(before.live),"live_change_bytes":LIVE.load(Relaxed) as i128-before.live as i128,"limitations":"Includes timed driver orchestration and retained application output; allocator retention is not a confirmed resource leak"})
    }
    #[cfg(not(feature = "diagnostics"))]
    {
        let _ = (before.calls, before.bytes, before.live);
        json!({"status":"not_instrumented"})
    }
}
