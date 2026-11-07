//! Checks for the shared scrollbars: where a bar sits and whether a press on its track scrolls.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, Pixels, TestAppContext, point, px,
};
use tf2tools_app::widgets::scroll::BAR_WIDTH;

pub fn bounds(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Bounds<Pixels> {
    let id = id.into();
    super::bounds_of(cx, win, id.clone()).unwrap_or_else(|| panic!("no {id:?}"))
}

fn near(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(1.)
}

/// Asserts the scrollbar `bar` spans the right edge of `area`, top to bottom.
pub fn assert_bar_on_right_edge(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    area: impl Into<ElementId>,
    bar: impl Into<ElementId>,
) {
    let (area, bar) = (bounds(cx, win, area), bounds(cx, win, bar));
    assert!(
        near(bar.right(), area.right())
            && near(bar.top(), area.top())
            && near(bar.bottom(), area.bottom()),
        "the bar {bar:?} is not on the right edge of {area:?}"
    );
}

/// Asserts the scrollbar `bar` runs top to bottom in a bar-wide gutter just right of `area`.
pub fn assert_bar_beside(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    area: impl Into<ElementId>,
    bar: impl Into<ElementId>,
) {
    let (area, bar) = (bounds(cx, win, area), bounds(cx, win, bar));
    assert!(
        near(bar.right(), area.right() + BAR_WIDTH)
            && near(bar.top(), area.top())
            && near(bar.bottom(), area.bottom()),
        "the bar {bar:?} is not beside {area:?}"
    );
}

/// Asserts `content` ends left of the track `bar` draws at its right edge.
pub fn assert_clear_of_bar(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    content: impl Into<ElementId>,
    bar: impl Into<ElementId>,
) {
    let (content, bar) = (bounds(cx, win, content), bounds(cx, win, bar));
    assert!(
        content.right() <= bar.right() - BAR_WIDTH + px(1.),
        "{content:?} runs under the bar at the right edge of {bar:?}"
    );
}

/// Presses the foot of `area`'s scrollbar track; a drawn bar scrolls toward the end.
pub fn press_track_foot(cx: &mut TestAppContext, win: AnyWindowHandle, area: impl Into<ElementId>) {
    let area = area.into();
    let b = bounds(cx, win, area.clone());
    cx.update_window(win, |_, w, cx| {
        w.click_at(
            area,
            point(b.size.width - px(6.), b.size.height - px(6.)),
            cx,
        )
    })
    .unwrap();
    super::settle(cx, win);
}

/// Asserts a press on `area`'s track foot moves `probe` up by more than a few pixels.
pub fn assert_track_scrolls(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    area: impl Into<ElementId>,
    probe: impl Into<ElementId>,
) {
    let (area, probe) = (area.into(), probe.into());
    let before = bounds(cx, win, probe.clone());
    press_track_foot(cx, win, area.clone());
    let after = bounds(cx, win, probe);
    assert!(
        after.top() < before.top() - px(20.),
        "no scrollbar took the press in {area:?}: {before:?} -> {after:?}"
    );
}

/// Like `assert_track_scrolls` for a virtualized list, where the press may scroll `probe`
/// out of the built rows altogether.
pub fn assert_track_scrolls_virtual(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    area: impl Into<ElementId>,
    probe: impl Into<ElementId>,
) {
    let (area, probe) = (area.into(), probe.into());
    let before = bounds(cx, win, probe.clone());
    press_track_foot(cx, win, area.clone());
    if let Some(after) = super::bounds_of(cx, win, probe) {
        assert!(
            after.top() < before.top() - px(20.),
            "no scrollbar took the press in {area:?}: {before:?} -> {after:?}"
        );
    }
}

/// Asserts a press on `area`'s track foot leaves `probe` where it was.
pub fn assert_track_stays(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    area: impl Into<ElementId>,
    probe: impl Into<ElementId>,
) {
    let probe = probe.into();
    let before = bounds(cx, win, probe.clone());
    press_track_foot(cx, win, area);
    assert_eq!(
        bounds(cx, win, probe),
        before,
        "pressing the track foot should not move the probe"
    );
}
