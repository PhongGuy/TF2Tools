//! Reading HUD option schemas: comments, caps, the bundled set and repo matching.

use tf2tools_core::{HudSchema, HudSchemas, SchemaError, normalise_repo};

#[test]
fn comments_go_and_strings_keep_their_slashes() {
    let text = "\u{feff}{\n  // groups\n  \"Links\": { \"GitHub\": \"https://github.com/Jotunn/kbnhud\" },\n  \"EnabledFolder\": \"^customizations//#enabled\", /* inline */\n  \"Controls\": { \"A\": [ { \"Name\": \"x\", \"Label\": \"X\", \"Type\": \"CheckBox\", \"Restart\": false // why\n } ] }\n}\n";
    let schema = HudSchema::parse("kbnhud", text.as_bytes()).unwrap();
    assert_eq!(
        (schema.name(), schema.repo(), schema.control_count()),
        ("kbnhud", Some("jotunn/kbnhud"), 1)
    );
}

#[test]
fn oversized_or_broken_schemas_are_refused() {
    let big = format!(
        "{{ \"Controls\": {{}}, \"pad\": \"{}\" }}",
        "a".repeat(1024 * 1024)
    );
    assert!(matches!(
        HudSchema::parse("big", big.as_bytes()),
        Err(SchemaError::TooLarge)
    ));
    let long = format!(
        "{{ \"Controls\": {{ \"A\": [ {{ \"Name\": \"{}\" }} ] }} }}",
        "n".repeat(4097)
    );
    assert!(matches!(
        HudSchema::parse("long", long.as_bytes()),
        Err(SchemaError::TooLarge)
    ));
    let many: Vec<String> = (0..2001)
        .map(|i| format!("{{ \"Name\": \"c{i}\", \"Type\": \"CheckBox\" }}"))
        .collect();
    let text = format!("{{ \"Controls\": {{ \"A\": [ {} ] }} }}", many.join(","));
    assert!(matches!(
        HudSchema::parse("many", text.as_bytes()),
        Err(SchemaError::TooLarge)
    ));
    for bad in [
        "",
        "[]",
        "{ \"Controls\": 3 }",
        "{ \"Controls\": { \"A\": [ 1 ] } }",
        "{ \"Controls\": {",
    ] {
        assert!(
            matches!(
                HudSchema::parse("bad", bad.as_bytes()),
                Err(SchemaError::Malformed { .. })
            ),
            "{bad}"
        );
    }
}

#[test]
fn the_bundled_set_reads_whole() {
    let schemas = HudSchemas::bundled();
    let mut names = schemas.names();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "berryhud",
            "budhud",
            "darhud",
            "eve-plus",
            "flawhud",
            "hexhud",
            "hud-fixes",
            "hypnotize-hud",
            "idhud",
            "kbnhud",
            "m0rehud-classic",
            "middle-mann",
            "rayshud",
            "sunsethud",
            "zeeshud",
        ]
    );
    let controls: usize = names
        .iter()
        .map(|n| schemas.for_name(n).unwrap().control_count())
        .sum();
    assert_eq!(controls, 602);
}

#[test]
fn a_schema_matches_by_repo_ignoring_case_and_dot_git() {
    let schemas = HudSchemas::bundled();
    for repo in [
        "https://github.com/rbjaxter/budhud",
        "https://GitHub.com/RBJAXTER/budhud.git/",
        "rbjaxter/budhud",
    ] {
        assert_eq!(
            schemas.for_repo(repo).map(HudSchema::name),
            Some("budhud"),
            "{repo}"
        );
    }
    assert!(
        schemas
            .for_repo("https://github.com/someone/budhud")
            .is_none()
    );
    assert!(schemas.for_repo("").is_none());
    assert_eq!(normalise_repo("https://gamebanana.com/mods/1"), None);
}

#[test]
fn groups_keep_the_file_order() {
    let text = r#"{ "Controls": { "Zeta": [ { "Name": "z", "Type": "CheckBox" } ], "Alpha": [], "Mid": [], "Alpha": [ { "Name": "dup" } ] } }"#;
    let schema = HudSchema::parse("o", text.as_bytes()).unwrap();
    assert_eq!(schema.groups(), ["Zeta", "Alpha", "Mid"]);
    assert_eq!(schema.control_count(), 1);
}

#[test]
fn the_bundled_files_are_the_editors_bytes() {
    use sha2::Digest as _;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/hud-schemas");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    let got: Vec<(String, String)> = names
        .iter()
        .map(|n| {
            let bytes = std::fs::read(dir.join(n)).unwrap();
            (
                n.clone(),
                sha2::Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
            )
        })
        .collect();
    let want = [
        (
            "berryhud.json",
            "5120a2bab6ffc1da3e0a33627fe27d93afdcc7fb2a0587082155ecf5f0829965",
        ),
        (
            "budhud.json",
            "ef8ef1914b2c8c9e60cbc88228493a1f237e21f6a39c2252d4b636dea3f3ad10",
        ),
        (
            "darhud.json",
            "04693bde29b55d5bd0ccfd7f63b790662c4352f5955d4524664dc9ee383bd858",
        ),
        (
            "eve-plus.json",
            "8e2e37b9719ff6cd10da02f82aee97cebe22114d63cbcaf8c56c6308a58287e8",
        ),
        (
            "flawhud.json",
            "06e767cf7a4774793d3a4cc258fcfe9b1975c4f6e828d9f241ac7cc54d0e8b33",
        ),
        (
            "hexhud.json",
            "7dd3c3949ba8f6e2bb0e19af2b2b06ad1027fc60afd15da3dc025e21a2cc4c96",
        ),
        (
            "hud-fixes.json",
            "ffc4fe37b84a706d3308342d5adc813b0fc9c7eeed97de5e0b5fd4dd965ed4aa",
        ),
        (
            "hypnotize-hud.json",
            "44eafa23a59a71dd577bb69db04a1f143d92f464c24333a07d7a371f20851abb",
        ),
        (
            "idhud.json",
            "79da821f8c849338f240010e50665d7166ce0f52762e42150de07c923ab6f381",
        ),
        (
            "kbnhud.json",
            "e41144032f6bc198b0c6de31b667fb10592a5be34fcabd9c8dfabf5f1c8adf40",
        ),
        (
            "m0rehud-classic.json",
            "4864602b67526f4c87cb06ad26d2cf87d351302ad6be10a924d610794eba89de",
        ),
        (
            "middle-mann.json",
            "7272d51274ec47c4d5f06121e57de03cb0c375add953e3dd8c63544347960e46",
        ),
        (
            "rayshud.json",
            "de0fba380ba9c4eb1e3284a3ade302eda8b1b718688201fc4380cc20836714cd",
        ),
        (
            "sunsethud.json",
            "a00b6a994f9ffab1f3c4edc7785e8a113007aeb9b59c47cad78beb07ac5d483c",
        ),
        (
            "zeeshud.json",
            "b919588fbee4845458690fc131c7fe6e05daee0e1278a2ad7e85e5bde3f82536",
        ),
    ];
    let want: Vec<(String, String)> = want
        .iter()
        .map(|(n, h)| ((*n).to_owned(), (*h).to_owned()))
        .collect();
    assert_eq!(got, want);
}
