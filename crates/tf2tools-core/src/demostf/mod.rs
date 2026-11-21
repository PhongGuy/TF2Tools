//! demos.tf: finding the STV demo recorded for a logs.tf log.

mod demo;

pub use demo::{DemoDetail, DemoPlayer, DemoQuery, DemoSummary, pick_demo};

/// The largest demo file a download accepts.
pub const DEMO_LIMIT: u64 = 1024 * 1024 * 1024;
