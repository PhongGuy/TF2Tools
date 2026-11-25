use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tf2tools_core::{CleanupRoots, DataDir, Freed, sweep_game_leftovers, sweep_leftovers};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const WEB_VIEW: [&str; 7] = [
    "WebKitCache",
    "CacheStorage",
    "cookies",
    "hsts-storage.sqlite",
    "localstorage",
    "mediakeys",
    "storage",
];

fn write(path: &Path, len: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![7u8; len]).unwrap();
}

fn age(path: &Path, by: Duration) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::now() - by)
        .unwrap();
}

fn gone(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_err()
}

// Every regular file under `root` with its bytes, keyed by its path relative to `root`.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let rel = e.path().strip_prefix(root).unwrap().to_path_buf();
            (rel, std::fs::read(e.path()).unwrap())
        })
        .collect()
}

// Data, local data, demo cache and temp folders under one tempdir, no AppImage.
fn roots(tmp: &Path) -> CleanupRoots {
    CleanupRoots::new(
        tmp.join("data"),
        tmp.join("local"),
        tmp.join("demos"),
        tmp.join("temp"),
    )
}

fn sweep(roots: &CleanupRoots) -> Freed {
    sweep_leftovers(roots, SystemTime::now(), DAY)
}

fn sorted(freed: &Freed) -> Vec<PathBuf> {
    let mut paths = freed.paths().to_vec();
    paths.sort();
    paths
}

#[test]
fn every_web_view_name_in_the_data_folder_goes_and_its_bytes_count() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    write(&data.join("WebKitCache/a/b/blob"), 100);
    for dir in ["CacheStorage", "localstorage", "mediakeys", "storage"] {
        write(&data.join(dir).join("f"), 10);
    }
    write(&data.join("cookies"), 5);
    write(&data.join("hsts-storage.sqlite"), 5);
    let freed = sweep(&roots(tmp.path()));
    assert_eq!(freed.bytes(), 150);
    let mut want: Vec<PathBuf> = WEB_VIEW.iter().map(|n| data.join(n)).collect();
    want.sort();
    assert_eq!(sorted(&freed), want);
    for name in WEB_VIEW {
        assert!(gone(&data.join(name)), "{name}");
    }
    assert!(data.is_dir());
}

#[test]
fn look_alikes_of_every_name_stay() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    let keep = [
        t.join("data/WebKitCache2/f"),
        t.join("data/WebKitCache.bak/f"),
        t.join("data/old-cookies"),
        t.join("data/foo.part"),
        t.join("data/library/huds/a/.tf2tools/thumbnail.webp"),
        t.join("demos/.7.dem.AbC123.part.bak"),
        t.join("demos/.x.dem.AbC123.part"),
        t.join("demos/7.dem.AbC123.part"),
        t.join("demos/.7.dem.AbC12.part"),
        t.join("demos/.7.dem.AbC12-.part"),
        t.join("temp/tf2tools-update-6.0.3-setup.exe.bak"),
        t.join("temp/tf2tools-update-latest-setup.exe"),
        t.join("temp/other-setup.exe"),
        t.join("local/EBWebView2/f"),
    ];
    for path in &keep {
        write(path, 3);
        age(path, 2 * DAY);
    }
    assert_eq!(sweep(&roots(t)), Freed::default());
    for path in &keep {
        assert!(path.exists(), "{}", path.display());
    }
}

#[cfg(unix)]
#[test]
fn a_linked_web_view_folder_loses_the_link_and_keeps_its_target() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let outside = tmp.path().join("outside");
    write(&outside.join("precious"), 50);
    std::fs::create_dir_all(data.join("CacheStorage")).unwrap();
    std::os::unix::fs::symlink(&outside, data.join("WebKitCache")).unwrap();
    // A link inside a removed folder is unlinked too, never followed.
    std::os::unix::fs::symlink(&outside, data.join("CacheStorage/inner")).unwrap();
    let freed = sweep(&roots(tmp.path()));
    assert!(gone(&data.join("WebKitCache")));
    assert!(gone(&data.join("CacheStorage")));
    assert_eq!(
        std::fs::read(outside.join("precious")).unwrap(),
        vec![7u8; 50]
    );
    assert_eq!(
        sorted(&freed),
        [data.join("CacheStorage"), data.join("WebKitCache")]
    );
    assert_eq!(freed.bytes(), 0);
}

