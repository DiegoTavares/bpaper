//! Concealed markup in the Markdown editor (spec V10). While the cursor is
//! elsewhere, a vault note's markup renders the way it would in preview:
//! heading markers and link syntax are folded away behind an invisible
//! placeholder, headings and link labels are coloured, and a `___` line is
//! drawn as a rule. Putting the cursor on a line restores that whole line's
//! source (§4.2). Everything here is display-only — folds and highlights live
//! in the `DisplayMap` and no code path writes to the buffer (§4.3).

use crate::markdown_syntax::{self, SpanKind};
use crate::vault::{Vault, VaultStatus};
use editor::display_map::Crease;
use editor::{Editor, EditorEvent, EditorMode, FoldPlaceholder, HighlightKey};
use gpui::{
    App, AppContext as _, Context, Empty, HighlightStyle, Hsla, IntoElement as _,
    ParentElement as _, Styled as _, Subscription, Task, WeakEntity, div, px,
};
use multi_buffer::{Anchor, MultiBufferOffset, MultiBufferSnapshot, ToOffset as _, ToPoint as _};
use settings::SettingsStore;
use std::any::TypeId;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;
use ui::ActiveTheme as _;

gpui::actions!(
    thock,
    [
        /// Shows or hides the Markdown markup in the current note.
        ToggleMarkdownSource
    ]
);

const REPARSE_DEBOUNCE: Duration = Duration::from_millis(50);

/// Tags the conceal folds so removal never touches the user's own folds and
/// theirs never diff as ours (§10.2).
struct ConcealFoldTag;

fn fold_type_tag() -> TypeId {
    TypeId::of::<ConcealFoldTag>()
}

/// A scanned span re-anchored into the buffer so it survives edits between
/// reparses.
struct AnchorSpan {
    range: Range<Anchor>,
    kind: SpanKind,
}

