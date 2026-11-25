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

const HOUR: Duration = Duration::from_secs(60 * 60);

#[test]
fn an_aged_demo_part_goes_and_a_fresh_one_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let demos = tmp.path().join("demos");
    let aged = demos.join(".7.dem.AbC123.part");
    let fresh = demos.join(".8.dem.Zz9yX0.part");
    let demo = demos.join("7.dem");
    write(&aged, 30);
    age(&aged, DAY + HOUR);
    write(&fresh, 20);
    age(&fresh, HOUR);
    write(&demo, 10);
    age(&demo, 3 * DAY);
    let freed = sweep(&roots(tmp.path()));
    assert_eq!(freed.paths(), std::slice::from_ref(&aged));
    assert_eq!(freed.bytes(), 30);
    assert!(fresh.exists() && demo.exists());
}

#[test]
fn aged_update_downloads_beside_the_appimage_and_in_temp_go() {
    let tmp = tempfile::tempdir().unwrap();
    let apps = tmp.path().join("apps");
    let temp = tmp.path().join("temp");
    let appimage = apps.join("TF2Tools.AppImage");
    write(&appimage, 8);
    let aged = [
        apps.join(".tf2tools-update-6.0.3"),
        apps.join(".tf2tools-update-6.0.3.part"),
        apps.join(".tf2tools-update-6.1.0-rc.1+ci.7"),
        temp.join("tf2tools-update-6.0.3-setup.exe"),
        temp.join("tf2tools-update-6.0.3-setup.exe.part"),
    ];
    for path in &aged {
        write(path, 10);
        age(path, 2 * DAY);
    }
    let fresh = apps.join(".tf2tools-update-6.0.4");
    write(&fresh, 10);
    age(&fresh, HOUR);
    let odd = [
        apps.join(".tf2tools-update-latest"),
        apps.join(".tf2tools-update-6.0.3.bak"),
        temp.join("tf2tools-update-6.0-setup.exe"),
    ];
    for path in &odd {
        write(path, 10);
        age(path, 2 * DAY);
    }
    let freed = sweep(&roots(tmp.path()).with_appimage(appimage.clone()));
    let mut want = aged.to_vec();
    want.sort();
    assert_eq!(sorted(&freed), want);
    assert_eq!(freed.bytes(), 50);
    assert!(fresh.exists() && appimage.exists());
    for path in &odd {
        assert!(path.exists(), "{}", path.display());
    }
}

#[test]
fn update_files_beside_an_unknown_appimage_stay() {
    let tmp = tempfile::tempdir().unwrap();
    let staged = tmp.path().join("apps/.tf2tools-update-6.0.3");
    write(&staged, 10);
    age(&staged, 2 * DAY);
    assert_eq!(sweep(&roots(tmp.path())), Freed::default());
    assert!(staged.exists());
}

#[test]
fn the_previous_appimage_backup_goes_once_the_new_build_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let appimage = tmp.path().join("apps/TF2Tools.AppImage");
    let bak = tmp.path().join("apps/TF2Tools.AppImage.bak");
    write(&appimage, 8);
    write(&bak, 8);
    let freed = sweep(&roots(tmp.path()).with_appimage(appimage.clone()));
    assert_eq!(freed.paths(), std::slice::from_ref(&bak));
    assert_eq!(freed.bytes(), 8);
    assert!(gone(&bak) && appimage.exists());
}

#[test]
fn a_folder_named_like_a_file_leftover_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    let appimage = t.join("apps/TF2Tools.AppImage");
    write(&appimage, 8);
    let folders = [
        t.join("apps/TF2Tools.AppImage.bak"),
        t.join("apps/.tf2tools-update-6.0.3"),
        t.join("demos/.7.dem.AbC123.part"),
        t.join("temp/tf2tools-update-6.0.3-setup.exe"),
        t.join("data/library/huds/a/.tf2tools/thumbnail.webp"),
        t.join("tf/custom/.tf2tools-AbC123.part"),
    ];
    for folder in &folders {
        write(&folder.join("inner"), 5);
    }
    write(&t.join("data/library/huds/a/.tf2tools/thumbnail.jpg"), 3);
    // Two days ahead makes every entry old enough, so only the kind check can keep it.
    let later = SystemTime::now() + 2 * DAY;
    let roots = roots(t).with_appimage(appimage);
    assert_eq!(sweep_leftovers(&roots, later, DAY), Freed::default());
    assert_eq!(
        sweep_game_leftovers(&t.join("tf"), later, DAY),
        Freed::default()
    );
    for folder in &folders {
        assert!(folder.join("inner").exists(), "{}", folder.display());
    }
}

