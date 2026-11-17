//! A Select row for a fixed menu of values, which registers itself for the tests and the pilot.

use gpui_kit::component::select::SelectItem;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    TestSupportExt as _, Window, div,
};

use crate::ids;

/// One row of a Select: the value it answers and the label it shows, which may be translated.
#[derive(Debug, Clone)]
pub struct LabelOption {
    value: &'static str,
    label: SharedString,
    id: ElementId,
}

impl LabelOption {
    /// The rows for `labels`, in order, each its own value, keyed under `select`, the id of the
    /// Select's wrapper.
    pub fn all(select: &str, labels: impl IntoIterator<Item = &'static str>) -> Vec<LabelOption> {
        Self::keyed(
            select,
            labels.into_iter().map(|value| (value, value.into())),
        )
    }

    /// The rows for `(value, label)` pairs, in order; each row's id comes from its value, so it
    /// stays the same in every language.
    pub fn keyed(
        select: &str,
        rows: impl IntoIterator<Item = (&'static str, SharedString)>,
    ) -> Vec<LabelOption> {
        rows.into_iter()
            .map(|(value, label)| LabelOption {
                value,
                label,
                id: ids::select_option(select, value),
            })
            .collect()
    }
}

// Kit rows carry no test id, so each row draws its own for headless clicks and the pilot.
impl SelectItem for LabelOption {
    type Value = &'static str;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(self.id.clone())
            .test_support()
            .child(self.label.clone())
    }

    fn value(&self) -> &&'static str {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::SharedString;
    use gpui_kit::component::select::SelectItem as _;

    use super::LabelOption;

    #[test]
    fn a_keyed_row_shows_its_label_and_answers_its_value() {
        let rows = LabelOption::keyed(
            "demo-select",
            [
                ("Name", SharedString::from("Название")),
                ("Most played", "Чаще всего".into()),
            ],
        );
        let shown: Vec<(SharedString, &str)> =
            rows.iter().map(|r| (r.title(), *r.value())).collect();
        assert_eq!(
            shown,
            [
                (SharedString::from("Название"), "Name"),
                (SharedString::from("Чаще всего"), "Most played")
            ]
        );
        let plain = LabelOption::all("demo-select", ["Dark"]);
        assert_eq!(
            (plain[0].title(), *plain[0].value()),
            (SharedString::from("Dark"), "Dark")
        );
    }
}
