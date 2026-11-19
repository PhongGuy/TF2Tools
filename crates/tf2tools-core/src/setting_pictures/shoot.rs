//! The pure parts of the setting pictures shooter: cfg text, log markers, launch
//! arguments, per-value action lists and the modules.cfg that applies the base at launch.
//! The `shoot_setting_pictures` example runs them.

use super::{Crop, PictureManifest, Reload, SettingShot};

/// The RMSE (0-255 scale) at or under which two shots count as the same look.
pub const SAME_LOOK_RMSE: f64 = 0.5;

/// One thing the shooter does, in order, to take a picture.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShotAction {
    /// Console commands to run as one cfg step.
    Run(Vec<String>),
    /// Wait this many milliseconds.
    Sleep(u64),
    /// Join the server again after a map change.
    Rejoin,
    /// Restart the game.
    Restart,
    /// Take a jpeg screenshot with this name.
    Jpeg(String),
}

/// The cfg the game execs at launch; it arms the loop that polls for step files.
/// mastercomfig aliases forward commands to the server, so the command quota is raised, and
/// team balance and respawn waits are off so stock-item puppet bots can join any team.
pub fn start_cfg() -> String {
    "echo TFS_START\nsv_cheats 1\nsv_quota_stringcmdspersecond 1000\n\
     mp_teams_unbalance_limit 0\nmp_autoteambalance 0\nmp_disable_respawn_times 1\nr_drawviewmodel 1\n\
     cl_disablehtmlmotd 1\n\
     alias tfs_loop \"tfs_poll\"\nalias tfs_poll \"exec tfs/step_1; wait 30; tfs_loop\"\ntfs_loop\n"
        .to_owned()
}

/// The cfg for step `n`: it points the loop at step `n + 1`, runs `commands`, then echoes its marker.
pub fn step_cfg(n: u32, commands: &[String]) -> String {
    let mut cfg = format!(
        "alias tfs_poll \"exec tfs/step_{}; wait 30; tfs_loop\"\n",
        n + 1
    );
    for command in commands {
        cfg.push_str(command);
        cfg.push('\n');
    }
    cfg.push_str(&format!("echo TFS_DONE_{n}\n"));
    cfg
}

/// Whether the console log holds step `n`'s marker as a whole line, so `_12` never matches `_1`.
pub fn marker_seen(log: &str, n: u32) -> bool {
    let marker = format!("TFS_DONE_{n}");
    log.lines().any(|line| line.trim() == marker)
}

/// Whether the console log shows the server dropping the player, such as for too many commands.
pub fn kick_seen(log: &str) -> bool {
    log.lines().map(str::trim).any(|line| {
        line.starts_with("Disconnect:") || line == "Issued too many commands to server."
    })
}