#[test]
fn the_database_and_everything_else_in_the_data_folder_stay_byte_identical() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    for (rel, len) in [
        ("tf2tools.db", 64),
        ("tf2tools.db-wal", 8),
        ("tf2tools.db-shm", 8),
        ("library/huds/budhud/info.vdf", 12),
        ("library/huds/budhud/.tf2tools/thumbnail.jpg", 9),
        ("library/sounds/s/hit.wav", 30),
        ("hud_tree_cache.json", 4),
        ("mastercomfig/9.9.9/modules.json", 7),
        ("hud_presets/p.json", 5),
        ("tmp/WebKitCache/f", 6),
        ("tmp/.tf2tools-AbC123.part", 2),
        ("crash/crash.txt", 3),
    ] {
        write(&data.join(rel), len);
    }
    age(&data.join("tmp/.tf2tools-AbC123.part"), 2 * DAY);
    write(&data.join("WebKitCache/f"), 10);
    let mut want = snapshot(&data);
    want.remove(Path::new("WebKitCache/f"));
    let freed = sweep(&roots(tmp.path()));
    assert_eq!(freed.paths(), [data.join("WebKitCache")]);
    assert_eq!(snapshot(&data), want);
}

#[test]
fn missing_roots_free_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let nowhere = tmp.path().join("nowhere");
    let roots = roots(&nowhere).with_appimage(nowhere.join("TF2Tools.AppImage"));
    assert_eq!(sweep(&roots), Freed::default());
    assert!(!nowhere.exists());
}

#[test]
fn the_edge_web_view_folder_goes_and_then_its_emptied_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let local = tmp.path().join("local");
    write(&local.join("EBWebView/Default/Cache/data_0"), 40);
    let freed = sweep(&roots(tmp.path()));
    assert_eq!(freed.bytes(), 40);
    assert_eq!(sorted(&freed), [local.clone(), local.join("EBWebView")]);
    assert!(gone(&local));
}

#[test]
fn a_local_folder_with_anything_else_in_it_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let local = tmp.path().join("local");
    write(&local.join("EBWebView/f"), 4);
    write(&local.join("keep.txt"), 1);
    let freed = sweep(&roots(tmp.path()));
    assert_eq!(freed.paths(), [local.join("EBWebView")]);
    assert!(local.join("keep.txt").exists());
}

#[test]
fn a_local_folder_that_holds_the_data_folder_is_never_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    write(&data.join("EBWebView/f"), 4);
    let same = CleanupRoots::new(
        data.clone(),
        data.clone(),
        tmp.path().join("demos"),
        tmp.path().join("temp"),
    );
    let freed = sweep(&same);
    assert_eq!(freed.paths(), [data.join("EBWebView")]);
    assert!(data.is_dir());
}

#[test]
fn roots_for_a_data_dir_that_is_not_the_platform_one_stay_inside_it() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::at(tmp.path().join("com.tf2tools.app"));
    let roots = CleanupRoots::for_data_dir(&data);
    assert_eq!(roots.data(), data.root());
    assert_eq!(roots.local_data(), data.root());
    assert_eq!(roots.temp(), data.root());
    assert_eq!(roots.demo_cache(), data.demo_cache_dir());
    assert_eq!(roots.appimage(), None);
    let freed = Freed::new(3, vec![data.root().join("cookies")]);
    assert_eq!((freed.bytes(), freed.paths().len()), (3, 1));
}

#[test]
fn a_missing_game_folder_frees_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let tf = tmp.path().join("nowhere/tf");
    assert_eq!(
        sweep_game_leftovers(&tf, SystemTime::now(), DAY),
        Freed::default()
    );
}
