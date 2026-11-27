//! The Demo Viewer's kill feed: the rows shown at a tick, their words and their corner.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Role, SharedString, Size, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, div, hsla, px,
};
use tf2tools_core::{DemoKill, DemoTracks, KillParty, Team, TfClass, WeaponNames};

use super::{BAR_MARGIN, BarPlace, DemoViewerPage};
use crate::i18n::tr;
use crate::ids;
use crate::kill_icons::{ICON_SLOT, SLOT_GAP, weapon_icon};

/// The feed's widest; a narrower page takes its width less the bar's margins.
const FEED_WIDTH: Pixels = px(600.);
/// A row's room besides its three names: padding, two gaps, the icon slot and the "+".
const ROW_CHROME: Pixels = px(112.);
/// The widest a name grows before it ends in an ellipsis.
pub const NAME_CAP: Pixels = px(160.);
/// The narrowest a name's cap shrinks to on a small page, so it never reads as a bare ellipsis.
pub const NAME_MIN: Pixels = px(48.);

/// The feed's width and each name's cap on a page `page_width` wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeedLayout {
    pub width: Pixels,
    pub name: Pixels,
}

/// The feed takes the page less the bar's margins up to `FEED_WIDTH`. Each of a row's three
/// names gets the same cap, shrunk on a narrow page so all three and the chrome still fit.
pub fn feed_layout(page_width: Pixels) -> FeedLayout {
    let width = FEED_WIDTH.min((page_width - BAR_MARGIN * 2.).max(px(0.)));
    let share = ((width - ROW_CHROME) / 3.).max(px(0.));
    FeedLayout {
        width,
        name: share.min(NAME_CAP).max(NAME_MIN),
    }
}

/// How long a kill stays in the feed, in demo seconds.
pub const FEED_HOLD_S: f32 = 6.0;
/// The most rows the feed shows at once.
pub const FEED_ROWS: usize = 5;

/// One side of a feed row: the name it shows and the team that colours it.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedParty {
    pub name: SharedString,
    pub team: Option<Team>,
}

/// One kill as the feed draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedRow {
    pub killer: Option<FeedParty>,
    pub assister: Option<FeedParty>,
    pub victim: FeedParty,
    /// The kill-icon key, as `DemoKill::weapon`.
    pub weapon: Option<String>,
    /// The killer's class at the kill, which `KillIconImages::for_weapon` takes.
    pub killer_class: Option<TfClass>,
}

fn window(tracks: &DemoTracks, tick: f64) -> &[DemoKill] {
    tracks.killfeed_at(tick, FEED_HOLD_S, FEED_ROWS)
}

/// What identifies the feed at `tick` without building rows: its length and its newest kill's
/// tick. The window is contiguous and ends after every kill at or before `tick`, so the pair
/// pins it down.
pub fn feed_key(tracks: &DemoTracks, tick: f64) -> (usize, Option<u32>) {
    let kills = window(tracks, tick);
    (kills.len(), kills.last().map(DemoKill::tick))
}

// A party without a name reads as the players list's numbered fallback.
fn party(p: &KillParty) -> FeedParty {
    let name = match p.name().filter(|n| !n.is_empty()) {
        Some(name) => SharedString::from(name.to_owned()),
        None => tr!("demo_viewer.camera.unnamed", entity = p.id().entity()),
    };
    FeedParty {
        name,
        team: p.team(),
    }
}

/// `killfeed_at(tick, FEED_HOLD_S, FEED_ROWS)` as rows, oldest first; a party without a name
/// reads as `demo_viewer.camera.unnamed` with its entity.
pub fn feed_rows(tracks: &DemoTracks, tick: f64) -> Vec<FeedRow> {
    window(tracks, tick)
        .iter()
        .map(|kill| FeedRow {
            killer: kill.killer().map(party),
            assister: kill.assister().map(party),
            victim: party(kill.victim()),
            weapon: kill.weapon().map(str::to_owned),
            killer_class: kill.killer().and_then(KillParty::class),
        })
        .collect()
}

