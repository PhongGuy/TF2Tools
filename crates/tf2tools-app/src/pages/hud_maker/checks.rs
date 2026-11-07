//! The checking tools' cache: each check runs again only when what it reads has changed, so
//! the views repaint every frame without laying the HUD out again.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tf2tools_core::{
    Backdrop, Change, GameText, HudModel, PartKey, Scene, ScreenBaseline, ScreenCheck,
    ScreenProblem, Vision,
};

use super::editor::HudEditor;

type ScreensKey = (Scene, Vec<Change>);
// Layout generation, backdrop id and view: every new layout has its own generation.
type HardKey = (u64, u64, Vision);
type ScreensResult = (Arc<[ScreenCheck]>, Arc<[FlagLabels]>);

/// One shape's flagged parts, once each in flag order, with the words their flag shows.
pub type FlagLabels = Vec<(PartKey, String)>;

/// Every screen shape's check and the hard-to-read labels for what an editor shows, each
/// worked out again only when what it reads has changed.
#[derive(Default)]
pub struct Checks {
    // The game's text, which `#token` labels draw; the screen check measures it.
    text: Arc<GameText>,
    // The HUD as opened changes only with the scene, so its baseline is kept per scene.
    baseline: Option<(Scene, ScreenBaseline)>,
    screens: Option<(ScreensKey, ScreensResult)>,
    hard: Option<(HardKey, Arc<HashSet<PartKey>>)>,
    screen_runs: usize,
    hard_runs: usize,
}

impl Checks {
    /// An empty cache for a HUD whose `#token` labels draw `text`.
    pub fn new(text: Arc<GameText>) -> Checks {
        Checks {
            text,
            ..Checks::default()
        }
    }

    /// `HudModel::check_screens` on the shown model against the pristine model's baseline
    /// (built once per scene); re-run only when the scene or the shown changes differ from the
    /// last run.
    pub fn screens(&mut self, editor: &HudEditor) -> Arc<[ScreenCheck]> {
        self.screens_with_labels(editor).0
    }

    /// `screens` with each shape's flag labels, worked out with the check rather than per frame.
    pub fn screens_with_labels(&mut self, editor: &HudEditor) -> ScreensResult {
        let scene = editor.scene();
        if let Some(((s, changes), result)) = &self.screens
            && *s == scene
            && changes.as_slice() == editor.shown_changes()
        {
            return result.clone();
        }
        let baseline = match &mut self.baseline {
            Some((s, baseline)) if *s == scene => baseline,
            slot => {
                &mut slot
                    .insert((scene, editor.pristine().screen_baseline(&self.text)))
                    .1
            }
        };
        let result: Arc<[ScreenCheck]> = editor.model().check_screens(baseline, &self.text).into();
        let labels: Arc<[FlagLabels]> = (result.iter())
            .map(|check| flag_labels(check, editor.model()))
            .collect();
        self.screen_runs += 1;
        let key = (scene, editor.shown_changes().to_vec());
        self.screens = Some((key, (result.clone(), labels.clone())));
        (result, labels)
    }

    /// `HudModel::hard_to_read` at the editor's screen; re-run only when the editor has laid out
    /// again or `backdrop_id` or `vision` differ from the last run.
    pub fn hard_to_read(
        &mut self,
        editor: &HudEditor,
        backdrop: &Backdrop,
        backdrop_id: u64,
        vision: Vision,
    ) -> Arc<HashSet<PartKey>> {
        let key = (editor.layout().generation(), backdrop_id, vision);
        if let Some((last, result)) = &self.hard
            && *last == key
        {
            return result.clone();
        }
        let found = editor
            .model()
            .hard_to_read(editor.layout(), backdrop, vision);
        let result = Arc::new(found.into_iter().collect::<HashSet<_>>());
        self.hard_runs += 1;
        self.hard = Some((key, result.clone()));
        result
    }

    /// How many times the screen check has run.
    pub fn screen_runs(&self) -> usize {
        self.screen_runs
    }

    /// How many times the hard-to-read check has run.
    pub fn hard_runs(&self) -> usize {
        self.hard_runs
    }
}

/// Each flagged part once, in flag order, with its problems joined in plain words.
fn flag_labels(check: &ScreenCheck, model: &HudModel) -> FlagLabels {
    let name = |key: &PartKey| {
        model
            .part(key)
            .map_or(key.as_str(), |p| p.name())
            .to_owned()
    };
    let mut labels: FlagLabels = Vec::new();
    let mut at: HashMap<&PartKey, usize> = HashMap::new();
    for flag in check.flags() {
        let part = flag.part();
        let words = match flag.problem() {
            ScreenProblem::Overlaps(other) => format!("{} overlaps {}", name(part), name(other)),
            ScreenProblem::LeavesScreen | _ => format!("{} goes off the screen", name(part)),
        };
        match at.get(part) {
            Some(&i) => {
                labels[i].1.push_str(". ");
                labels[i].1.push_str(&words);
            }
            None => {
                at.insert(part, labels.len());
                labels.push((part.clone(), words));
            }
        }
    }
    labels
}
