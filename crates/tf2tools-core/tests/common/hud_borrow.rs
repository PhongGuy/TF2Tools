use super::hud_stock::stock;
use tf2tools_core::{BorrowSource, HudFiles, HudModel, Platform};

/// A source HUD: `top` files over the stock files, as a library HUD folder sits above the game.
pub fn files_over_stock(top: &[(&str, &str)]) -> HudFiles {
    let top = top
        .iter()
        .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
        .collect();
    let stock = stock()
        .into_iter()
        .map(|(p, t)| (p, t.into_bytes()))
        .collect();
    HudFiles::from_memory(vec![top, stock])
}

pub fn source(top: &[(&str, &str)]) -> BorrowSource {
    let files = files_over_stock(top);
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    BorrowSource::new(model, files, "Mine HUD", "Mia", "GameBanana item 7")
}

/// Like `files_over_stock` with raw bytes, for fonts and pictures.
pub fn files_over_stock_bytes(top: &[(&str, Vec<u8>)]) -> HudFiles {
    let top = top
        .iter()
        .map(|(p, b)| ((*p).to_owned(), b.clone()))
        .collect();
    let stock = stock()
        .into_iter()
        .map(|(p, t)| (p, t.into_bytes()))
        .collect();
    HudFiles::from_memory(vec![top, stock])
}

/// A source HUD over ready files.
pub fn source_over(files: HudFiles) -> BorrowSource {
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    BorrowSource::new(model, files, "Mine HUD", "Mia", "GameBanana item 7")
}
