//! The demo viewer's UI-free pieces: the map mesh drawn behind a demo, the free-fly camera
//! and the demo header.

mod free_fly;
mod header;
mod mesh;

pub use free_fly::{FlyInput, FreeFly};
pub use header::{DemoHeader, read_demo_header};
pub use mesh::{
    MAX_MAP_BYTES, MAX_MAP_TRIANGLES, MapMesh, load_map_file, load_map_file_capped, load_map_mesh,
};
