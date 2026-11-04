//! The animations a borrowed part brings: the source HUD's own events that act on its panels
//! or run one another, as whole events, and the game triggers' lines that start them, as
//! appends to the player's triggers. FireCommand lines never come along.

use std::collections::{HashMap, HashSet};

use super::{Brings, LeftBehind, Run};
use crate::hud_maker::animations::ConsoleReach;
use crate::hud_maker::{
    AnimCommand, AnimEvent, AnimationAppend, AnimationChange, Change, ChangeTarget, GAME_TRIGGERS,
    Part,
};

fn is_trigger(name: &str) -> bool {
    GAME_TRIGGERS.iter().any(|t| t.eq_ignore_ascii_case(name))
}

/// The event a command starts or stops, when it names one.
fn runs(command: &AnimCommand) -> Option<&str> {
    match command {
        AnimCommand::RunEvent { event, .. }
        | AnimCommand::StopEvent { event, .. }
        | AnimCommand::RunEventChild { event, .. } => Some(event),
        _ => None,
    }
}

impl Run<'_> {
    /// Adds the part's events and trigger lines to the changes, renaming any event whose
    /// name the player's HUD already has, and notes the FireCommand lines left out.
    pub(super) fn carry_animations(&mut self, set: &[&Part]) {
        let panels: HashSet<String> = set.iter().map(|p| p.name().to_ascii_lowercase()).collect();
        let own: Vec<&AnimEvent> = (self.source.model.animations().events().iter())
            .filter(|e| e.layer() == Some(0))
            .collect();
        let names_panel = |c: &AnimCommand| {
            c.panel()
                .is_some_and(|p| panels.contains(&p.to_ascii_lowercase()))
        };

        // A line that starts an event carries only when that event comes along too, so a
        // borrowed line never runs one of the player's own events by name.
        let carries = |c: &AnimCommand, part: &HashSet<String>| match runs(c) {
            Some(r) => part.contains(&r.to_ascii_lowercase()),
            None => names_panel(c),
        };

        // The part's events grow from those naming a panel to those running one of them; each
        // round adds at least one event or stops, so the source's event count bounds it.
        let mut part: HashSet<String> = HashSet::new();
        for _ in 0..=own.len() {
            let before = part.len();
            for e in own.iter().filter(|e| !is_trigger(e.name())) {
                if (e.commands().iter()).any(|c| carries(c, &part)) {
                    part.insert(e.name().to_ascii_lowercase());
                }
            }
            if part.len() == before {
                break;
            }
        }
        let carried = |c: &AnimCommand| carries(c, &part);
        // Lines that name a borrowed panel but start an event left behind are counted.
        let stranded = |e: &AnimEvent| {
            (e.commands().iter())
                .filter(|c| names_panel(c) && runs(c).is_some() && !carried(c))
                .count()
        };

        let platform = self.target.platform();
        let mut left_out = 0usize;
        let mut chosen: Vec<String> = Vec::new();
        let mut rename: HashMap<String, String> = HashMap::new();
        let events: Vec<&AnimEvent> = (own.iter().copied())
            .filter(|e| part.contains(&e.name().to_ascii_lowercase()))
            .collect();
        for e in &events {
            let name = self.free_event_name(e.name(), &chosen);
            rename.insert(e.name().to_ascii_lowercase(), name.clone());
            chosen.push(name);
        }
        let mut wholes = Vec::new();
        for (e, name) in events.iter().zip(&chosen) {
            left_out += fire_commands(e) + stranded(e);
            let body = self.lines(e, &carried, &rename);
            wholes.push((name.clone(), body));
        }
        let mut appends = Vec::new();
        for e in own.iter().filter(|e| is_trigger(e.name())) {
            // A trigger's own FireCommands are never carried lines, so none is counted here.
            left_out += stranded(e);
            let lines = self.lines(e, &carried, &rename);
            if lines.is_empty() {
                continue;
            }
            // The player's trigger that runs a console command is never appended to.
            let locked = self.target.can_change_animation(e.name()).is_err();
            if locked {
                left_out += lines.lines().count();
            } else {
                appends.push((e.name().to_owned(), lines));
            }
        }

        // Every line that starts an event goes through the recipe's walk over the player's
        // events with the borrowed ones laid over them; one reaching a console command stays.
        let group = self.group;
        let mut changes: Vec<(String, Change)> = (wholes.into_iter())
            .map(|(n, b)| {
                (
                    n.clone(),
                    Change::animation(group, AnimationChange::new(&n, &b)),
                )
            })
            .collect();
        let whole_changes: Vec<Change> = changes.iter().map(|(_, c)| c.clone()).collect();
        changes.extend((appends.into_iter()).map(|(n, l)| {
            let change = Change::animation_append(group, AnimationAppend::new(&n, &l));
            (n, change)
        }));
        let mut reach = ConsoleReach::over(self.target.animations());
        reach.lay_over(&whole_changes, platform);
        for (name, change) in changes {
            let (change, cut) = reach.cut(&change, platform);
            left_out += cut;
            // A change the replay would drop is left out here, so the borrow replays whole.
            let keep = match change.target() {
                ChangeTarget::Animation(a) => a.check(platform).is_some(),
                ChangeTarget::AnimationAppend(a) => {
                    !a.lines().is_empty() && a.check(platform).is_some()
                }
                _ => false,
            };
            if keep {
                self.changes.push(change);
                self.brings.push(Brings::Animation(name));
            }
        }
        if left_out > 0 {
            self.left_behind.push(LeftBehind::Animations(left_out));
        }
    }

    /// `name`, or `<name>_b<n>` with the smallest free `n` from 1 when the player's HUD or this
    /// borrow already has the name in any case.
    fn free_event_name(&self, name: &str, chosen: &[String]) -> String {
        let anims = self.target.animations();
        let taken =
            |n: &str| anims.event(n).is_some() || chosen.iter().any(|c| c.eq_ignore_ascii_case(n));
        if !taken(name) {
            return name.to_owned();
        }
        (1..)
            .map(|n| format!("{name}_b{n}"))
            .find(|n| !taken(n))
            .unwrap_or_else(|| name.to_owned())
    }

    /// The lines of `event` that `keep` picks, as script text, with event names following
    /// `rename` and names the borrow renamed following `Borrowed::renamed`.
    fn lines(
        &self,
        event: &AnimEvent,
        keep: &dyn Fn(&AnimCommand) -> bool,
        rename: &HashMap<String, String>,
    ) -> String {
        let renamed = |text: &mut String| {
            if let Some((_, new)) =
                (self.renamed.iter()).find(|(old, _)| old.eq_ignore_ascii_case(text))
            {
                *text = new.clone();
            }
        };
        let event_named = |text: &mut String| {
            if let Some(new) = rename.get(&text.to_ascii_lowercase()) {
                *text = new.clone();
            }
        };
        (event.commands().iter())
            .filter(|c| keep(c) && !matches!(c, AnimCommand::FireCommand { .. }))
            .map(|c| {
                let mut c = c.clone();
                match &mut c {
                    AnimCommand::Animate { panel, value, .. }
                    | AnimCommand::SetFont { panel, value, .. }
                    | AnimCommand::SetTexture { panel, value, .. } => {
                        renamed(panel);
                        renamed(value);
                    }
                    AnimCommand::RunEvent { event, .. } | AnimCommand::StopEvent { event, .. } => {
                        event_named(event);
                    }
                    AnimCommand::RunEventChild { panel, event, .. } => {
                        renamed(panel);
                        event_named(event);
                    }
                    AnimCommand::SetVisible { panel, .. }
                    | AnimCommand::SetInputEnabled { panel, .. }
                    | AnimCommand::StopAnimation { panel, .. }
                    | AnimCommand::StopPanelAnimations { panel, .. }
                    | AnimCommand::SetString { panel, .. } => renamed(panel),
                    _ => {}
                }
                c.to_line()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn fire_commands(event: &AnimEvent) -> usize {
    (event.commands().iter())
        .filter(|c| matches!(c, AnimCommand::FireCommand { .. }))
        .count()
}
