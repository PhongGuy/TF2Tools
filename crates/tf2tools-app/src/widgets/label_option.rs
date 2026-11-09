//! A Select row for a fixed menu of labels, which registers itself for the tests and the pilot.

use gpui_kit::component::select::SelectItem;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    TestSupportExt as _, Window, div,
};

use crate::ids;

/// One label of a Select whose value is the label itself.
#[derive(Debug, Clone)]
pub struct LabelOption {
    label: &'static str,
    id: ElementId,
}

impl LabelOption {
    /// The rows for `labels`, in order, each keyed under `select`, the id of the Select's wrapper.
    pub fn all(select: &str, labels: impl IntoIterator<Item = &'static str>) -> Vec<LabelOption> {
        labels
            .into_iter()
            .map(|label| LabelOption {
                label,
                id: ids::select_option(select, label),
            })
            .collect()
    }
}

// Kit rows carry no test id, so each row draws its own for headless clicks and the pilot.
impl SelectItem for LabelOption {
    type Value = &'static str;

    fn title(&self) -> SharedString {
        self.label.into()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().id(self.id.clone()).test_support().child(self.label)
    }

    fn value(&self) -> &&'static str {
        &self.label
    }
}
