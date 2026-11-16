//! 2D board locks: locked pieces (outline and internal lines) cannot be picked or box-selected
//! on the board and are drawn grey; locked internal lines leave clicks and boxes to the pieces.
//! A selection aid only: the 3D view, the inspector and scripts still reach a locked piece, and
//! nothing physical depends on the locks. Every change is one `Command::SetBoardLocks` step.
use super::*;
use marvelous_cloth::authoring::Command;
use std::collections::BTreeSet;

/// The lock state a lock action leads to, from the current one. Pure, so it can be tested
/// without a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LockAction {
    /// Lock the given selection, keeping the pieces that were already locked.
    LockSelected,
    LockAll,
    UnlockAll,
    LockInternalLines,
    UnlockInternalLines,
}

pub(super) fn locks_after(
    action: LockAction,
    all: &[String],
    selected: &[String],
    locked: &BTreeSet<String>,
    internal_lines: bool,
) -> Result<(BTreeSet<String>, bool), String> {
    Ok(match action {
        LockAction::LockSelected => {
            if selected.is_empty() {
                return Err("Select the pieces to lock first".into());
            }
            let mut next = locked.clone();
            next.extend(selected.iter().cloned());
            (next, internal_lines)
        }
        LockAction::LockAll => (all.iter().cloned().collect(), internal_lines),
        LockAction::UnlockAll => (BTreeSet::new(), internal_lines),
        LockAction::LockInternalLines => (locked.clone(), true),
        LockAction::UnlockInternalLines => (locked.clone(), false),
    })
}

impl Atelier {
    /// Whether a piece is locked on the 2D board.
    pub(super) fn board_locked(&self, panel: &str) -> bool {
        self.project.template.locked_panels.contains(panel)
    }

    /// Apply a lock action as one undo step and drop any board selection on locked pieces.
    pub(super) fn apply_lock_action(&mut self, action: LockAction) -> Result<(), String> {
        let all: Vec<String> = self
            .project
            .template
            .panels
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let (panels, internal_lines) = locks_after(
            action,
            &all,
            &self.selected_panels(),
            &self.project.template.locked_panels,
            self.project.template.internal_lines_locked,
        )?;
        let count = panels.len();
        if !self.edit(Command::SetBoardLocks {
            panels,
            internal_lines,
        }) {
            return Err(self.status.clone());
        }
        self.drop_locked_selection();
        self.status = match action {
            LockAction::LockSelected | LockAction::LockAll => {
                format!(
                    "{count} pieces locked in 2D · Unlock all pieces to edit them on the board again"
                )
            }
            LockAction::UnlockAll => "All pieces unlocked in 2D".into(),
            LockAction::LockInternalLines => {
                "Internal lines locked in 2D: clicks and boxes now select only pieces".into()
            }
            LockAction::UnlockInternalLines => "Internal lines unlocked in 2D".into(),
        };
        Ok(())
    }

    /// Lock or unlock one piece from the inspector (a convenience beyond lock-all/unlock-all).
    pub(super) fn set_piece_locked(&mut self, panel: &str, locked: bool) {
        let mut panels = self.project.template.locked_panels.clone();
        if locked {
            panels.insert(panel.to_string());
        } else {
            panels.remove(panel);
        }
        if self.edit(Command::SetBoardLocks {
            panels,
            internal_lines: self.project.template.internal_lines_locked,
        }) {
            self.drop_locked_selection();
        }
    }

    /// A locked piece cannot stay selected on the board: clear its board selection state.
    fn drop_locked_selection(&mut self) {
        let locked = self.project.template.locked_panels.clone();
        let lines = self.project.template.internal_lines_locked;
        self.authoring
            .selection
            .panels
            .retain(|id| !locked.contains(id));
        let board = &mut self.authoring.board;
        board.point_groups.retain(|id, _| !locked.contains(id));
        if board
            .selection_panel
            .as_ref()
            .is_some_and(|id| locked.contains(id))
        {
            board.selection_panel = None;
            board.selection.clear();
        }
        if self
            .authoring
            .anchor
            .as_ref()
            .is_some_and(|(id, _)| locked.contains(id))
        {
            self.authoring.anchor = None;
        }
        let edge_locked = self.authoring.edge.as_ref().is_some_and(|edge| {
            locked.contains(&edge.panel)
                || (lines
                    && self
                        .project
                        .template
                        .panels
                        .iter()
                        .find(|p| p.id == edge.panel)
                        .and_then(|p| p.polygon.as_ref())
                        .is_some_and(|poly| {
                            poly.lines
                                .iter()
                                .any(|line| edge.edge.as_ref() == Some(&line.id))
                        }))
        });
        if edge_locked {
            self.authoring.edge = None;
        }
    }

    /// The "Lock" part of the board's display menu.
    pub(super) fn lock_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("LOCK");
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.selected_panels().is_empty(),
                    egui::Button::new("Lock selected"),
                )
                .on_hover_text(
                    "Selected pieces can no longer be picked or box-selected on the board",
                )
                .clicked()
            {
                action = Some(LockAction::LockSelected);
            }
            if ui.button("Lock all").clicked() {
                action = Some(LockAction::LockAll);
            }
            if ui
                .add_enabled(
                    !self.project.template.locked_panels.is_empty(),
                    egui::Button::new("Unlock all"),
                )
                .clicked()
            {
                action = Some(LockAction::UnlockAll);
            }
        });
        let mut lines = self.project.template.internal_lines_locked;
        if ui
            .checkbox(&mut lines, "Lock internal lines")
            .on_hover_text("Clicks and boxes on the board reach only the pieces")
            .changed()
        {
            action = Some(if lines {
                LockAction::LockInternalLines
            } else {
                LockAction::UnlockInternalLines
            });
        }
        if let Some(action) = action
            && let Err(error) = self.apply_lock_action(action)
        {
            self.status = error;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn lock_selected_keeps_earlier_locks_and_needs_a_selection() {
        let all = ids(&["front", "back", "sleeve"]);
        let locked: BTreeSet<String> = ids(&["back"]).into_iter().collect();
        let (next, lines) = locks_after(
            LockAction::LockSelected,
            &all,
            &ids(&["front"]),
            &locked,
            true,
        )
        .unwrap();
        assert_eq!(next, ids(&["back", "front"]).into_iter().collect());
        assert!(lines, "locking pieces leaves the internal-line lock alone");
        assert!(locks_after(LockAction::LockSelected, &all, &[], &locked, false).is_err());
    }

    #[test]
    fn lock_all_unlock_all_and_internal_lines() {
        let all = ids(&["front", "back"]);
        let none = BTreeSet::new();
        let (every, _) = locks_after(LockAction::LockAll, &all, &[], &none, false).unwrap();
        assert_eq!(every.len(), 2);
        let (cleared, lines) = locks_after(LockAction::UnlockAll, &all, &[], &every, true).unwrap();
        assert!(cleared.is_empty());
        assert!(lines);
        let (kept, lines) =
            locks_after(LockAction::LockInternalLines, &all, &[], &every, false).unwrap();
        assert_eq!(kept, every);
        assert!(lines);
        let (_, lines) =
            locks_after(LockAction::UnlockInternalLines, &all, &[], &every, true).unwrap();
        assert!(!lines);
    }
}
