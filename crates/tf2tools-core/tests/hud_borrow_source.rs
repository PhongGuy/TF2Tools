use crate::common;

use common::{LibraryEnv, gb, import_hud_as, library_env, temp_tf2_tree, vpk_bytes, write_file};
use tf2tools_core::{HudFiles, HudId, LibraryKind};

const GAMEINFO: &str = "\"GameInfo\"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tgame+mod+vgui\ttf/tf2_misc.vpk\n\t\t}\n\t}\n}\n";

fn game_with_misc() -> (tempfile::TempDir, std::path::PathBuf) {
    let (tmp, game) = temp_tf2_tree();
    let tf = game.join("tf");
    write_file(&tf.join("gameinfo.txt"), GAMEINFO.as_bytes());
    write_file(
        &tf.join("tf2_misc_dir.vpk"),
        &vpk_bytes(&[
            ("resource/ui/hudplayerhealth.res", b"game health"),
            ("resource/ui/other.res", b"game other"),
        ]),
    );
    (tmp, tf)
}

fn hud_dir(env: &LibraryEnv, folder: &str) -> std::path::PathBuf {
    env.dir.library_dir(LibraryKind::Huds).join(folder)
}

#[test]
fn a_hud_folder_reads_above_the_game_and_leaves_its_metadata_out() {
    let (_tmp, tf) = game_with_misc();
    let hud = tf.parent().unwrap().join("hud");
    write_file(&hud.join("Resource/UI/HudPlayerHealth.res"), b"hud health");
    write_file(&hud.join(".tf2tools/maker.json"), b"{}");
    let files = HudFiles::open_hud(&tf, &hud).unwrap();
    let read = |p: &str| files.read(p).unwrap().map(|f| f.into_bytes());
    assert_eq!(
        read("resource/ui/hudplayerhealth.res").unwrap(),
        b"hud health"
    );
    assert_eq!(read("resource/ui/other.res").unwrap(), b"game other");
    assert!(!files.contains(".tf2tools/maker.json"));
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_and_an_oversized_file_are_not_indexed() {
    let (_tmp, tf) = game_with_misc();
    let hud = tf.parent().unwrap().join("hud");
    write_file(&hud.join("resource/ui/real.res"), b"real");
    std::os::unix::fs::symlink("/etc/passwd", hud.join("resource/ui/link.res")).unwrap();
    let big = std::fs::File::create(hud.join("resource/ui/big.res")).unwrap();
    big.set_len(32 * 1024 * 1024 + 1).unwrap();
    let files = HudFiles::open_hud(&tf, &hud).unwrap();
    assert!(files.contains("resource/ui/real.res"));
    assert!(!files.contains("resource/ui/link.res"));
    assert!(!files.contains("resource/ui/big.res"));
}

#[test]
fn only_huds_with_readable_res_files_are_borrowable_and_never_the_one_being_edited() {
    let env = library_env();
    let good = import_hud_as(&env, "goodhud", gb(1));
    let bare = import_hud_as(&env, "barehud", gb(2));
    let mine = import_hud_as(&env, "minehud", gb(3));
    for (hud, name) in [(&good, "goodhud"), (&mine, "minehud")] {
        write_file(
            &hud_dir(&env, hud.folder_name()).join("resource/ui/hudplayerhealth.res"),
            name.as_bytes(),
        );
    }
    let names = |except: Option<&HudId>| -> Vec<String> {
        env.lib
            .list_borrowable(except)
            .unwrap()
            .iter()
            .map(|h| h.name().to_owned())
            .collect()
    };
    // `bare` is made truly bare: import_hud_as writes a resource/clientscheme.res stub.
    std::fs::remove_dir_all(hud_dir(&env, bare.folder_name()).join("resource")).unwrap();
    let all = names(None);
    assert!(all.contains(&good.name().to_owned()) && all.contains(&mine.name().to_owned()));
    assert!(!all.contains(&bare.name().to_owned()));
    assert!(!names(Some(mine.id())).contains(&mine.name().to_owned()));
}

#[test]
fn borrow_files_reads_the_huds_folder_over_the_game_and_refuses_unknown_ids() {
    let env = library_env();
    let (_tmp, tf) = game_with_misc();
    let hud = import_hud_as(&env, "goodhud", gb(1));
    write_file(
        &hud_dir(&env, hud.folder_name()).join("resource/ui/hudplayerhealth.res"),
        b"borrowed",
    );
    let (entry, files) = env.lib.borrow_files(hud.id(), &tf).unwrap();
    assert_eq!(entry.id(), hud.id());
    assert_eq!(
        files
            .read("resource/ui/hudplayerhealth.res")
            .unwrap()
            .unwrap()
            .bytes(),
        b"borrowed"
    );
    assert!(files.contains("resource/ui/other.res"));
    let err = env.lib.borrow_files(&HudId::new("nope"), &tf).unwrap_err();
    assert!(matches!(
        err,
        tf2tools_core::BorrowSourceError::NotFound { .. }
    ));
}

#[cfg(unix)]
#[test]
fn a_file_swapped_for_a_link_or_grown_after_indexing_is_refused_at_read() {
    let (_tmp, tf) = game_with_misc();
    let hud = tf.parent().unwrap().join("hud");
    write_file(&hud.join("resource/ui/swap.res"), b"real");
    write_file(&hud.join("resource/ui/grow.res"), b"small");
    let files = HudFiles::open_hud(&tf, &hud).unwrap();
    let secret = write_file(&tf.parent().unwrap().join("outside.txt"), b"secret");
    std::fs::remove_file(hud.join("resource/ui/swap.res")).unwrap();
    std::os::unix::fs::symlink(&secret, hud.join("resource/ui/swap.res")).unwrap();
    assert!(files.read("resource/ui/swap.res").is_err());
    let grow = std::fs::OpenOptions::new()
        .write(true)
        .open(hud.join("resource/ui/grow.res"))
        .unwrap();
    grow.set_len(32 * 1024 * 1024 + 1).unwrap();
    assert!(files.read("resource/ui/grow.res").is_err());
    // A folder swapped for a link is refused too.
    let elsewhere = tf.parent().unwrap().join("elsewhere");
    write_file(&elsewhere.join("secret.res"), b"secret");
    write_file(&hud.join("resource/sub/a.res"), b"a");
    let files = HudFiles::open_hud(&tf, &hud).unwrap();
    std::fs::remove_dir_all(hud.join("resource/sub")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, hud.join("resource/sub")).unwrap();
    assert!(files.read("resource/sub/a.res").is_err());
}

#[cfg(unix)]
#[test]
fn a_library_folder_that_is_itself_a_link_is_never_opened_or_offered() {
    let env = library_env();
    let (_tmp, tf) = game_with_misc();
    let hud = import_hud_as(&env, "linkhud", gb(1));
    let real = tf.parent().unwrap().join("realhud");
    write_file(&real.join("resource/ui/hudplayerhealth.res"), b"x");
    let live = hud_dir(&env, hud.folder_name());
    std::fs::remove_dir_all(&live).unwrap();
    std::os::unix::fs::symlink(&real, &live).unwrap();
    assert!(HudFiles::open_hud(&tf, &live).is_err());
    assert!(env.lib.borrow_files(hud.id(), &tf).is_err());
    let names: Vec<String> = env
        .lib
        .list_borrowable(None)
        .unwrap()
        .iter()
        .map(|h| h.name().to_owned())
        .collect();
    assert!(!names.contains(&hud.name().to_owned()));
}
