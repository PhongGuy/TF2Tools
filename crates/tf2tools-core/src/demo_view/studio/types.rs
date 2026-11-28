//! The owned, checked parts of a class rig that posing reads: skeleton, pose parameters and
//! sequences, with every index already remapped into the merged tables.

use super::anim::BonePose;

/// `STUDIO_LOOPING`: the sequence wraps instead of holding its last frame.
const STUDIO_LOOPING: i32 = 0x1;

/// A rig's bones in parent-first order.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Skeleton {
    pub(crate) bones: Vec<RigBone>,
}

impl Skeleton {
    /// Every bone, parents before their children.
    pub fn bones(&self) -> &[RigBone] {
        &self.bones
    }

    /// The index of the bone with this name, ignoring ASCII case.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.bones
            .iter()
            .position(|b| b.name.eq_ignore_ascii_case(name))
    }
}

/// One bone of a rig: its name, parent, bind pose and inverse bind matrix.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RigBone {
    pub(crate) name: String,
    pub(crate) parent: Option<usize>,
    pub(crate) bind: BonePose,
    pub(crate) pose_to_bone: [[f32; 4]; 3],
}

impl RigBone {
    /// The bone's name as the model spells it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The parent bone's index, or None for a root.
    pub fn parent(&self) -> Option<usize> {
        self.parent
    }

    /// The bone's rest transform relative to its parent.
    pub fn bind(&self) -> BonePose {
        self.bind
    }

    /// The 3x4 matrix taking model space at bind into this bone's space.
    pub fn pose_to_bone(&self) -> [[f32; 4]; 3] {
        self.pose_to_bone
    }
}

/// A pose parameter after merging the include chain: its name and value range.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PoseParam {
    pub(crate) name: String,
    pub(crate) start: f32,
    pub(crate) end: f32,
    pub(crate) loop_range: f32,
}

impl PoseParam {
    /// The parameter's name, such as `body_pitch`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value the parameter's 0 maps to.
    pub fn start(&self) -> f32 {
        self.start
    }

    /// The value the parameter's 1 maps to.
    pub fn end(&self) -> f32 {
        self.end
    }

    /// The period a looping parameter wraps at, or 0 when it does not loop.
    pub fn loop_range(&self) -> f32 {
        self.loop_range
    }
}

/// A sequence a sequence plays on top of itself, with its fade window.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct AutoLayer {
    pub(crate) sequence: usize,
    pub(crate) pose: Option<usize>,
    pub(crate) flags: i32,
    pub(crate) start: f32,
    pub(crate) peak: f32,
    pub(crate) tail: f32,
    pub(crate) end: f32,
}

impl AutoLayer {
    /// The merged index of the layered sequence.
    pub fn sequence(&self) -> usize {
        self.sequence
    }

    /// The merged pose parameter that drives the layer's weight, if any.
    pub fn pose(&self) -> Option<usize> {
        self.pose
    }

    /// The layer's `STUDIO_AL_*` flags.
    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// Where the layer starts fading in.
    pub fn start(&self) -> f32 {
        self.start
    }

    /// Where the layer reaches full weight.
    pub fn peak(&self) -> f32 {
        self.peak
    }

    /// Where the layer starts fading out.
    pub fn tail(&self) -> f32 {
        self.tail
    }

    /// Where the layer has faded out.
    pub fn end(&self) -> f32 {
        self.end
    }
}

/// A sequence after merging: its blend grid of animations, layers and per-bone weights.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Sequence {
    pub(crate) label: String,
    pub(crate) activity: String,
    pub(crate) flags: i32,
    pub(crate) actweight: i32,
    pub(crate) fade_in: f32,
    pub(crate) fade_out: f32,
    pub(crate) grid: [usize; 2],
    pub(crate) params: [Option<usize>; 2],
    pub(crate) param_start: [f32; 2],
    pub(crate) param_end: [f32; 2],
    pub(crate) pose_keys: Option<[Vec<f32>; 2]>,
    pub(crate) animations: Vec<usize>,
    pub(crate) autolayers: Vec<AutoLayer>,
    pub(crate) bone_weights: Vec<f32>,
}

impl Sequence {
    /// The sequence's name, such as `run_melee`.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The activity the sequence plays for, or empty.
    pub fn activity(&self) -> &str {
        &self.activity
    }

    /// The sequence's `STUDIO_*` flags.
    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// Whether the sequence wraps when it ends.
    pub fn is_looping(&self) -> bool {
        self.flags & STUDIO_LOOPING != 0
    }

