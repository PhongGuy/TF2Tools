//! HUD images: material lookup, texture decoding and why an image cannot be shown.

use tf2tools_core::{
    HudFiles, HudImage, ImageProblem, Platform, RgbaImage, decode_vtf, rgba_to_vtf,
};

/// A one-mip, one-frame v7.2 VTF of `format` (the VTF format number) holding `data` as given.
fn raw_vtf(format: i32, width: u16, height: u16, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"VTF\0");
    out.extend_from_slice(&7u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&80u32.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&0x2300u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    for _ in 0..3 {
        out.extend_from_slice(&0f32.to_le_bytes());
    }
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&1f32.to_le_bytes());
    out.extend_from_slice(&format.to_le_bytes());
    out.push(1);
    out.extend_from_slice(&(-1i32).to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.resize(80, 0);
    out.extend_from_slice(data);
    out
}

fn red_2x2() -> RgbaImage {
    RgbaImage::from_pixels(2, 2, [255, 0, 0, 255].repeat(4)).unwrap()
}

fn files(entries: Vec<(&str, Vec<u8>)>) -> HudFiles {
    HudFiles::from_memory(vec![
        entries
            .into_iter()
            .map(|(p, b)| (p.to_owned(), b))
            .collect(),
    ])
}

#[test]
fn luminance_and_alpha_formats_decode() {
    let i8 = decode_vtf(&raw_vtf(5, 2, 1, &[10, 200])).unwrap();
    assert_eq!(i8.pixels(), [10, 10, 10, 255, 200, 200, 200, 255]);
    let ia88 = decode_vtf(&raw_vtf(6, 1, 1, &[30, 40])).unwrap();
    assert_eq!(ia88.pixels(), [30, 30, 30, 40]);
    let a8 = decode_vtf(&raw_vtf(8, 1, 1, &[77])).unwrap();
    assert_eq!(a8.pixels(), [0, 0, 0, 77]);
}

#[test]
fn an_image_resolves_through_its_material() {
    let files = files(vec![
        (
            "materials/hud/health_bg.vmt",
            br#""UnlitGeneric" { "$baseTexture" "HUD\health_bg" "$translucent" "1" }"#.to_vec(),
        ),
        (
            "materials/hud/health_bg.vtf",
            rgba_to_vtf(&red_2x2()).unwrap(),
        ),
    ]);
    let HudImage::Found(texture) = files.image("../hud/health_bg", Platform::Linux).unwrap() else {
        panic!("not found");
    };
    assert_eq!(texture.image(), &red_2x2());
    assert_eq!(texture.material(), "materials/hud/health_bg.vmt");
    assert_eq!(texture.texture(), "materials/hud/health_bg.vtf");
    assert_eq!(texture.scale(), (1.0, 1.0));
    assert!(!texture.distance_field());
}

#[test]
fn transforms_and_distance_fields_are_reported() {
    let files = files(vec![
        (
            "materials/vgui/replay/thumbnails/icon.vmt",
            br#""UnlitGeneric" { "$basetexture" "vgui/replay/thumbnails/icon.vtf" "$basetexturetransform" "center .5 .5 scale -1 1 rotate 0 translate 0 0" "$distancealpha" "1" }"#.to_vec(),
        ),
        ("materials/vgui/replay/thumbnails/icon.vtf", rgba_to_vtf(&red_2x2()).unwrap()),
    ]);
    let HudImage::Found(texture) = files
        .image("replay/thumbnails/ICON.vmt", Platform::Linux)
        .unwrap()
    else {
        panic!("not found");
    };
    assert_eq!(texture.scale(), (-1.0, 1.0));
    assert!(texture.distance_field());
}

