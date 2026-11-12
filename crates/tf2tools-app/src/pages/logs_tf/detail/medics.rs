//! The open log's Medic blocks: each Medic's healing and uber rows as logs.tf words them, then
//! the players they healed with class and share.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Styled as _, TestSupportExt as _, div, px,
};
use tf2tools_core::{LogDetail, MedicLine};

use super::{ICON, NUMBER, number, team_cell};
use crate::ids;
use crate::pages::logs_tf::tables::heal_rows;
use crate::pages::logs_tf::text::{charge_text, healing_text, seconds_text, tenths_text};
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::labelled;

// A block's width; in the 800 px window's content column they wrap one to a line.
const MEDIC_WIDTH: Pixels = px(300.);
const CLASS: Pixels = px(40.);

/// One block per Medic in `log`, wrapping onto new lines as the width runs out.
pub(super) fn medics(log: &LogDetail, tracked: &super::Tracked, cx: &App) -> impl IntoElement {
    h_flex()
        .id(ids::LOGSTF_MEDICS)
        .test_support()
        .flex_wrap()
        .items_start()
        .gap_4()
        .children(log.medics().iter().map(|m| medic_block(m, tracked, cx)))
}

// The Medic's name in team colour, the ten rows logs.tf shows, then the heal-target table.
fn medic_block(medic: &MedicLine, tracked: &super::Tracked, cx: &App) -> impl IntoElement {
    let id = medic.steam_id();
    let fact = |label: &str, key: &str, value: String| {
        h_flex()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_owned()),
            )
            .child(labelled(div(), ids::logstf_medic_fact(id, key), value))
    };
    // One Charges row per medigun, the label only on the first, as logs.tf lays them out.
    let total: u32 = medic.charges().iter().map(|(_, n)| n).sum();
    let mut charges: Vec<Div> = medic
        .charges()
        .iter()
        .enumerate()
        .map(|(i, (gun, n))| {
            let label = if i == 0 { "Charges" } else { "" };
            let key = format!("Charges:{}", gun.key());
            fact(label, &key, charge_text(gun, *n, total))
        })
        .collect();
    if charges.is_empty() {
        charges.push(fact("Charges", "Charges", "0".to_owned()));
    }
    let count = |n: u32| n.to_string();
    v_flex()
        .id(ids::logstf_medic(id))
        .test_support()
        .w(MEDIC_WIDTH)
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .gap_2()
                .font_weight(FontWeight::SEMIBOLD)
                .child(team_cell(medic.team(), cx))
                .child(super::name_cell(
                    div().id("logstf-medic-name").min_w_0(),
                    id,
                    medic.name(),
                    tracked,
                    "medic",
                )),
        )
        .child(fact(
            "Healing",
            "Healing",
            healing_text(medic.heal(), medic.heal_per_min()),
        ))
        .children(charges)
        .children(
            [
                ("Drops", count(medic.drops())),
                ("Avg time to build", seconds_text(medic.avg_time_to_build())),
                (
                    "Avg time before using",
                    seconds_text(medic.avg_time_before_using()),
                ),
                ("Near full charge deaths", count(medic.near_full_deaths())),
                ("Avg uber length", tenths_text(medic.avg_uber_length())),
                ("Deaths after charge", count(medic.deaths_after_uber())),
                ("Major advantages lost", count(medic.advantages_lost())),
                (
                    "Biggest advantage lost",
                    seconds_text(f64::from(medic.biggest_advantage_lost())),
                ),
            ]
            .map(|(label, value)| fact(label, label, value)),
        )
        .child(heal_table(medic, tracked, cx))
}

// Heal Target, Class, Heals and a bold share, most healed first.
fn heal_table(medic: &MedicLine, tracked: &super::Tracked, cx: &App) -> Div {
    let assets = &cx.global::<Services>().assets;
    let header = h_flex()
        .gap_1()
        .pt_2()
        .pb_1()
        .text_color(cx.theme().muted_foreground)
        .font_weight(FontWeight::MEDIUM)
        .child(div().flex_1().min_w_0().child("Heal Target"))
        .child(div().w(CLASS).flex_none().child("Class"))
        .child(number("Heals".to_owned()))
        .child(number("%".to_owned()));
    let rows = heal_rows(medic).into_iter().map(|row| {
        let icon = row
            .class
            .map(|c| class_icon(c, assets.class_icon(c).as_deref(), ICON));
        let share = labelled(
            div()
                .w(NUMBER)
                .flex_none()
                .text_right()
                .font_weight(FontWeight::BOLD),
            ids::logstf_heal_target(medic.steam_id(), row.steam_id),
            row.share,
        );
        let line = h_flex().id(ids::logstf_heal_row(medic.steam_id(), row.steam_id));
        super::player_row(line, tracked.is(row.steam_id), cx)
            .test_support()
            .gap_1()
            .py_0p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(super::name_cell(
                div()
                    .id(ids::logstf_heal_name(medic.steam_id(), row.steam_id))
                    .flex_1()
                    .min_w_0()
                    // Kept clear of the class icon after a cut-off name.
                    .mr_1(),
                row.steam_id,
                &row.name,
                tracked,
                &format!("heal:{}", medic.steam_id().id64()),
            ))
            .child(
                div()
                    .id(ids::logstf_heal_class(medic.steam_id(), row.steam_id))
                    .test_support()
                    .w(CLASS)
                    .flex_none()
                    .children(icon),
            )
            .child(number(row.heals.to_string()))
            .child(share)
    });
    v_flex().child(header).children(rows)
}