/// The Steam arguments for a private, windowed, silent local server on `map`, opened on the second display.
pub fn launch_args(map: &str) -> Vec<String> {
    [
        "-applaunch",
        "440",
        "-insecure",
        "-windowed",
        "-noborder",
        "-sdl_displayindex",
        "1",
        "-w",
        "1440",
        "-h",
        "810",
        "-condebug",
        "+volume",
        "0",
        "+sv_lan",
        "1",
        "+maxplayers",
        "1",
        "+sv_quota_stringcmdspersecond",
        "1000",
        "+map",
        map,
        "+exec",
        "tfs/start",
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect()
}

/// The commands that put the shooter on BLU as Soldier, whose stock Rocket Launcher shows
/// in first person, with the HUD hidden.
pub fn join_commands() -> Vec<String> {
    [
        "jointeam blue",
        "joinclass soldier",
        "cl_drawhud 0",
        "r_drawviewmodel 1",
    ]
    .iter()
    .map(|c| (*c).to_owned())
    .collect()
}

/// The screenshot name for a module's value.
pub fn shot_name(module: &str, value: &str) -> String {
    format!("tfs_{module}_{value}")
}

/// Places the camera and runs the setting's setup; the base look is applied at launch.
pub fn setting_actions(_manifest: &PictureManifest, setting: &SettingShot) -> Vec<ShotAction> {
    let mut place = Vec::new();
    if let Some([x, y, z]) = setting.setpos {
        place.push(format!("setpos {x} {y} {z}"));
    }
    if let Some([p, y, r]) = setting.setang {
        place.push(format!("setang {p} {y} {r}"));
    }
    place.extend(setting.setup.iter().cloned());
    vec![ShotAction::Run(place), ShotAction::Sleep(1000)]
}

/// Puts a live-reloaded setting's module back on its base value so the next setting starts clean.
pub fn reset_actions(manifest: &PictureManifest, setting: &SettingShot) -> Vec<ShotAction> {
    if setting.reload == Reload::Restart {
        return Vec::new();
    }
    match manifest.base.get(&setting.module) {
        Some(base) => vec![ShotAction::Run(vec![format!("{}_{base}", setting.module)])],
        None => Vec::new(),
    }
}

/// The modules.cfg text mastercomfig reads at startup: the base, with `value` (module, value)
/// replacing that module's base entry or added last when the module has none.
pub fn modules_cfg(manifest: &PictureManifest, value: Option<(&str, &str)>) -> String {
    let mut cfg = String::new();
    for (module, base) in &manifest.base {
        let shown = match value {
            Some((m, v)) if m == module => v,
            _ => base,
        };
        cfg.push_str(&format!("{module}={shown}\n"));
    }
    if let Some((m, v)) = value
        && !manifest.base.contains_key(m)
    {
        cfg.push_str(&format!("{m}={v}\n"));
    }
    cfg
}

/// Everything the shooter does to apply one value of `setting` and take its picture.
pub fn value_actions(
    manifest: &PictureManifest,
    setting: &SettingShot,
    value: &str,
) -> Vec<ShotAction> {
    let alias = format!("{}_{value}", setting.module);
    let map = setting.map.as_deref().unwrap_or(&manifest.map);
    let mut actions = match setting.reload {
        Reload::None => vec![ShotAction::Run(vec![alias])],
        Reload::Materials => {
            vec![ShotAction::Run(vec![
                alias,
                "mat_reloadallmaterials".to_owned(),
            ])]
        }
        Reload::Map => {
            let mut a = vec![
                ShotAction::Run(vec![alias.clone()]),
                ShotAction::Run(vec![format!("map {map}")]),
                ShotAction::Rejoin,
            ];
            a.extend(setting_actions(manifest, setting));
            a.push(ShotAction::Run(vec![alias]));
            a
        }
        Reload::Restart => {
            // The executor writes the value into modules.cfg before relaunching; never live.
            let mut a = vec![ShotAction::Restart];
            a.extend(setting_actions(manifest, setting));
            a
        }
    };
    actions.push(ShotAction::Sleep(1000));
    for step in &setting.steps {
        actions.push(ShotAction::Run(vec![step.run.clone()]));
        actions.push(ShotAction::Sleep(step.wait_ms));
    }
    if setting.pause {
        actions.push(ShotAction::Run(vec!["setpause".to_owned()]));
    }
    actions.push(ShotAction::Jpeg(shot_name(&setting.module, value)));
    if setting.pause {
        actions.push(ShotAction::Run(vec!["unpause".to_owned()]));
    }
    actions
}

/// The root-mean-square difference of two equal-length pixel buffers; `None` if empty or unequal.
pub fn rmse(a: &[u8], b: &[u8]) -> Option<f64> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let sum: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| {
            let d = f64::from(*x) - f64::from(*y);
            d * d
        })
        .sum();
    Some((sum / a.len() as f64).sqrt())
}

/// Where to cut a zoom from a screenshot and how large to make it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ZoomGeometry {
    /// The left edge in the screenshot.
    pub x: u32,
    /// The top edge in the screenshot.
    pub y: u32,
    /// The width in the screenshot.
    pub w: u32,
    /// The height in the screenshot.
    pub h: u32,
    /// The zoom's width: twice the crop's.
    pub out_w: u32,
    /// The zoom's height: twice the crop's.
    pub out_h: u32,
}

/// Scales `crop` from picture coordinates (`size`) to a `source` frame of another size, so the
/// zoom is cut from the full-resolution screenshot rather than the downscaled picture.
pub fn zoom_geometry(crop: Crop, size: [u32; 2], source: (u32, u32)) -> ZoomGeometry {
    let scale = |v: u32, from: u32, to: u32| -> u32 {
        let from = u64::from(from.max(1));
        let scaled = (u64::from(v) * u64::from(to) + from / 2) / from;
        u32::try_from(scaled).unwrap_or(u32::MAX)
    };
    let [width, height] = size;
    ZoomGeometry {
        x: scale(crop.x, width, source.0),
        y: scale(crop.y, height, source.1),
        w: scale(crop.w, width, source.0),
        h: scale(crop.h, height, source.1),
        out_w: crop.w.saturating_mul(2),
        out_h: crop.h.saturating_mul(2),
    }
}
