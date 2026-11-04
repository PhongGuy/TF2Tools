use std::path::Path;

use chrono::{TimeZone as _, Utc};
use tf2tools_app::crash::{
    CRASH_FILE, CrashReport, MAX_REPORT_BYTES, PREVIOUS_FILE, archive_crash, crash_dir, panic_text,
    read_crash, redact_home, system, take_crash, write_crash,
};

fn report() -> CrashReport {
    CrashReport {
        version: "6.0.0".into(),
        time: Utc.with_ymd_and_hms(2026, 10, 19, 12, 30, 5).unwrap(),
        system: "linux x86_64".into(),
        thread: Some("main".into()),
        message: "index out of bounds".into(),
        location: Some("crates/tf2tools-app/src/pages/huds.rs:12:5".into()),
        backtrace: "   0: tf2tools_app::pages::huds::render\n".into(),
        recent: vec!["2026-10-19T12:30:00Z WARN tf2tools_core: disk is full".into()],
    }
}

#[test]
fn a_report_reads_top_to_bottom() {
    assert_eq!(
        report().render(None),
        "TF2Tools 6.0.0 closed unexpectedly.\n\
         Time: 2026-10-19T12:30:05Z\n\
         System: linux x86_64\n\
         Thread: main\n\
         Message: index out of bounds\n\
         Location: crates/tf2tools-app/src/pages/huds.rs:12:5\n\
         \n\
         Backtrace:\n   0: tf2tools_app::pages::huds::render\n\
         \n\
         Recent warnings and errors:\n\
         2026-10-19T12:30:00Z WARN tf2tools_core: disk is full\n"
    );
}

#[test]
fn missing_parts_read_as_plain_placeholders() {
    let bare = CrashReport {
        thread: None,
        location: None,
        backtrace: String::new(),
        recent: Vec::new(),
        ..report()
    };
    let text = bare.render(None);
    assert!(text.contains("\nThread: unnamed\n"), "{text}");
    assert!(text.contains("\nLocation: unknown\n"), "{text}");
    assert!(text.contains("\nBacktrace:\nNone\n"), "{text}");
    assert!(
        text.ends_with("\nRecent warnings and errors:\nNone\n"),
        "{text}"
    );
    assert_eq!(
        system(),
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
    );
}

#[test]
fn the_home_folder_becomes_a_tilde() {
    let home = Path::new("/home/sniper");
    assert_eq!(
        redact_home(
            "at /home/sniper/.local/share/x and /home/sniper",
            Some(home)
        ),
        "at ~/.local/share/x and ~"
    );
    assert_eq!(redact_home("/home/sniper/x", None), "/home/sniper/x");
    assert_eq!(redact_home("/a/b", Some(Path::new("/"))), "/a/b");
    let windows = Path::new(r"C:\Users\Sniper");
    assert_eq!(
        redact_home(
            r"C:\Users\Sniper\AppData and C:/Users/Sniper/x",
            Some(windows)
        ),
        r"~\AppData and ~/x"
    );
    let leaky = CrashReport {
        message: "cannot open /home/sniper/tf/custom/hud".into(),
        ..report()
    };
    let text = leaky.render(Some(home));
    assert!(
        text.contains("Message: cannot open ~/tf/custom/hud\n"),
        "{text}"
    );
    assert!(!text.contains("sniper"), "{text}");
}

#[test]
fn panic_payloads_read_as_text() {
    assert_eq!(panic_text(&"static boom"), "static boom");
    assert_eq!(panic_text(&String::from("owned boom")), "owned boom");
    assert_eq!(panic_text(&42_u32), "panic");
}

#[test]
fn a_crash_file_is_written_whole_and_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = crash_dir(tmp.path());
    assert_eq!(dir, tmp.path().join("crash"));
    write_crash(&dir, "first\n").unwrap();
    write_crash(&dir, "second\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join(CRASH_FILE)).unwrap(),
        "second\n"
    );
    let names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, vec![CRASH_FILE.to_owned()], "no temp file is left");
}

#[test]
fn a_huge_report_is_cut_short_on_a_char_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = crash_dir(tmp.path());
    write_crash(&dir, &"é".repeat(MAX_REPORT_BYTES)).unwrap();
    let bytes = std::fs::read(dir.join(CRASH_FILE)).unwrap();
    assert!(bytes.len() <= MAX_REPORT_BYTES, "{}", bytes.len());
    let text = String::from_utf8(bytes).expect("valid UTF-8");
    assert!(text.ends_with("\n[cut short]\n"));
}

