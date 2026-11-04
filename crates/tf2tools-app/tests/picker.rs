use std::path::PathBuf;

use gpui_kit::TestAppContext;
use tf2tools_app::picker::{PathPicker, PickKind, ScriptedPicker, file_name_for, with_extension};

#[gpui_kit::test]
async fn a_scripted_picker_hands_out_its_queue_then_nothing(cx: &mut TestAppContext) {
    let picker = ScriptedPicker::new([PathBuf::from("/a.zip"), PathBuf::from("/b.wav")]);
    let first = cx.update(|cx| picker.pick(PickKind::HudArchive, cx)).await;
    let second = cx.update(|cx| picker.pick(PickKind::SoundFile, cx)).await;
    let third = cx.update(|cx| picker.pick(PickKind::Tf2Folder, cx)).await;
    assert_eq!(first, Some(PathBuf::from("/a.zip")));
    assert_eq!(second, Some(PathBuf::from("/b.wav")));
    assert_eq!(third, None);
    assert_eq!(
        picker.asked(),
        vec![
            PickKind::HudArchive,
            PickKind::SoundFile,
            PickKind::Tf2Folder
        ]
    );
}

#[gpui_kit::test]
async fn a_scripted_save_shares_the_queue_and_records_kind_and_name(cx: &mut TestAppContext) {
    let picker = ScriptedPicker::new([PathBuf::from("/out/a.png"), PathBuf::from("/in/b.png")]);
    let saved = cx
        .update(|cx| picker.save_to(PickKind::ExportPng, "Zebra.png", cx))
        .await;
    let picked = cx.update(|cx| picker.pick(PickKind::MakerPng, cx)).await;
    let none = cx
        .update(|cx| picker.save_to(PickKind::ExportPng, "Yak.png", cx))
        .await;
    assert_eq!(saved, Some(PathBuf::from("/out/a.png")));
    assert_eq!(picked, Some(PathBuf::from("/in/b.png")));
    assert_eq!(none, None);
    assert_eq!(
        picker.asked(),
        vec![PickKind::ExportPng, PickKind::MakerPng, PickKind::ExportPng]
    );
    assert_eq!(picker.suggested(), vec!["Zebra.png", "Yak.png"]);
}

#[test]
fn file_names_keep_plain_characters_only() {
    assert_eq!(file_name_for("Zebra Dot", "png"), "Zebra Dot.png");
    assert_eq!(file_name_for("a/b\\c:d*e?", "png"), "a_b_c_d_e_.png");
    assert_eq!(
        file_name_for("Croix (rouge)-2_x", "png"),
        "Croix (rouge)-2_x.png"
    );
    assert_eq!(file_name_for("Zébra", "png"), "Zébra.png");
    assert_eq!(file_name_for("   ", "png"), "crosshair.png");
}

#[test]
fn file_names_never_use_a_name_windows_reserves() {
    for stem in ["CON", "prn", "Aux", "nul", "COM1", "com9", "LPT1", "lpt9"] {
        assert_eq!(file_name_for(stem, "png"), format!("{stem}_.png"), "{stem}");
    }
    // With an extension typed into the name, or spaces around it, the stem is still reserved.
    assert_eq!(file_name_for("con.txt", "png"), "con_txt.png");
    assert_eq!(file_name_for(" NUL ", "png"), "NUL_.png");
    assert_eq!(file_name_for("nul.", "png"), "nul_.png");
    // Only the exact device names are reserved.
    for plain in ["CONSOLE", "COM10", "LPT0", "Auxiliary", "nul2"] {
        assert_eq!(file_name_for(plain, "png"), format!("{plain}.png"));
    }
}

#[test]
fn file_names_never_end_in_a_dot_or_space() {
    assert_eq!(file_name_for("Zebra . ", "png"), "Zebra _.png");
    assert_eq!(file_name_for("Zebra...", "png"), "Zebra___.png");
}

#[test]
fn with_extension_adds_png_only_when_missing() {
    assert_eq!(
        with_extension(PathBuf::from("/t/a"), "png"),
        PathBuf::from("/t/a.png")
    );
    assert_eq!(
        with_extension(PathBuf::from("/t/a.PNG"), "png"),
        PathBuf::from("/t/a.PNG")
    );
    assert_eq!(
        with_extension(PathBuf::from("/t/a.txt"), "png"),
        PathBuf::from("/t/a.txt.png")
    );
}