#[test]
fn a_material_s_alpha_fades_the_texture_as_drawn() {
    let vmt = |alpha: &str| {
        format!(r#""UnlitGeneric" {{ "$basetexture" "hud/panel" "$translucent" "1"{alpha} }}"#)
            .into_bytes()
    };
    let found = |alpha: &str| {
        let files = files(vec![
            ("materials/hud/panel.vmt", vmt(alpha)),
            ("materials/hud/panel.vtf", rgba_to_vtf(&red_2x2()).unwrap()),
        ]);
        match files.image("../hud/panel", Platform::Linux).unwrap() {
            HudImage::Found(texture) => texture,
            other => panic!("{other:?}"),
        }
    };
    // The win panel's team panels fade this way: the texels stay, the game draws them faded.
    let faded = found(r#" "$alpha" ".5""#);
    assert_eq!(faded.alpha(), 0.5);
    assert_eq!(faded.image(), &red_2x2());
    assert_eq!(faded.drawn().pixels(), [255, 0, 0, 128].repeat(4));
    // Without the key the texture draws as it is; a value past either end is clamped.
    let plain = found("");
    assert_eq!(plain.alpha(), 1.0);
    assert_eq!(plain.drawn(), red_2x2());
    assert_eq!(found(r#" "$alpha" "3""#).alpha(), 1.0);
    assert_eq!(found(r#" "$alpha" "-1""#).alpha(), 0.0);
}

#[test]
fn missing_and_unreadable_images_say_why() {
    let vmt = |base: &str| format!(r#""UnlitGeneric" {{ "$basetexture" "{base}" }}"#).into_bytes();
    let files = files(vec![
        (
            "materials/hud/nobase.vmt",
            br#""UnlitGeneric" { "$translucent" "1" }"#.to_vec(),
        ),
        ("materials/hud/gone.vmt", vmt("hud/gone")),
        (
            "materials/hud/patched.vmt",
            br#""patch" { "include" "materials/hud/x.vmt" }"#.to_vec(),
        ),
        ("materials/hud/huge.vmt", vmt("hud/huge")),
        ("materials/hud/huge.vtf", raw_vtf(12, 8192, 4096, &[])),
        ("materials/hud/junk.vmt", vmt("hud/junk")),
        ("materials/hud/junk.vtf", b"not a vtf".to_vec()),
    ]);
    let image = |name: &str| files.image(name, Platform::Linux).unwrap();
    assert_eq!(
        image("../hud/nothing"),
        HudImage::Missing {
            path: "materials/hud/nothing.vmt".into()
        }
    );
    assert_eq!(
        image("../../../x"),
        HudImage::Missing {
            path: "../../../x".into()
        }
    );
    assert_eq!(
        image("../hud/gone"),
        HudImage::Missing {
            path: "materials/hud/gone.vtf".into()
        }
    );
    assert_eq!(
        image("../hud/nobase"),
        HudImage::Unreadable {
            path: "materials/hud/nobase.vmt".into(),
            problem: ImageProblem::NoBaseTexture
        }
    );
    assert_eq!(
        image("../hud/patched"),
        HudImage::Unreadable {
            path: "materials/hud/patched.vmt".into(),
            problem: ImageProblem::Patch
        }
    );
    assert_eq!(
        image("../hud/huge"),
        HudImage::Unreadable {
            path: "materials/hud/huge.vtf".into(),
            problem: ImageProblem::TooLarge {
                width: 8192,
                height: 4096
            },
        }
    );
    assert!(matches!(
        image("../hud/junk"),
        HudImage::Unreadable {
            problem: ImageProblem::Decode(_),
            ..
        }
    ));
}

#[test]
fn image_lookups_stay_inside_materials() {
    let files = files(vec![
        (
            "materials/vgui.vmt",
            br#""UnlitGeneric" { "$basetexture" "hud/x" }"#.to_vec(),
        ),
        (
            "materials/hud/out.vmt",
            br#""UnlitGeneric" { "$basetexture" "../resource/x" }"#.to_vec(),
        ),
        ("resource/x.vtf", raw_vtf(12, 2, 2, &[0; 12])),
        (
            "resource/y.vmt",
            br#""UnlitGeneric" { "$basetexture" "hud/x" }"#.to_vec(),
        ),
    ]);
    let image = |name: &str| files.image(name, Platform::Linux).unwrap();
    let missing = |path: &str| HudImage::Missing { path: path.into() };
    assert_eq!(image(""), missing(""));
    assert_eq!(image("./"), missing("./"));
    assert_eq!(image("../hud/out"), missing("../resource/x"));
    assert_eq!(image("../../resource/y"), missing("../../resource/y"));
}

#[test]
fn images_cross_threads() {
    fn assert_send_sync<T: Send + Sync + std::fmt::Debug + 'static>() {}
    assert_send_sync::<HudImage>();
    assert_send_sync::<tf2tools_core::HudTexture>();
    assert_send_sync::<ImageProblem>();
}
