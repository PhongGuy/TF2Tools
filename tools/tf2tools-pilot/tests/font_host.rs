use std::path::Path;

use tf2tools_core::FontHost;
use tf2tools_pilot::PilotFontHost;

#[test]
fn the_pilot_font_host_knows_no_fonts_and_keeps_its_own_dir() {
    let host = PilotFontHost::new(Path::new("/scratch/data"));
    assert!(host.installed_names().unwrap().is_empty());
    assert_eq!(
        host.fonts_dir().unwrap(),
        Path::new("/scratch/data/pilot-fonts")
    );
    host.register(Path::new("/scratch/data/pilot-fonts/a.ttf"), "A", false)
        .unwrap();
    host.refresh().unwrap();
}
