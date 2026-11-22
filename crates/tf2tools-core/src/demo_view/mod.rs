//! The demo viewer's UI-free pieces: the map mesh drawn behind a demo.

mod mesh;

pub use mesh::{
    MAX_MAP_BYTES, MAX_MAP_TRIANGLES, MapMesh, load_map_file, load_map_file_capped, load_map_mesh,
};
