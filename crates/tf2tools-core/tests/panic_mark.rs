//! Panics that core catches itself run with the thread marked, so the app's crash hook skips them.
//! It swaps the process-wide panic hook, so it records only its own thread's panics and hands
//! the other tests' panics to the previous hook.

use std::sync::{Arc, Mutex};

use tf2tools_core::panic_mark::is_marked;
use tf2tools_core::{decode_vtf, extract_archive};

use crate::common;

#[test]
fn panics_inside_core_catch_sites_run_marked() {
    let seen: Arc<Mutex<Vec<(String, bool)>>> = Arc::default();
    let record = Arc::clone(&seen);
    let me = std::thread::current().id();
    let fallback = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() != me {
            return fallback(info);
        }
        let message = info.payload_as_str().unwrap_or("panic").to_owned();
        if let Ok(mut seen) = record.lock() {
            seen.push((message, is_marked()));
        }
    }));

    // Corrupt inputs the third-party readers are known to panic on, each behind a core guard.
    let tmp = tempfile::tempdir().unwrap();
    let full = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/crosshairs/vtf/Crossdot.vtf"),
    )
    .unwrap();
    for len in [16, 64, 80, full.len() / 2, full.len() - 1] {
        let _ = decode_vtf(&full[..len]);
    }
    let vpk = common::vpk_bytes(&[("sound/ui/hitsound.wav", b"RIFF0000WAVE")]);
    let bad_7z = [&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C][..], &[0xEE; 40][..]].concat();
    for (name, bytes) in [
        ("cut.vpk", vpk[..vpk.len() / 2].to_vec()),
        ("bad.7z", bad_7z),
    ] {
        let archive = tmp.path().join(name);
        std::fs::write(&archive, bytes).unwrap();
        let _ = extract_archive(&archive, &tmp.path().join(format!("out-{name}")));
    }

    // Back to std's hook, so a failing assert below prints its message.
    drop(std::panic::take_hook());
    let seen = seen.lock().unwrap().clone();
    assert!(
        !seen.is_empty(),
        "no corrupt input panicked; the test proves nothing"
    );
    let unmarked: Vec<_> = seen.iter().filter(|(_, marked)| !marked).collect();
    assert!(unmarked.is_empty(), "caught but unmarked: {unmarked:?}");
    assert!(!is_marked());
}
