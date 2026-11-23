#![cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux;

pub use linux::pointer_lock;
pub use linux::{current_platform, linux_platform};
