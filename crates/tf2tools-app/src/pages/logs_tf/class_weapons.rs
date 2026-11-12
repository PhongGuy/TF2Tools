//! The class weapons panel under the averages grid: the selected class's weapon sums over the
//! history range, in the open log's class card layout.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity,
    div, px,
};
use tf2tools_core::{ClassWeapons, LogsRange, TfClass, WeaponNames};

use super::LogsTfPage;
use super::tables::class_weapon_rows;
use super::text::{NO_WEAPONS_TEXT, counting_text, weapons_title};
use crate::ids;
use crate::widgets::scroll::sideways;
use crate::widgets::{Named as _, labelled};

// The name column keeps room for long logs.tf names; the rest fit "14029 (100%)" and
// their own title on one line.
const NAME: Pixels = px(200.);
const KILLS: Pixels = px(80.);
const DAMAGE: Pixels = px(104.);
const ACCURACY: Pixels = px(80.);

/// The panel for `weapons`, read for `class` over `range` and named by `names`; its close button
/// closes it on `page`.
pub(super) fn class_weapons_panel(
    class: TfClass,
    weapons: &ClassWeapons,
    names: &WeaponNames,
    range: LogsRange,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    let note = if weapons.pending() > 0 {
        Some(counting_text(weapons.pending()))
    } else if weapons.weapons().is_empty() {
        Some(NO_WEAPONS_TEXT.to_owned())
    } else {
        None
    };
    let header = h_flex()
        .gap_2()
        .child(labelled(
            div().flex_1().min_w_0().font_weight(FontWeight::SEMIBOLD),
            ids::LOGSTF_CLASS_WEAPONS_TITLE,
            weapons_title(class, range, weapons.logs()),
        ))
        .child(
            Button::new(ids::LOGSTF_CLASS_WEAPONS_CLOSE)
                .ghost()
                .xsmall()
                .icon(IconName::X)
                .named(format!("Close {} weapons", class.name()))
                .on_click(move |_, _, cx| {
                    let _ = page.update(cx, |page, cx| page.close_class(cx));
                }),
        );
    v_flex()
        .id(ids::LOGSTF_CLASS_WEAPONS)
        .test_support()
        .w_full()
        .gap_2()
        .p_3()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .child(header)
        .children(note.map(|note| {
            labelled(
                div().text_sm().text_color(muted),
                ids::LOGSTF_CLASS_WEAPONS_NOTE,
                note,
            )
        }))
        .children((!weapons.weapons().is_empty()).then(|| {
            sideways(
                ids::LOGSTF_CLASS_WEAPONS_TABLE,
                weapon_table(class, weapons, names, cx),
            )
        }))
}

// The column titles, then one line per weapon labelled as the class card's rows are.
fn weapon_table(
    class: TfClass,
    weapons: &ClassWeapons,
    names: &WeaponNames,
    cx: &App,
) -> impl IntoElement + use<> {
    let border = cx.theme().border;
    let cell = |width: Pixels| div().w(width).flex_none().text_right();
    let head = h_flex()
        .id(ids::LOGSTF_CLASS_WEAPONS_HEAD)
        .test_support()
        .gap_2()
        .whitespace_nowrap()
        .text_color(cx.theme().muted_foreground)
        .font_weight(FontWeight::MEDIUM)
        .child(div().flex_1().min_w(NAME).child("Weapon"))
        .child(cell(KILLS).child("Kills"))
        .child(cell(DAMAGE).child("Damage"))
        .child(cell(ACCURACY).child("Accuracy"));
    let rows = class_weapon_rows(weapons, names).into_iter().map(|w| {
        let kills = format!("{} ({})", w.kills, w.kill_share);
        let dmg = format!("{} ({})", w.dmg, w.dmg_share);
        let label: SharedString = format!("{} {kills} {dmg} {}", w.name, w.accuracy).into();
        h_flex()
            .id(ids::logstf_class_weapon(class, &w.raw))
            .aria_label(label)
            .test_support()
            .gap_2()
            .py_0p5()
            .border_t_1()
            .border_color(border)
            .child(div().flex_1().min_w(NAME).child(w.name))
            .child(cell(KILLS).child(kills))
            .child(cell(DAMAGE).child(dmg))
            .child(cell(ACCURACY).child(w.accuracy))
    });
    let rows: Vec<_> = rows.collect();
    v_flex().w_full().text_sm().child(head).children(rows)
}