    /// The weight for picking this sequence among those of the same activity.
    pub fn actweight(&self) -> i32 {
        self.actweight
    }

    /// Seconds the sequence takes to fade in.
    pub fn fade_in(&self) -> f32 {
        self.fade_in
    }

    /// Seconds the sequence takes to fade out.
    pub fn fade_out(&self) -> f32 {
        self.fade_out
    }

    /// The blend grid's width and height.
    pub fn grid(&self) -> [usize; 2] {
        self.grid
    }

    /// The merged pose parameter on each grid axis.
    pub fn params(&self) -> [Option<usize>; 2] {
        self.params
    }

    /// The parameter value at each axis's first column or row.
    pub fn param_start(&self) -> [f32; 2] {
        self.param_start
    }

    /// The parameter value at each axis's last column or row.
    pub fn param_end(&self) -> [f32; 2] {
        self.param_end
    }

    /// The parameter value at each grid line of an axis, when the sequence lists them.
    pub fn pose_keys(&self, axis: usize) -> Option<&[f32]> {
        self.pose_keys.as_ref()?.get(axis).map(Vec::as_slice)
    }

    /// The merged animation in each grid cell, row by row (`y * grid[0] + x`).
    pub fn animations(&self) -> &[usize] {
        &self.animations
    }

    /// The merged animation at a grid cell, clamping x and y to the grid as the SDK clamps
    /// blend indices; None for an empty grid or a missing cell.
    pub fn animation(&self, x: usize, y: usize) -> Option<usize> {
        let [w, h] = self.grid;
        if w == 0 || h == 0 {
            return None;
        }
        self.animations
            .get(y.min(h - 1) * w + x.min(w - 1))
            .copied()
    }

    /// The layers this sequence plays on top of itself.
    pub fn autolayers(&self) -> &[AutoLayer] {
        &self.autolayers
    }

    /// The sequence's weight per base bone; 0 for a bone its file lacks.
    pub fn bone_weights(&self) -> &[f32] {
        &self.bone_weights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(flags: i32, grid: [usize; 2], animations: Vec<usize>) -> Sequence {
        Sequence {
            label: "run".to_owned(),
            activity: String::new(),
            flags,
            actweight: 0,
            fade_in: 0.2,
            fade_out: 0.2,
            grid,
            params: [None, None],
            param_start: [0.; 2],
            param_end: [0.; 2],
            pose_keys: Some([vec![-1., 1.], vec![90., 45., 0., -45.]]),
            animations,
            autolayers: Vec::new(),
            bone_weights: Vec::new(),
        }
    }

    #[test]
    fn grid_cells_clamp_to_the_grid() {
        let s = seq(0, [3, 2], vec![10, 11, 12, 13, 14, 15]);
        assert_eq!(
            (s.animation(0, 0), s.animation(1, 1), s.animation(2, 1)),
            (Some(10), Some(14), Some(15))
        );
        assert_eq!(
            (s.animation(9, 0), s.animation(0, 9), s.animation(9, 9)),
            (Some(12), Some(13), Some(15)),
            "past the grid clamps to its edge"
        );
        assert_eq!(
            seq(0x800, [0, 0], Vec::new()).animation(0, 0),
            None,
            "a forward declaration plays nothing"
        );
        assert_eq!(
            seq(0, [2, 1], vec![10]).animation(1, 0),
            None,
            "a short cell list never panics"
        );
    }

    #[test]
    fn flags_and_pose_keys_read_back() {
        assert!(seq(0x1, [1, 1], vec![0]).is_looping());
        assert!(!seq(0x800, [1, 1], vec![0]).is_looping());
        let s = seq(0, [1, 1], vec![0]);
        assert_eq!(s.pose_keys(0), Some([-1., 1.].as_slice()));
        assert_eq!(
            (s.pose_keys(1).map(<[f32]>::len), s.pose_keys(2)),
            (Some(4), None)
        );
    }

    #[test]
    fn bones_are_found_by_name_ignoring_case() {
        let bone = |name: &str, parent| RigBone {
            name: name.to_owned(),
            parent,
            bind: BonePose::IDENTITY,
            pose_to_bone: [[0.; 4]; 3],
        };
        let s = Skeleton {
            bones: vec![bone("bip_pelvis", None), bone("bip_head", Some(0))],
        };
        assert_eq!(
            (s.find("BIP_HEAD"), s.find("bip_pelvis"), s.find("nope")),
            (Some(1), Some(0), None)
        );
        assert_eq!(s.bones()[1].parent(), Some(0));
    }
}
