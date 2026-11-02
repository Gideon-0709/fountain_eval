//! Reproducible Fountain evaluation: resolve, build, validate, measure and compare.
//! The CLI and the execution API share this single package; native drivers are embedded.

pub mod batch;
pub mod build;
pub mod catalog;
pub mod config;
pub mod content;
pub mod coverage;
pub mod evidence;
pub mod execution;
pub mod measurement;
pub mod process;
pub mod protocol;
pub mod runner;
pub mod source;
pub mod util;
pub mod validation;

pub mod report;
