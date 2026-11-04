use gpui_kit::TestAppContext;
use tf2tools_app::picker::PickKind;

// gpui-pilot's `pick_file` answers the platform's file dialogs, so every pick the app makes
// must open one rather than be answered inside the app.
#[gpui_kit::test]
fn every_pick_opens_the_platform_dialog(cx: &mut TestAppContext) {
    let picker = tf2tools_pilot::picker();
    for kind in [
        PickKind::HudArchive,
        PickKind::SoundFile,
        PickKind::CrosshairImage,
        PickKind::Tf2Folder,
        PickKind::MakerPng,
    ] {
        let _pick = cx.update(|cx| picker.pick(kind, cx));
        cx.run_until_parked();
        assert!(cx.did_prompt_for_paths(), "{kind:?} opened no dialog");
        cx.simulate_path_prompt_response(|_| None);
    }
    let _save = cx.update(|cx| picker.save_to(PickKind::ExportPng, "cross.png", cx));
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path(), "Export PNG opened no dialog");
    cx.simulate_new_path_selection(|_| None);
}
