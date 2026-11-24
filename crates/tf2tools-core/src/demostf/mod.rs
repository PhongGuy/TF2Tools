//! demos.tf: browsing its demo list, finding the demo recorded for a logs.tf log, and caching downloads.

mod cache;
mod client;
mod demo;
mod list;

pub use cache::{DEMO_CACHE_HIGH, DEMO_CACHE_LOW, DemoCache};
pub use client::{DemosClient, DemosEndpoints};
pub use demo::{DemoDetail, DemoPlayer, DemoQuery, DemoSummary, pick_demo};
pub use list::{DemoCursor, DemoFormat, DemoListQuery, DemoPage, DemosRange, merge_demos};

/// The largest demo file a download accepts.
pub const DEMO_LIMIT: u64 = 1024 * 1024 * 1024;