pub struct MarkdownConcealAddon {
    enabled: bool,
    spans: Vec<AnchorSpan>,
    reparse: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl editor::Addon for MarkdownConcealAddon {
    fn extend_key_context(&self, key_context: &mut gpui::KeyContext, _: &App) {
        key_context.add("ThockMarkdownConceal");
    }

    fn to_any(&self) -> &dyn std::any::Any {
        self
    }

    fn to_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

pub fn init(cx: &mut App) {
    cx.observe_new(|editor: &mut Editor, _, cx| register(editor, cx))
        .detach();
}

/// Installs the addon on editors where conceal applies: a full, writable,
/// singleton editor over a `.md` file that lives under a Thock vault root
/// (§4.4). Every other editor — a README in a code repo, pickers, minimaps —
/// is left exactly as it is today.
fn register(editor: &mut Editor, cx: &mut Context<Editor>) {
    if !editor.mode().is_full()
        || matches!(editor.mode(), EditorMode::Minimap { .. })
        || editor.read_only(cx)
    {
        return;
    }
    let Some(buffer) = editor.buffer().read(cx).as_singleton() else {
        return;
    };
    let Some(file) = buffer.read(cx).file() else {
        return;
    };
    if file.path().extension() != Some("md") {
        return;
    }
    let Some(file) = project::File::from_dyn(Some(file)) else {
        return;
    };
    let vault_root = file.worktree.read(cx).abs_path();
    let VaultStatus::Valid(vault) = Vault::detect(&vault_root) else {
        return;
    };
    install(editor, vault.config.markdown.conceal, cx);
}

/// Installs the addon and its subscriptions on an editor that passed the
/// vault gate. Split from `register` so tests can drive an editor without a
/// vault on the real filesystem.
fn install(editor: &mut Editor, enabled: bool, cx: &mut Context<Editor>) {
    let mut subscriptions = Vec::new();
    subscriptions.push(cx.subscribe(
        &cx.entity(),
        |editor, _, event: &EditorEvent, cx| match event {
            EditorEvent::BufferEdited => schedule_reparse(editor, cx),
            EditorEvent::SelectionsChanged { .. } => apply_folds(editor, cx),
            _ => {}
        },
    ));
    // Highlight styles capture theme colours, so a theme change needs a
    // re-apply to pick up the new palette.
    subscriptions
        .push(cx.observe_global::<SettingsStore>(|editor, cx| apply_highlights(editor, cx)));

    let editor_handle = cx.weak_entity();
    subscriptions.push(
        editor.register_action::<ToggleMarkdownSource>(move |_, _, cx| {
            editor_handle
                .update(cx, |editor, cx| toggle(editor, cx))
                .ok();
        }),
    );

    editor.register_addon(MarkdownConcealAddon {
        enabled,
        spans: Vec::new(),
        reparse: Task::ready(()),
        _subscriptions: subscriptions,
    });
    schedule_reparse(editor, cx);
}

fn toggle(editor: &mut Editor, cx: &mut Context<Editor>) {
    let Some(addon) = editor.addon_mut::<MarkdownConcealAddon>() else {
        return;
    };
    addon.enabled = !addon.enabled;
    apply_highlights(editor, cx);
    apply_folds(editor, cx);
}

/// Reparses the whole buffer on a background task after a short debounce,
/// then re-anchors the span plan and applies it. Replacing the previous task
/// is the debounce — only the last edit in a burst parses.
fn schedule_reparse(editor: &mut Editor, cx: &mut Context<Editor>) {
    let task = cx.spawn(async move |editor, cx| {
        cx.background_executor().timer(REPARSE_DEBOUNCE).await;
        let Ok(snapshot) = editor.read_with(cx, |editor, cx| editor.buffer().read(cx).snapshot(cx))
        else {
            return;
        };
        let spans = cx
            .background_spawn(async move {
                markdown_syntax::conceal_spans(&snapshot.text())
                    .into_iter()
                    .map(|span| AnchorSpan {
                        range: snapshot.anchor_after(MultiBufferOffset(span.range.start))
                            ..snapshot.anchor_before(MultiBufferOffset(span.range.end)),
                        kind: span.kind,
                    })
                    .collect::<Vec<_>>()
            })
            .await;
        editor
            .update(cx, |editor, cx| {
                if let Some(addon) = editor.addon_mut::<MarkdownConcealAddon>() {
                    addon.spans = spans;
                }
                apply_highlights(editor, cx);
                apply_folds(editor, cx);
            })
            .ok();
    });
    if let Some(addon) = editor.addon_mut::<MarkdownConcealAddon>() {
        addon.reparse = task;
    }
}

/// Recomputes the desired fold set (all conceal spans minus those on revealed
/// lines) and diffs it against the folds currently in the display map. Runs
/// on every selection change, so it queries real fold state rather than
/// trusting bookkeeping — a `zR` that wiped our folds heals on the next
/// cursor move (§10.2).
///
/// This must go through the `DisplayMap` directly: `Editor::fold_creases`
/// routes through `folds_did_change`, which persists folds into workspace
/// restoration data (§10.1).
fn apply_folds(editor: &mut Editor, cx: &mut Context<Editor>) {
    let Some(addon) = editor.addon::<MarkdownConcealAddon>() else {
        return;
    };
    let enabled = addon.enabled;
    let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);

    let revealed = revealed_rows(editor, &buffer_snapshot);
    let mut desired: Vec<(Range<MultiBufferOffset>, SpanKind)> = Vec::new();
    if enabled {
        for span in &addon.spans {
            if !matches!(span.kind, SpanKind::Marker | SpanKind::Rule) {
                continue;
            }
            let start = span.range.start.to_offset(&buffer_snapshot);
            let end = span.range.end.to_offset(&buffer_snapshot);
            if start >= end {
                continue;
            }
            let start_row = span.range.start.to_point(&buffer_snapshot).row;
            let end_row = span.range.end.to_point(&buffer_snapshot).row;
            let is_revealed = revealed
                .iter()
                .any(|&(first, last)| start_row <= last && end_row >= first);
            if !is_revealed {
                desired.push((start..end, span.kind));
            }
        }
    }

    let display_snapshot = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
    let mut existing: Vec<Range<MultiBufferOffset>> = Vec::new();
    let mut restored_impostors: Vec<Range<MultiBufferOffset>> = Vec::new();
    for fold in display_snapshot.folds_in_range(MultiBufferOffset(0)..buffer_snapshot.len()) {
        let range = fold.range.start.to_offset(&buffer_snapshot)
            ..fold.range.end.to_offset(&buffer_snapshot);
        if fold.placeholder.type_tag == Some(fold_type_tag()) {
            existing.push(range);
        } else if addon.spans.iter().any(|span| {
            matches!(span.kind, SpanKind::Marker | SpanKind::Rule)
                && span.range.start.to_offset(&buffer_snapshot) == range.start
                && span.range.end.to_offset(&buffer_snapshot) == range.end
        }) {
            // A fold restored from the workspace database that exactly
            // matches a conceal span is one of ours that got swept into fold
            // persistence — purge it before it renders as a literal `⋯`
            // (§10.1).
            restored_impostors.push(range);
        }
    }

    let stale: Vec<Range<MultiBufferOffset>> = existing
        .iter()
        .filter(|range| !desired.iter().any(|(desired, _)| desired == *range))
        .cloned()
        .collect();
    let new: Vec<(Range<MultiBufferOffset>, SpanKind)> = desired
        .iter()
        .filter(|(range, _)| !existing.contains(range) || restored_impostors.contains(range))
        .cloned()
        .collect();
    if stale.is_empty() && new.is_empty() && restored_impostors.is_empty() {
        return;
    }

    let editor_handle = cx.weak_entity();
    let creases: Vec<Crease<MultiBufferOffset>> = new
        .into_iter()
        .map(|(range, kind)| {
            let placeholder = match kind {
                SpanKind::Rule => rule_placeholder(editor_handle.clone()),
                _ => marker_placeholder(),
            };
            Crease::simple(range, placeholder)
        })
        .collect();

    editor.display_map.update(cx, |map, cx| {
        if !restored_impostors.is_empty() {
            map.unfold_intersecting(restored_impostors, false, cx);
        }
        if !stale.is_empty() {
            map.remove_folds_with_type(stale, fold_type_tag(), cx);
        }
        if !creases.is_empty() {
            map.fold(creases, cx);
        }
    });
    cx.notify();
}

/// The buffer rows currently revealed, as inclusive `(first, last)` pairs:
/// any row a selection's head or tail lies on, or that a selection covers —
/// the union across all cursors, with no "primary" (§5 R1–R3).
fn revealed_rows(editor: &Editor, buffer_snapshot: &MultiBufferSnapshot) -> Vec<(u32, u32)> {
    editor
        .selections
        .disjoint_anchors()
        .iter()
        .map(|selection| {
            let start = selection.start.to_point(buffer_snapshot).row;
            let end = selection.end.to_point(buffer_snapshot).row;
            (start, end)
        })
        .collect()
}

/// Placeholder for hidden markup: a single-character collapsed text rendered
/// by a zero-width element, so nothing is drawn where the markers were.
///
/// This is the §11.1 fallback, not the zero-length ideal: an empty collapsed
/// text produces a zero-output transform that underflows the tab map's
/// `TabStops` iterator (`chunk_len - 1` on an empty chunk), so each concealed
/// span costs one invisible display column instead (§10.3).
fn marker_placeholder() -> FoldPlaceholder {
    FoldPlaceholder {
        render: Arc::new(|_, _, _| Empty.into_any_element()),
        constrain_width: false,
        merge_adjacent: false,
        type_tag: Some(fold_type_tag()),
        collapsed_text: Some(" ".into()),
    }
}

/// Placeholder for a `___` line: a drawn horizontal rule. The collapsed text
/// must be non-empty for the rendered element to survive to layout, which
/// costs one display column on a line that is otherwise entirely folded.
fn rule_placeholder(editor: WeakEntity<Editor>) -> FoldPlaceholder {
    FoldPlaceholder {
        render: Arc::new(move |_, _, cx| {
            // The placeholder render never sees the layout's max width, so
            // the rule is sized from the editor's last known bounds — close
            // enough to edge-to-edge without overflowing into a horizontal
            // scroll.
            let width = editor
                .upgrade()
                .and_then(|editor| {
                    editor
                        .read(cx)
                        .last_bounds()
                        .map(|bounds| bounds.size.width)
                })
                .map(|width| width * 0.9)
                .unwrap_or(px(480.));
            div()
                .w(width)
                .h_full()
                .flex()
                .items_center()
                .child(div().w_full().h(px(1.)).bg(cx.theme().colors().border))
                .into_any_element()
        }),
        constrain_width: false,
        merge_adjacent: false,
        type_tag: Some(fold_type_tag()),
        collapsed_text: Some(" ".into()),
    }
}

/// The highlight slot for a colourable span. Links get the higher-priority
/// slots so a link label inside a heading keeps its link colour.
fn highlight_slot(kind: SpanKind) -> Option<usize> {
    match kind {
        SpanKind::WikilinkLabel => Some(0),
        SpanKind::LinkLabel => Some(1),
        // Levels 4–6 reuse level 3 — three signals are enough to read
        // structure at a glance (§7.1).
        SpanKind::Heading(level) => Some(1 + (level.clamp(1, 3) as usize)),
        SpanKind::Marker | SpanKind::Rule => None,
    }
}

fn slot_color(slot: usize, cx: &App) -> Hsla {
    let colors = cx.theme().colors();
    match slot {
        0 => colors.text_accent,
        1 => cx
            .theme()
            .syntax()
            .style_for_name("link_uri")
            .and_then(|style| style.color)
            .unwrap_or(colors.text_accent),
        _ => {
            let players = &cx.theme().players().0;
            // Slot 0 of the player palette is the local-user colour; heading
            // levels take the stable slots after it, the same bet the Day
            // Planner makes (§7.1, A4).
            players
                .get(1..)
                .filter(|palette| !palette.is_empty())
                .and_then(|palette| palette.get((slot - 2) % palette.len()))
                .map(|player| player.cursor)
                .unwrap_or(colors.text_accent)
        }
    }
}

/// Reapplies all colour highlights from the current span plan. Colours are
/// independent of reveal — a heading on the cursor's line stays coloured
/// while showing its `#` (§8.2).
fn apply_highlights(editor: &mut Editor, cx: &mut Context<Editor>) {
    let Some(addon) = editor.addon::<MarkdownConcealAddon>() else {
        return;
    };
    let mut by_slot: [Vec<Range<Anchor>>; 5] = Default::default();
    if addon.enabled {
        for span in &addon.spans {
            if let Some(slot) = highlight_slot(span.kind) {
                by_slot[slot].push(span.range.clone());
            }
        }
    }
    for (slot, ranges) in by_slot.into_iter().enumerate() {
        editor.clear_highlights(HighlightKey::ThockMarkdownConceal(slot), cx);
        if !ranges.is_empty() {
            let style = HighlightStyle {
                color: Some(slot_color(slot, cx)),
                ..Default::default()
            };
            editor.highlight_text(HighlightKey::ThockMarkdownConceal(slot), ranges, style, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs::FakeFs;
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use multi_buffer::MultiBufferPoint;
    use project::Project;
    use serde_json::json;
    use std::path::Path;
    use text::Bias;

    const NOTE: &str = "# Title\nsee [[wiki]] and [docs](https://a.example)\n___\nplain tail\n";

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = settings::SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
            release_channel::init(semver::Version::new(0, 0, 0), cx);
            editor::init(cx);
        });
    }

    async fn setup(cx: &mut TestAppContext, text: &str) -> (Entity<Editor>, VisualTestContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/vault", json!({ "note.md": text })).await;
        let project = Project::test(fs, [Path::new("/vault")], cx).await;
        let buffer = project
            .update(cx, |project, cx| {
                project.open_local_buffer("/vault/note.md", cx)
            })
            .await
            .unwrap();
        let window = cx
            .add_window(|window, cx| Editor::for_buffer(buffer, Some(project.clone()), window, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let editor = window.root(&mut cx).unwrap();
        editor.update_in(&mut cx, |editor, _, cx| install(editor, true, cx));
        settle(&mut cx);
        (editor, cx)
    }

    /// Lets the reparse debounce elapse and all effects flush.
    fn settle(cx: &mut VisualTestContext) {
        cx.executor().advance_clock(REPARSE_DEBOUNCE * 2);
        cx.run_until_parked();
    }

    fn display_text(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
        editor.update(cx, |editor, cx| editor.display_text(cx))
    }

    fn move_cursor_to(editor: &Entity<Editor>, row: u32, cx: &mut VisualTestContext) {
        editor.update_in(cx, |editor, window, cx| {
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections
                    .select_ranges([MultiBufferPoint::new(row, 0)..MultiBufferPoint::new(row, 0)]);
            });
        });
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn conceals_markup_when_the_cursor_is_elsewhere(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        move_cursor_to(&editor, 3, &mut cx);
        assert_eq!(
            display_text(&editor, &mut cx),
            " Title\nsee  wiki  and  docs \n \nplain tail\n"
        );
    }

    #[gpui::test]
    async fn the_cursors_line_shows_its_source(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        move_cursor_to(&editor, 0, &mut cx);
        assert_eq!(
            display_text(&editor, &mut cx),
            "# Title\nsee  wiki  and  docs \n \nplain tail\n"
        );
        move_cursor_to(&editor, 1, &mut cx);
        assert_eq!(
            display_text(&editor, &mut cx),
            " Title\nsee [[wiki]] and [docs](https://a.example)\n \nplain tail\n"
        );
    }

    #[gpui::test]
    async fn a_multi_line_selection_reveals_every_covered_line(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        editor.update_in(&mut cx, |editor, window, cx| {
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections
                    .select_ranges([MultiBufferPoint::new(0, 0)..MultiBufferPoint::new(2, 0)]);
            });
        });
        cx.run_until_parked();
        assert_eq!(
            display_text(&editor, &mut cx),
            "# Title\nsee [[wiki]] and [docs](https://a.example)\n___\nplain tail\n"
        );
    }