#[test]
fn a_write_that_cannot_happen_is_an_error_not_a_panic() {
    let tmp = tempfile::tempdir().unwrap();
    let blocker = tmp.path().join("crash");
    std::fs::write(&blocker, "a file where the folder should be").unwrap();
    assert!(write_crash(&blocker, "text").is_err());
}

#[test]
fn taking_a_crash_moves_it_aside_once() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = crash_dir(tmp.path());
    assert_eq!(take_crash(&dir), None, "no folder yet");
    write_crash(&dir, "Message: boom\n").unwrap();
    assert_eq!(take_crash(&dir).as_deref(), Some("Message: boom\n"));
    assert!(!dir.join(CRASH_FILE).exists());
    assert_eq!(
        std::fs::read_to_string(dir.join(PREVIOUS_FILE)).unwrap(),
        "Message: boom\n"
    );
    assert_eq!(take_crash(&dir), None, "shown once");
}

#[test]
fn a_blank_or_odd_crash_file_is_handled() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = crash_dir(tmp.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(CRASH_FILE), "  \n").unwrap();
    assert_eq!(take_crash(&dir), None);
    assert!(
        !dir.join(CRASH_FILE).exists(),
        "a blank file is moved aside too"
    );
    let mut odd = vec![0xff, b'x'];
    odd.extend(std::iter::repeat_n(b'a', MAX_REPORT_BYTES * 2));
    std::fs::write(dir.join(CRASH_FILE), odd).unwrap();
    let text = take_crash(&dir).unwrap();
    assert!(text.starts_with("\u{fffd}x"), "lossy UTF-8");
    assert!(text.len() <= MAX_REPORT_BYTES + 2, "{}", text.len());
}

#[test]
fn reading_a_crash_leaves_it_until_archived() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = crash_dir(tmp.path());
    archive_crash(&dir);
    assert!(!dir.exists(), "archiving nothing creates nothing");
    write_crash(&dir, "Message: boom\n").unwrap();
    assert_eq!(read_crash(&dir).as_deref(), Some("Message: boom\n"));
    assert_eq!(
        read_crash(&dir).as_deref(),
        Some("Message: boom\n"),
        "a read leaves the file"
    );
    archive_crash(&dir);
    assert_eq!(read_crash(&dir), None);
    assert_eq!(
        std::fs::read_to_string(dir.join(PREVIOUS_FILE)).unwrap(),
        "Message: boom\n"
    );
}

#[test]
fn a_windows_home_is_matched_in_any_case() {
    let windows = Path::new(r"C:\Users\Sniper");
    assert_eq!(
        redact_home(
            r"c:\users\sniper\AppData and C:/USERS/SNIPER/x",
            Some(windows)
        ),
        r"~\AppData and ~/x"
    );
    // A Unix home is case-sensitive, like its file system.
    assert_eq!(
        redact_home("/home/SNIPER/x", Some(Path::new("/home/sniper"))),
        "/home/SNIPER/x"
    );
}

#[test]
fn a_home_with_a_trailing_separator_is_hidden_everywhere() {
    let text = redact_home(
        "/home/sniper/x and /home/sniper end",
        Some(Path::new("/home/sniper/")),
    );
    assert_eq!(text, "~/x and ~ end");
    assert!(!text.contains("sniper"), "{text}");
    let text = redact_home(
        r"C:\Users\Sniper\AppData and C:\Users\Sniper end and C:/Users/Sniper/x",
        Some(Path::new(r"C:\Users\Sniper\")),
    );
    assert_eq!(text, r"~\AppData and ~ end and ~/x");
    assert!(!text.to_lowercase().contains("sniper"), "{text}");
}

#[test]
fn a_windows_home_written_with_mixed_separators_is_hidden() {
    let text = redact_home(
        r"at C:\Users/Sniper\AppData and c:/users\SNIPER/x",
        Some(Path::new(r"C:\Users\Sniper")),
    );
    assert_eq!(text, r"at ~\AppData and ~/x");
    assert!(!text.to_lowercase().contains("sniper"), "{text}");
}