#[cfg(unix)]
#[test]
fn a_backup_that_is_a_link_stays_and_so_does_its_target() {
    let tmp = tempfile::tempdir().unwrap();
    let appimage = tmp.path().join("apps/TF2Tools.AppImage");
    let bak = tmp.path().join("apps/TF2Tools.AppImage.bak");
    let target = tmp.path().join("elsewhere");
    write(&appimage, 8);
    write(&target, 4);
    std::os::unix::fs::symlink(&target, &bak).unwrap();
    let freed = sweep(&roots(tmp.path()).with_appimage(appimage));
    assert_eq!(freed, Freed::default());
    assert!(!gone(&bak) && target.exists());
}

#[test]
fn a_v5_webp_thumbnail_goes_only_beside_a_v6_jpg() {
    let tmp = tempfile::tempdir().unwrap();
    let huds = tmp.path().join("data/library/huds");
    let paired = huds.join("a/.tf2tools/thumbnail.webp");
    write(&paired, 11);
    write(&huds.join("a/.tf2tools/thumbnail.jpg"), 3);
    write(&huds.join("b/.tf2tools/thumbnail.webp"), 11);
    write(&huds.join("c/thumbnail.webp"), 11);
    write(&huds.join("c/thumbnail.jpg"), 3);
    let freed = sweep(&roots(tmp.path()));
    assert_eq!(freed.paths(), std::slice::from_ref(&paired));
    assert_eq!(freed.bytes(), 11);
    assert!(huds.join("a/.tf2tools/thumbnail.jpg").exists());
    assert!(huds.join("b/.tf2tools/thumbnail.webp").exists());
    assert!(huds.join("c/thumbnail.webp").exists());
}

#[cfg(unix)]
#[test]
fn a_linked_hud_folder_is_not_followed_for_its_thumbnail() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().join("outside");
    write(&outside.join(".tf2tools/thumbnail.webp"), 11);
    write(&outside.join(".tf2tools/thumbnail.jpg"), 3);
    let huds = tmp.path().join("data/library/huds");
    std::fs::create_dir_all(&huds).unwrap();
    std::os::unix::fs::symlink(&outside, huds.join("linked")).unwrap();
    assert_eq!(sweep(&roots(tmp.path())), Freed::default());
    assert!(outside.join(".tf2tools/thumbnail.webp").exists());
}

#[test]
fn aged_download_parts_in_custom_and_maps_go() {
    let tmp = tempfile::tempdir().unwrap();
    let tf = tmp.path().join("tf");
    let aged = [
        tf.join("custom/.tf2tools-AbC123.part"),
        tf.join("maps/.tf2tools-Zz9yX0.part"),
    ];
    for path in &aged {
        write(path, 10);
        age(path, 2 * DAY);
    }
    let fresh = tf.join("custom/.tf2tools-QqQqQq.part");
    write(&fresh, 10);
    age(&fresh, HOUR);
    let keep = [
        tf.join("custom/.tf2tools-AbC123.part.bak"),
        tf.join("custom/.tf2tools-AbC12.part"),
        tf.join("custom/tf2tools-AbC123.part"),
        tf.join("custom/budhud.vpk"),
        tf.join("maps/cp_x.bsp"),
        // The game's own download folder; TF2Tools never writes parts there.
        tf.join("download/maps/.tf2tools-AbC123.part"),
    ];
    for path in &keep {
        write(path, 3);
        age(path, 2 * DAY);
    }
    let freed = sweep_game_leftovers(&tf, SystemTime::now(), DAY);
    let mut want = aged.to_vec();
    want.sort();
    assert_eq!(sorted(&freed), want);
    assert_eq!(freed.bytes(), 20);
    assert!(fresh.exists());
    for path in &keep {
        assert!(path.exists(), "{}", path.display());
    }
}

#[cfg(unix)]
#[test]
fn a_local_folder_that_links_to_an_empty_folder_is_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::os::unix::fs::symlink(&target, tmp.path().join("local")).unwrap();
    assert_eq!(sweep(&roots(tmp.path())), Freed::default());
    assert!(target.is_dir() && !gone(&tmp.path().join("local")));
}

#[test]
fn an_empty_local_folder_that_is_a_parent_of_the_data_folder_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let local = tmp.path().join("local");
    std::fs::create_dir_all(&local).unwrap();
    // The data folder is not created, so only the prefix check keeps the empty parent.
    let nested = CleanupRoots::new(
        local.join("data"),
        local.clone(),
        tmp.path().join("demos"),
        tmp.path().join("temp"),
    );
    assert_eq!(sweep(&nested), Freed::default());
    assert!(local.is_dir());
}