    #[gpui::test]
    async fn the_buffer_is_never_modified(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        move_cursor_to(&editor, 3, &mut cx);
        move_cursor_to(&editor, 0, &mut cx);
        editor.update_in(&mut cx, |editor, _, cx| toggle(editor, cx));
        editor.update_in(&mut cx, |editor, _, cx| toggle(editor, cx));
        cx.run_until_parked();
        editor.update(&mut cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).snapshot(cx).text(), NOTE);
            assert!(!editor.buffer().read(cx).read(cx).is_dirty());
        });
    }

    #[gpui::test]
    async fn toggling_shows_and_hides_the_source(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        move_cursor_to(&editor, 3, &mut cx);
        editor.update_in(&mut cx, |editor, _, cx| toggle(editor, cx));
        cx.run_until_parked();
        assert_eq!(display_text(&editor, &mut cx), NOTE);
        editor.update_in(&mut cx, |editor, _, cx| toggle(editor, cx));
        cx.run_until_parked();
        assert_eq!(
            display_text(&editor, &mut cx),
            " Title\nsee  wiki  and  docs \n \nplain tail\n"
        );
    }

    #[gpui::test]
    async fn editing_reconceals_after_the_debounce(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        move_cursor_to(&editor, 3, &mut cx);
        editor.update_in(&mut cx, |editor, window, cx| {
            editor.change_selections(Default::default(), window, cx, |selections| {
                let end = MultiBufferPoint::new(4, 0);
                selections.select_ranges([end..end]);
            });
            editor.insert("## Two\nmore\n", window, cx);
        });
        settle(&mut cx);
        move_cursor_to(&editor, 5, &mut cx);
        assert_eq!(
            display_text(&editor, &mut cx),
            " Title\nsee  wiki  and  docs \n \nplain tail\n Two\nmore\n"
        );
    }

    #[gpui::test]
    async fn display_points_map_through_concealed_markers(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        move_cursor_to(&editor, 3, &mut cx);
        editor.update_in(&mut cx, |editor, _, cx| {
            let snapshot = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
            // "# Title" displays as " Title" — the marker fold costs one
            // invisible column (§10.3) — so buffer column 2 is display
            // column 1, and display column 4 lands inside "Title".
            let display = snapshot.point_to_display_point(MultiBufferPoint::new(0, 2), Bias::Left);
            assert_eq!(display.row().0, 0);
            assert_eq!(display.column(), 1);
            let display = snapshot.point_to_display_point(MultiBufferPoint::new(0, 5), Bias::Left);
            assert_eq!(display.column(), 4);
            let buffer_point = snapshot.display_point_to_point(display, Bias::Left);
            assert_eq!(buffer_point, text::Point::new(0, 5));
        });
    }

    #[gpui::test]
    async fn soft_wrap_survives_concealed_folds(cx: &mut TestAppContext) {
        let long = "# A heading long enough to wrap several times when the wrap width is small [[with a wikilink]]\nplain\n";
        let (editor, mut cx) = setup(cx, long).await;
        move_cursor_to(&editor, 1, &mut cx);
        editor.update_in(&mut cx, |editor, _, cx| {
            editor
                .display_map
                .update(cx, |map, cx| map.set_wrap_width(Some(px(160.)), cx));
            let snapshot = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
            assert!(!snapshot.text().contains("[["));
        });
    }

    #[gpui::test]
    async fn restored_folds_matching_spans_are_purged(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        move_cursor_to(&editor, 3, &mut cx);
        // Simulate a fold restored from the workspace database: same range as
        // the heading marker, default placeholder, no type tag (§10.1).
        editor.update_in(&mut cx, |editor, _, cx| {
            let placeholder = editor.default_fold_placeholder(cx);
            editor.display_map.update(cx, |map, cx| {
                map.fold(
                    vec![Crease::simple(
                        MultiBufferOffset(0)..MultiBufferOffset(2),
                        placeholder,
                    )],
                    cx,
                );
            });
        });
        move_cursor_to(&editor, 2, &mut cx);
        move_cursor_to(&editor, 3, &mut cx);
        assert_eq!(
            display_text(&editor, &mut cx),
            " Title\nsee  wiki  and  docs \n \nplain tail\n"
        );
        editor.update_in(&mut cx, |editor, _, cx| {
            let snapshot = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
            let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
            let untagged = snapshot
                .folds_in_range(MultiBufferOffset(0)..buffer_snapshot.len())
                .filter(|fold| fold.placeholder.type_tag != Some(fold_type_tag()))
                .count();
            assert_eq!(untagged, 0);
        });
    }

    #[gpui::test]
    async fn a_wiped_fold_set_heals_on_the_next_cursor_move(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        move_cursor_to(&editor, 3, &mut cx);
        // `unfold_all` and vim's `zR` remove every fold, ours included
        // (§10.2).
        editor.update_in(&mut cx, |editor, _, cx| {
            let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
            editor.display_map.update(cx, |map, cx| {
                map.unfold_intersecting([MultiBufferOffset(0)..buffer_snapshot.len()], true, cx);
            });
        });
        assert_eq!(display_text(&editor, &mut cx), NOTE);
        move_cursor_to(&editor, 0, &mut cx);
        assert_eq!(
            display_text(&editor, &mut cx),
            "# Title\nsee  wiki  and  docs \n \nplain tail\n"
        );
    }

    #[gpui::test]
    async fn headings_and_link_labels_are_coloured(cx: &mut TestAppContext) {
        let (editor, mut cx) = setup(cx, NOTE).await;
        let highlighted = |editor: &mut Editor, slot: usize, cx: &mut Context<Editor>| {
            let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
            editor
                .text_highlights(HighlightKey::ThockMarkdownConceal(slot), cx)
                .map(|(_, ranges)| {
                    ranges
                        .iter()
                        .map(|range| {
                            let start = range.start.to_offset(&buffer_snapshot);
                            let end = range.end.to_offset(&buffer_snapshot);
                            NOTE[start.0..end.0].to_string()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        editor.update_in(&mut cx, |editor, _, cx| {
            assert_eq!(highlighted(editor, 0, cx), vec!["wiki"]);
            assert_eq!(highlighted(editor, 1, cx), vec!["docs"]);
            assert_eq!(highlighted(editor, 2, cx), vec!["Title"]);
            toggle(editor, cx);
            assert!(highlighted(editor, 0, cx).is_empty());
            assert!(highlighted(editor, 2, cx).is_empty());
        });
    }
}
