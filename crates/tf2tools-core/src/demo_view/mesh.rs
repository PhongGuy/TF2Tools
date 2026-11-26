//! A BSP's drawable faces as a flat triangle list, brush entities placed where the map puts them.

use std::path::Path;

use super::ray_grid::{CELL, MAX_GRID_ENTRIES, RayGrid};
use crate::error::MapMeshError;
use crate::panic_mark::catching;

/// Most triangles a map may hold before it is refused.
pub const MAX_MAP_TRIANGLES: usize = 1_000_000;
/// Largest `.bsp` file read.
pub const MAX_MAP_BYTES: u64 = 512 * 1024 * 1024;

/// A map as a triangle list with one flat normal per vertex.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct MapMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    start: Option<([f32; 3], f32)>,
    grid: Option<RayGrid>,
}

impl MapMesh {
    /// Vertex positions, three per triangle.
    pub fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }

    /// One normal per vertex: its face's plane normal.
    pub fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.positions.len() / 3
    }

    /// The `(min, max)` corners over every position.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in &self.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        (min, max)
    }

    /// Origin and yaw in degrees of the first `info_player_teamspawn`, if the map has one.
    pub fn start(&self) -> Option<([f32; 3], f32)> {
        self.start
    }

    /// Distance along unit `dir` to the nearest triangle within `max_distance`; None on a miss,
    /// a bad input (non-finite, |dir| not within 1e-3 of 1, max_distance not > 0) or no grid.
    pub fn ray_hit(&self, origin: [f32; 3], dir: [f32; 3], max_distance: f32) -> Option<f32> {
        let finite = origin.iter().chain(&dir).all(|v| v.is_finite()) && max_distance.is_finite();
        let unit =
            ((dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt() - 1.).abs() <= 1e-3;
        if !(finite && unit && max_distance > 0.) {
            return None;
        }
        self.grid
            .as_ref()?
            .hit(&self.positions, origin, dir, max_distance)
    }

    /// The ray grid's heap bytes; 0 with no grid.
    pub fn grid_bytes(&self) -> usize {
        self.grid.as_ref().map_or(0, RayGrid::heap_bytes)
    }

    /// A mesh over bare positions with zero normals, no start and its grid built.
    #[cfg(test)]
    pub(crate) fn from_positions(positions: Vec<[f32; 3]>) -> MapMesh {
        MapMesh {
            normals: vec![[0.0; 3]; positions.len()],
            grid: RayGrid::build(&positions, CELL, MAX_GRID_ENTRIES),
            positions,
            start: None,
        }
    }
}

// Parses up to three space-separated floats; a missing or unparsable component is zero.
fn vec3(text: Option<&str>) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (slot, part) in out.iter_mut().zip(text.unwrap_or("").split_whitespace()) {
        *slot = part.parse().unwrap_or(0.0);
    }
    out
}

// Source's AngleMatrix: pitch, yaw, roll in degrees, as three rows.
fn angle_matrix(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let [sp, sy, sr] = angles.map(|a| a.to_radians().sin());
    let [cp, cy, cr] = angles.map(|a| a.to_radians().cos());
    [
        [cp * cy, sr * sp * cy - cr * sy, cr * sp * cy + sr * sy],
        [cp * sy, sr * sp * sy + cr * cy, cr * sp * sy - sr * cy],
        [-sp, sr * cp, cr * cp],
    ]
}

