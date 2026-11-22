//! A synthetic Source BSP v20 and HL2DEMO header, written byte by byte so the demo viewer's tests
//! carry no third-party content. Shared by core's tests and, through `#[path]`, the app's.

/// What the synthetic map holds besides its fixed faces: a world floor (DEV/GREY, z 0), a
/// TOOLS/TOOLSCLIP face (z 8), a NODRAW face (z 16) and brush model `*1`, a 64x64 floor.
#[derive(Clone, Debug)]
pub struct BspOptions {
    /// The `"origin"` and `"angles"` of the `func_brush` whose model is `*1`.
    pub brush: (&'static str, &'static str),
    /// Each `info_player_teamspawn`'s `"origin"` and `"angles"`, in entity-lump order.
    pub spawns: Vec<(&'static str, &'static str)>,
    /// Gives the first face a plane index past the plane lump, which vbsp's `Face::normal` unwraps.
    pub bad_plane: bool,
    /// Every face uses the TOOLS texture, so nothing is drawn.
    pub only_tools: bool,
}

impl Default for BspOptions {
    fn default() -> Self {
        Self {
            brush: ("100 0 0", "0 90 0"),
            spawns: vec![("10 20 30", "0 45 0"), ("500 500 500", "0 180 0")],
            bad_plane: false,
            only_tools: false,
        }
    }
}