/// The row's translated accessible name; a weapon reads as `names.get(key)`, else the key.
pub fn feed_words(row: &FeedRow, names: &WeaponNames) -> SharedString {
    let victim = row.victim.name.as_ref();
    let Some(killer) = &row.killer else {
        return tr!("demo_viewer.killfeed.died", victim = victim);
    };
    let weapon: SharedString = match row.weapon.as_deref() {
        Some(key) => names.get(key).unwrap_or(key).to_owned().into(),
        None => tr!("demo_viewer.killfeed.unknown_weapon"),
    };
    let (killer, weapon) = (killer.name.as_ref(), weapon.as_ref());
    match &row.assister {
        Some(assister) => tr!(
            "demo_viewer.killfeed.assisted",
            killer = killer,
            assister = assister.name.as_ref(),
            victim = victim,
            weapon = weapon
        ),
        None => tr!(
            "demo_viewer.killfeed.kill",
            killer = killer,
            victim = victim,
            weapon = weapon
        ),
    }
}

/// Whether the feed sits top right: unless the bar has more room below it than above.
pub fn feed_on_top(place: BarPlace, area: Size<Pixels>, bar: Size<Pixels>) -> bool {
    let bar_at = place.clamp(area, bar).y;
    bar_at >= area.height - bar_at - bar.height
}

// A name's colour: its team's, or the text colour for a party with no team.
fn party_colour(team: Option<Team>, cx: &App) -> Hsla {
    match team {
        Some(Team::Red) => cx.theme().red,
        Some(_) => cx.theme().blue,
        None => cx.theme().foreground,
    }
}

// One name, coloured by team and cut with an ellipsis past its cap.
fn name(id: ElementId, party: &FeedParty, cap: Pixels, cx: &App) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .max_w(cap)
        .min_w_0()
        .flex_shrink(1.)
        .truncate()
        .text_color(party_colour(party.team, cx))
        .child(party.name.clone())
}

impl DemoViewerPage {
    /// The kill feed overlay while tracks are loaded: oldest kill on top, in the corner away
    /// from the bar. It has no handlers and no occlusion, so every pointer event reaches the map.
    pub(super) fn killfeed(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let timeline = self.timeline.as_ref()?;
        let area = self.area.get();
        let FeedLayout { width, name: cap } = feed_layout(area.width);
        let names = self.weapon_names.read(cx).names();
        let icons = self.kill_icons.read(cx).icons();
        let rows = timeline.feed.iter().enumerate().map(|(n, row)| {
            let icon = row
                .killer
                .as_ref()
                .and(row.weapon.as_deref())
                .and_then(|weapon| icons.for_weapon(weapon, row.killer_class, names))
                .map(|image| weapon_icon(ids::demo_viewer_kill_icon(n), image));
            let killer = row.killer.as_ref().map(|killer| {
                h_flex()
                    .min_w_0()
                    .child(name(ids::demo_viewer_kill_killer(n), killer, cap, cx))
                    .when_some(row.assister.as_ref(), |el, assister| {
                        let assister = name(ids::demo_viewer_kill_assister(n), assister, cap, cx);
                        el.child(div().flex_none().px_1().child("+"))
                            .child(assister)
                    })
            });
            h_flex()
                .id(ids::demo_viewer_kill(n))
                .role(Role::ListItem)
                .aria_label(timeline.feed_words.get(n).cloned().unwrap_or_default())
                .test_support()
                .max_w_full()
                .overflow_hidden()
                .items_center()
                .gap(SLOT_GAP)
                .px_2()
                .py_0p5()
                .rounded_sm()
                .bg(hsla(0., 0., 0., 0.6))
                .text_sm()
                .children(killer)
                .children(icon.map(|i| h_flex().max_w(ICON_SLOT).flex_none().child(i)))
                .child(name(ids::demo_viewer_kill_victim(n), &row.victim, cap, cx))
        });
        let feed = v_flex()
            .id(ids::DEMO_VIEWER_KILLFEED)
            .role(Role::List)
            .aria_label(tr!("demo_viewer.killfeed.title"))
            .test_support()
            .absolute()
            .right(BAR_MARGIN)
            .w(width)
            .items_end()
            .gap_1()
            .map(
                |el| match feed_on_top(self.bar, area, self.bar_size.get()) {
                    true => el.top(BAR_MARGIN),
                    false => el.bottom(BAR_MARGIN),
                },
            )
            .children(rows);
        Some(feed.into_any_element())
    }
}