fn rotate(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

fn is_tools(name: &str) -> bool {
    name.get(..6)
        .is_some_and(|p| p.eq_ignore_ascii_case("tools/"))
}

fn build(bsp: &[u8], max_triangles: usize) -> Result<MapMesh, MapMeshError> {
    let bsp = vbsp::Bsp::read(bsp).map_err(|e| MapMeshError::Unreadable(e.to_string()))?;

    // Brush model `*i` takes the origin and angles of the entity that names it.
    let placement = |index: usize| -> ([f32; 3], [[f32; 3]; 3]) {
        let wanted = format!("*{index}");
        bsp.entities
            .iter()
            .find(|e| e.prop("model") == Some(wanted.as_str()))
            .map_or(([0.0; 3], angle_matrix([0.0; 3])), |e| {
                (vec3(e.prop("origin")), angle_matrix(vec3(e.prop("angles"))))
            })
    };

    let (mut positions, mut normals) = (Vec::new(), Vec::new());
    for (index, model) in bsp.models().enumerate() {
        // Model 0 is the world and stays where it is.
        let (origin, matrix) = if index == 0 {
            ([0.0; 3], angle_matrix([0.0; 3]))
        } else {
            placement(index)
        };
        for face in model.faces() {
            if !face.is_visible() || is_tools(face.texture().name()) {
                continue;
            }
            let normal = rotate(&matrix, {
                let n = face.normal();
                [n.x, n.y, n.z]
            });
            for v in face.vertex_positions() {
                let p = rotate(&matrix, [v.x, v.y, v.z]);
                positions.push([p[0] + origin[0], p[1] + origin[1], p[2] + origin[2]]);
                normals.push(normal);
            }
            if positions.len() / 3 > max_triangles {
                return Err(MapMeshError::TooLarge);
            }
        }
    }
    if positions.is_empty() {
        return Err(MapMeshError::Empty);
    }

    let start = bsp
        .entities
        .iter()
        .find(|e| e.prop("classname") == Some("info_player_teamspawn"))
        .map(|e| (vec3(e.prop("origin")), vec3(e.prop("angles"))[1]));
    let grid = RayGrid::build(&positions, CELL, MAX_GRID_ENTRIES);
    Ok(MapMesh {
        positions,
        normals,
        start,
        grid,
    })
}

fn load_capped(bsp: &[u8], max_triangles: usize) -> Result<MapMesh, MapMeshError> {
    // vbsp unwraps on corrupt indices, so a panic is an unreadable map, not a crash.
    catching(|| build(bsp, max_triangles))
        .unwrap_or_else(|_| Err(MapMeshError::Unreadable("the map reader panicked".into())))
}

/// Builds the mesh of a `.bsp` held in memory. Errors on a map vbsp cannot read (or panics on),
/// a map with nothing drawn, or more than [`MAX_MAP_TRIANGLES`] triangles.
pub fn load_map_mesh(bsp: &[u8]) -> Result<MapMesh, MapMeshError> {
    load_capped(bsp, MAX_MAP_TRIANGLES)
}

/// Reads a `.bsp` file of at most [`MAX_MAP_BYTES`] and builds its mesh.
pub fn load_map_file(path: &Path) -> Result<MapMesh, MapMeshError> {
    load_map_file_capped(path, MAX_MAP_BYTES, MAX_MAP_TRIANGLES)
}

/// [`load_map_file`] with both caps given, so tests need not build a huge map.
#[doc(hidden)]
pub fn load_map_file_capped(
    path: &Path,
    max_bytes: u64,
    max_triangles: usize,
) -> Result<MapMesh, MapMeshError> {
    let unreadable = |e: std::io::Error| MapMeshError::Unreadable(e.to_string());
    if std::fs::metadata(path).map_err(unreadable)?.len() > max_bytes {
        return Err(MapMeshError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(unreadable)?;
    load_capped(&bytes, max_triangles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::ray_grid::{CELL, RayGrid};

    #[test]
    fn a_map_past_the_grid_cap_has_no_ray_test() {
        let p = vec![
            [-1000., -1000., 0.],
            [1000., -1000., 0.],
            [1000., 1000., 0.],
        ];
        let mesh = MapMesh {
            grid: RayGrid::build(&p, CELL, 1),
            normals: vec![[0., 0., 1.]; 3],
            positions: p,
            start: None,
        };
        assert_eq!(mesh.grid_bytes(), 0);
        assert_eq!(mesh.ray_hit([500., -500., 50.], [0., 0., -1.], 96.), None);
        let built = MapMesh::from_positions(mesh.positions.clone());
        assert!(
            built
                .ray_hit([500., -500., 50.], [0., 0., -1.], 96.)
                .is_some()
        );
    }
}
