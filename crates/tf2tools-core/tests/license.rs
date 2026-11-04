use std::path::PathBuf;

fn repo_file(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

// Line breaks may differ from the canonical text; the words may not.
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_repo_root_holds_the_mit_license() {
    let text = repo_file("LICENSE");
    assert!(
        text.starts_with("MIT License\n\nCopyright (c) 2021-2026 TF2Tools contributors\n"),
        "{text}"
    );
    let flat = words(&text);
    for clause in [
        "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction,",
        "The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.",
        "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,",
        "OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.",
    ] {
        assert!(flat.contains(clause), "missing: {clause}");
    }
}

#[test]
fn the_manifest_and_readme_point_at_it() {
    assert!(repo_file("Cargo.toml").contains("license = \"MIT\""));
    let readme = repo_file("README.md");
    let section = readme
        .split("\n## License\n")
        .nth(1)
        .expect("a License section")
        .split("\n## ")
        .next()
        .unwrap();
    assert!(section.contains("[LICENSE](LICENSE)"), "{section}");
    assert!(section.contains("licenses/Inter-OFL.txt"), "{section}");
    assert!(section.contains("licenses/Lucide-ISC.txt"), "{section}");
}

#[test]
fn the_bundled_lucide_icons_carry_their_isc_notice() {
    let flat = words(&repo_file("licenses/Lucide-ISC.txt"));
    for clause in [
        "ISC License Copyright (c) 2026 Lucide Icons and Contributors",
        "provided that the above copyright notice and this permission notice appear in all copies.",
        "The MIT License (MIT) (for the icons listed above) Copyright (c) 2013-present Cole Bemis",
    ] {
        assert!(flat.contains(clause), "missing: {clause}");
    }
}