fn f32s(out: &mut Vec<u8>, values: &[f32]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn i32s(out: &mut Vec<u8>, values: &[i32]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn i16s(out: &mut Vec<u8>, values: &[i16]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// The map as `Bsp::read` accepts it: every lump vbsp 0.9.2 insists on (a node, its leaf, a
/// game lump with an empty `sprp`, an empty-zip pakfile) and the faces and entities above.
pub fn bsp(opts: &BspOptions) -> Vec<u8> {
    // Texture names 0 GREY, 1 TOOLS, 2 NODRAW-flagged; texinfo i uses texdata i.
    let names = ["DEV/GREY", "TOOLS/TOOLSCLIP", "DEV/NODRAWISH"];
    let (mut strdata, mut table) = (Vec::new(), Vec::new());
    for name in names {
        i32s(&mut table, &[strdata.len() as i32]);
        strdata.extend_from_slice(name.as_bytes());
        strdata.push(0);
    }
    let (mut texdata, mut texinfo) = (Vec::new(), Vec::new());
    for (i, flags) in [(0, 0), (1, 0), (2, 0x80)] {
        f32s(&mut texdata, &[0.5, 0.5, 0.5]);
        i32s(&mut texdata, &[i, 64, 64, 64, 64]);
        f32s(&mut texinfo, &[0.0; 16]);
        i32s(&mut texinfo, &[flags, i]);
    }
    // Four 64x64 quads: world GREY z 0, TOOLS z 8, NODRAW z 16, then model 1's GREY z 0.
    let (mut verts, mut edges, mut surf, mut faces) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    edges.extend_from_slice(&[0u8; 4]);
    let mut vert_count = 0u16;
    let mut edge_count = 1i32;
    let quads = [(0.0, 0i16), (8.0, 1), (16.0, 2), (0.0, 0)];
    for (n, (z, texture)) in quads.into_iter().enumerate() {
        let texture = if opts.only_tools { 1 } else { texture };
        for (x, y) in [(0.0, 0.0), (64.0, 0.0), (64.0, 64.0), (0.0, 64.0)] {
            f32s(&mut verts, &[x, y, z]);
        }
        let first = (surf.len() / 4) as i32;
        for i in 0..4u16 {
            edges.extend_from_slice(&(vert_count + i).to_le_bytes());
            edges.extend_from_slice(&(vert_count + (i + 1) % 4).to_le_bytes());
            i32s(&mut surf, &[edge_count]);
            edge_count += 1;
        }
        vert_count += 4;
        let plane: u16 = if opts.bad_plane && n == 0 { 7 } else { 0 };
        faces.extend_from_slice(&plane.to_le_bytes());
        faces.extend_from_slice(&[0, 0]);
        i32s(&mut faces, &[first]);
        i16s(&mut faces, &[4, texture, -1, -1]);
        faces.extend_from_slice(&[0; 4]);
        i32s(&mut faces, &[-1]);
        f32s(&mut faces, &[0.0]);
        i32s(&mut faces, &[0, 0, 0, 0, -1]);
        faces.extend_from_slice(&[0; 8]);
    }
    let mut planes = Vec::new();
    f32s(&mut planes, &[0.0, 0.0, 1.0, 0.0]);
    i32s(&mut planes, &[2]);
    let mut models = Vec::new();
    f32s(
        &mut models,
        &[0.0, 0.0, 0.0, 64.0, 64.0, 16.0, 0.0, 0.0, 0.0],
    );
    i32s(&mut models, &[0, 0, 3]);
    f32s(
        &mut models,
        &[0.0, 0.0, 0.0, 64.0, 64.0, 0.0, 0.0, 0.0, 0.0],
    );
    i32s(&mut models, &[0, 3, 1]);
    let mut nodes = Vec::new();
    i32s(&mut nodes, &[0, -1, -1]);
    i16s(&mut nodes, &[0, 0, 0, 64, 64, 16]);
    nodes.extend_from_slice(&0u16.to_le_bytes());
    nodes.extend_from_slice(&3u16.to_le_bytes());
    i16s(&mut nodes, &[0, 0]);
    let mut ents = String::from("{\n\"classname\" \"worldspawn\"\n}\n");
    ents += &format!(
        "{{\n\"classname\" \"func_brush\"\n\"model\" \"*1\"\n\"origin\" \"{}\"\n\"angles\" \"{}\"\n}}\n",
        opts.brush.0, opts.brush.1
    );
    for (origin, angles) in &opts.spawns {
        ents += &format!(
            "{{\n\"classname\" \"info_player_teamspawn\"\n\"origin\" \"{origin}\"\n\"angles\" \"{angles}\"\n}}\n"
        );
    }
    let mut ents = ents.into_bytes();
    ents.push(0);
    let mut empty_zip = b"PK\x05\x06".to_vec();
    empty_zip.extend_from_slice(&[0; 18]);
    let lumps: Vec<(usize, Vec<u8>)> = vec![
        (0, ents),
        (1, planes),
        (2, texdata),
        (3, verts),
        (5, nodes),
        (6, texinfo),
        (7, faces),
        (10, vec![0; 56]),
        (12, edges),
        (13, surf),
        (14, models),
        (40, empty_zip),
        (43, strdata),
        (44, table),
    ];
    // Lumps follow the header in index order, each padded to 4 bytes, then the game lump, whose
    // `sprp` entry points at its own absolute offset.
    let header_len = 8 + 64 * 16 + 4;
    let mut dirs = [(0i32, 0i32); 64];
    let mut body = Vec::new();
    let mut place = |index: usize, data: &[u8], body: &mut Vec<u8>| {
        dirs[index] = ((header_len + body.len()) as i32, data.len() as i32);
        body.extend_from_slice(data);
        body.resize(body.len().next_multiple_of(4), 0);
    };
    for (index, data) in &lumps {
        place(*index, data, &mut body);
    }
    let sprp_offset = (header_len + body.len() + 4 + 16) as i32;
    let mut game = Vec::new();
    i32s(&mut game, &[1, i32::from_be_bytes(*b"sprp")]);
    game.extend_from_slice(&0u16.to_le_bytes());
    game.extend_from_slice(&10u16.to_le_bytes());
    i32s(&mut game, &[sprp_offset, 12, 0, 0, 0]);
    place(35, &game, &mut body);
    let mut out = b"VBSP".to_vec();
    i32s(&mut out, &[20]);
    for (offset, len) in dirs {
        i32s(&mut out, &[offset, len, 0, 0]);
    }
    i32s(&mut out, &[1]);
    out.extend_from_slice(&body);
    out
}

/// A 1072-byte HL2DEMO header naming `map`, `playback_seconds` long.
pub fn demo_header(map: &str, playback_seconds: f32) -> Vec<u8> {
    let mut out = b"HL2DEMO\0".to_vec();
    i32s(&mut out, &[3, 24]);
    for text in ["127.0.0.1:27015", "player", map, "tf"] {
        let mut field = text.as_bytes().to_vec();
        field.resize(260, 0);
        out.extend_from_slice(&field);
    }
    f32s(&mut out, &[playback_seconds]);
    i32s(&mut out, &[(playback_seconds * 66.0) as i32, 1000, 0]);
    out
}
