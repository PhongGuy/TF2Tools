//! The switch layer: written and removed paths over a copy, pristine reads beneath.

use tf2tools_core::HudFiles;

fn copy() -> HudFiles {
    HudFiles::from_memory(vec![vec![
        (
            "customizations/damage.res".to_owned(),
            b"//#base \"a.res\"\n".to_vec(),
        ),
        ("#customization/opt.res".to_owned(), b"\"R\" { }".to_vec()),
    ]])
    .with_install_folder("somehud")
}

#[test]
fn written_paths_win_and_removed_paths_vanish() {
    let files = copy().with_switches(
        vec![
            (
                "customizations/damage.res".to_owned(),
                b"#base \"a.res\"\n".to_vec(),
            ),
            (
                "#customization/_enabled/opt.res".to_owned(),
                b"\"R\" { }".to_vec(),
            ),
        ],
        vec!["#customization/opt.res".to_owned()],
    );
    let read = |p: &str| files.read(p).unwrap().map(|f| f.into_bytes());
    assert_eq!(
        read("customizations/damage.res").unwrap(),
        b"#base \"a.res\"\n"
    );
    assert!(files.contains("#customization/_enabled/opt.res"));
    assert!(!files.contains("#customization/opt.res"));
    assert_eq!(
        files.list("#customization/"),
        ["#customization/_enabled/opt.res"]
    );
    assert_eq!(
        files.switched_paths(),
        (
            vec![
                "#customization/_enabled/opt.res".to_owned(),
                "customizations/damage.res".to_owned()
            ],
            vec!["#customization/opt.res".to_owned()]
        )
    );
}

#[test]
fn pristine_reads_skip_the_switch_layer() {
    let files = copy().with_switches(
        vec![("customizations/damage.res".to_owned(), b"changed".to_vec())],
        vec!["#customization/opt.res".to_owned()],
    );
    let pristine = |p: &str| files.read_pristine(p).unwrap().map(|f| f.into_bytes());
    assert_eq!(
        pristine("customizations/damage.res").unwrap(),
        b"//#base \"a.res\"\n"
    );
    assert_eq!(pristine("#customization/opt.res").unwrap(), b"\"R\" { }");
    assert!(files.in_copy("customizations/damage.res"));
    assert!(!files.in_copy("#customization/_enabled/opt.res"));
}

#[test]
fn the_original_view_is_untouched_and_hostile_paths_are_skipped() {
    let base = copy();
    let files = base.with_switches(
        vec![
            ("../outside.res".to_owned(), b"x".to_vec()),
            ("cfg/autoexec.cfg".to_owned(), b"x".to_vec()),
        ],
        vec!["/etc/passwd".to_owned()],
    );
    assert_eq!(files.switched_paths(), (Vec::new(), Vec::new()));
    assert!(base.read("customizations/damage.res").unwrap().is_some());
    assert_eq!(base.switched_paths(), (Vec::new(), Vec::new()));
    assert_eq!(
        base.copy_paths(),
        ["#customization/opt.res", "customizations/damage.res"]
    );
}

#[test]
fn the_metadata_folder_is_never_a_switch_path() {
    let files = copy().with_switches(
        vec![
            (".tf2tools/maker.json".to_owned(), b"{}".to_vec()),
            (".TF2Tools/original/x.res".to_owned(), b"x".to_vec()),
            ("resource/.tf2tools/x.res".to_owned(), b"x".to_vec()),
            ("resource/.tf2tools_/x.res".to_owned(), b"kept".to_vec()),
            ("resource/../.tf2tools/maker.json".to_owned(), b"x".to_vec()),
            ("./.TF2Tools/x.res".to_owned(), b"x".to_vec()),
            ("a\\.tf2tools\\x.res".to_owned(), b"x".to_vec()),
        ],
        vec![".tf2tools/maker.json".to_owned()],
    );
    assert_eq!(
        files.switched_paths(),
        (vec!["resource/.tf2tools_/x.res".to_owned()], Vec::new())
    );
}
