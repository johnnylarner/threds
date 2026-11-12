use gpui::{prelude::*, *};
use std::ops::Range;

pub struct EditorView {
    pub focus: FocusHandle,
    pub revision: u64,
    layout: Option<Layout>,
    pub scroll: ScrollHandle,
    pub reveal_caret: bool,
    viewport: Option<Bounds<Pixels>>,
    input_reset: bool,
}
impl EditorView {
    pub fn new(cx: &mut Context<AppSession>) -> Self {
        Self {
            focus: cx.focus_handle(),
            revision: 0,
            layout: None,
            scroll: ScrollHandle::new(),
            reveal_caret: true,
            viewport: None,
            input_reset: true,
        }
    }
    pub fn reset(&mut self, cx: &mut Context<AppSession>) {
        self.focus = cx.focus_handle();
        self.input_reset = true;
        self.revision += 1;
        self.layout = None;
        self.reveal_caret = true;
        self.scroll.set_offset(point(px(0.), px(0.)));
    }
}

use crate::app::AppSession;
use threds_core::TextRange;

use unicode_segmentation::UnicodeSegmentation;

actions!(
    threds,
    [
        Backspace, Delete, Left, Right, Home, End, SelectAll, Copy, Cut, Paste, Undo, Redo, Newline
    ]
);

struct Row {
    bytes: TextRange,
    line: ShapedLine,
    bounds: Bounds<Pixels>,
}

struct Layout {
    revision: u64,
    rows: Vec<Row>,
}

impl Layout {
    fn rectangles(&self, range: &TextRange) -> Vec<Bounds<Pixels>> {
        self.rows
            .iter()
            .filter_map(|row| {
                let start = range.0.start.max(row.bytes.0.start);
                let end = range.0.end.min(row.bytes.0.end);
                let intersects = if range.0.is_empty() {
                    range.0.start >= row.bytes.0.start && range.0.start <= row.bytes.0.end
                } else {
                    start <= end
                        && range.0.start <= row.bytes.0.end
                        && range.0.end > row.bytes.0.start
                };
                if !intersects {
                    return None;
                }
                let left = row.line.x_for_index(start - row.bytes.0.start);
                let right = row.line.x_for_index(end - row.bytes.0.start);
                Some(Bounds::new(
                    point(row.bounds.left() + left, row.bounds.top()),
                    size((right - left).max(px(2.)), row.bounds.size.height),
                ))
            })
            .collect()
    }

    fn index(&self, position: Point<Pixels>) -> Option<usize> {
        let row = self
            .rows
            .iter()
            .find(|row| position.y < row.bounds.bottom())
            .or_else(|| self.rows.last())?;
        Some(row.bytes.0.start + row.line.closest_index_for_x(position.x - row.bounds.left()))
    }
}

impl AppSession {
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some("ThredsEditor")),
            KeyBinding::new("delete", Delete, Some("ThredsEditor")),
            KeyBinding::new("enter", Newline, Some("ThredsEditor")),
            KeyBinding::new("left", Left, Some("ThredsEditor")),
            KeyBinding::new("right", Right, Some("ThredsEditor")),
            KeyBinding::new("home", Home, Some("ThredsEditor")),
            KeyBinding::new("end", End, Some("ThredsEditor")),
            KeyBinding::new("cmd-a", SelectAll, Some("ThredsEditor")),
            KeyBinding::new("cmd-c", Copy, Some("ThredsEditor")),
            KeyBinding::new("cmd-x", Cut, Some("ThredsEditor")),
            KeyBinding::new("cmd-v", Paste, Some("ThredsEditor")),
            KeyBinding::new("cmd-z", Undo, Some("ThredsEditor")),
            KeyBinding::new("cmd-shift-z", Redo, Some("ThredsEditor")),
        ]);
    }

    pub fn changed(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.editor.revision += 1;
        self.editor.layout = None;
        self.editor.reveal_caret = true;
        self.input_changed(cx);
        window.notify_text_input_changed(&self.editor.focus);
        cx.notify();
    }

    pub fn app_undo(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.input_mut().undo();
        self.changed(window, cx);
    }

    fn select(&mut self, range: Range<usize>, window: &Window, cx: &mut Context<Self>) {
        if self.input_mut().select(range.clone()) {
            self.editor.reveal_caret = true;
            window.notify_text_selection_changed(&self.editor.focus);
            cx.notify();
        }
    }

    fn delete_grapheme(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input().text();
        let selection = self.input().selection();
        let range = if selection.0.is_empty() {
            let cursor = selection.0.end;
            let other = if backwards {
                text.grapheme_indices(true)
                    .rev()
                    .find(|(i, _)| *i < cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            } else {
                text.grapheme_indices(true)
                    .find(|(i, _)| *i > cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())
            };
            TextRange(other.min(cursor)..other.max(cursor))
        } else {
            selection.clone()
        };
        self.replace_text_in_range(Some(range.utf16(text)), "", window, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selection = &self.input().selection().0;
        if !selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.input().text()[selection.clone()].into(),
            ));
        }
    }
}

impl Focusable for AppSession {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.editor.focus.clone()
    }
}

impl EntityInputHandler for AppSession {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        *actual = None;
        let parsed = TextRange::parse(self.input().text(), range.clone())?;
        *actual = Some(range);
        Some(self.input().text()[parsed.0].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.input().selection().utf16(self.input().text()),
            reversed: false,
        })
    }
    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select(range, window, cx);
    }
    fn text_input_configuration(
        &mut self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> TextInputConfiguration {
        TextInputConfiguration {
            autocorrect: self.editing_note(),
            suggestions: self.editing_note(),
            input_action: if self.editing_note() {
                TextInputAction::Enter
            } else {
                TextInputAction::Search
            },
            ..Default::default()
        }
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.input().text().encode_utf16().count())
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.input()
            .marked()
            .as_ref()
            .map(|c| c.utf16(self.input().text()))
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input_mut().unmark();
        self.changed(window, cx);
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editing_note() && matches!(text, "\n" | "\r" | "\r\n") {
            window.dismiss_virtual_keyboard();
            return;
        }
        let query_text;
        let text = if self.editing_note() {
            text
        } else {
            query_text = text.replace(['\n', '\r'], " ");
            &query_text
        };
        if self.input_mut().replace(range, text) {
            self.changed(window, cx);
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_mut().mark(range, text, selection) {
            self.changed(window, cx);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = TextRange::parse(self.input().text(), range)?;
        let layout = self
            .editor
            .layout
            .as_ref()
            .filter(|l| l.revision == self.editor.revision)?;
        layout.rectangles(&range).into_iter().next()
    }
    fn selection_bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        let Some(range) = TextRange::parse(self.input().text(), range) else {
            return vec![];
        };
        self.editor
            .layout
            .as_ref()
            .filter(|l| l.revision == self.editor.revision)
            .map(|l| l.rectangles(&range))
            .unwrap_or_default()
    }
    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self
            .editor
            .layout
            .as_ref()
            .filter(|l| l.revision == self.editor.revision)?;
        Some(
            TextRange::caret(layout.index(position)?)
                .utf16(self.input().text())
                .start,
        )
    }
}

impl AppSession {
    pub fn editor_element(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let viewport = window.fully_visible_bounds();
        if self.editor.viewport != Some(viewport) {
            self.editor.viewport = Some(viewport);
            self.editor.reveal_caret = true;
        }
        let palette = crate::app::Palette::for_window(window);
        div()
            .id("editor")
            .size_full()
            .min_h_0()
            .w_full()
            .overflow_scroll()
            .track_scroll(&self.editor.scroll)
            .bg(palette.surface)
            .key_context("ThredsEditor")
            .track_focus(&self.editor.focus)
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.editor.focus, cx);
                    if let Some(byte) = this
                        .editor
                        .layout
                        .as_ref()
                        .and_then(|l| l.index(event.position))
                    {
                        this.select(
                            TextRange::caret(byte).utf16(this.input().text()),
                            window,
                            cx,
                        );
                    }
                    window.request_virtual_keyboard();
                }),
            )
            .on_action(cx.listener(|this, _: &Backspace, w, cx| this.delete_grapheme(true, w, cx)))
            .on_action(cx.listener(|this, _: &Delete, w, cx| this.delete_grapheme(false, w, cx)))
            .on_action(cx.listener(|this, _: &Newline, w, cx| {
                if this.editing_note() {
                    this.replace_text_in_range(None, "\n", w, cx);
                } else {
                    w.dismiss_virtual_keyboard();
                }
            }))
            .on_action(cx.listener(|this, _: &SelectAll, w, cx| {
                this.select(0..this.input().text().encode_utf16().count(), w, cx)
            }))
            .on_action(cx.listener(|this, _: &Home, w, cx| this.select(0..0, w, cx)))
            .on_action(cx.listener(|this, _: &End, w, cx| {
                let end = this.input().text().encode_utf16().count();
                this.select(end..end, w, cx);
            }))
            .on_action(cx.listener(|this, _: &Left, w, cx| {
                let byte = this
                    .input()
                    .text()
                    .grapheme_indices(true)
                    .rev()
                    .find(|(i, _)| *i < this.input().selection().0.start)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                this.select(TextRange::caret(byte).utf16(this.input().text()), w, cx);
            }))
            .on_action(cx.listener(|this, _: &Right, w, cx| {
                let byte = this
                    .input()
                    .text()
                    .grapheme_indices(true)
                    .find(|(i, _)| *i > this.input().selection().0.end)
                    .map(|(i, _)| i)
                    .unwrap_or(this.input().text().len());
                this.select(TextRange::caret(byte).utf16(this.input().text()), w, cx);
            }))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(|this, _: &Cut, w, cx| {
                this.copy(&Copy, w, cx);
                if !this.input().selection().0.is_empty() {
                    this.replace_text_in_range(None, "", w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, w, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    this.replace_text_in_range(None, &text, w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Undo, w, cx| {
                this.app_undo(w, cx);
            }))
            .on_action(cx.listener(|this, _: &Redo, w, cx| {
                this.input_mut().redo();
                this.changed(w, cx);
            }))
            .text_size(px(20.))
            .line_height(px(30.))
            .child(EditorText {
                editor: cx.entity(),
            })
    }
}

struct EditorText {
    editor: Entity<AppSession>,
}

impl Element for EditorText {
    type RequestLayoutState = Vec<(TextRange, ShapedLine)>;
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let editor = self.editor.read(cx);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let mut start = 0;
        let mut width = window.fully_visible_bounds().size.width - px(26.);
        let lines = editor
            .input()
            .text()
            .split('\n')
            .map(|text| {
                let bytes = TextRange(start..start + text.len());
                start += text.len() + 1;
                let run = TextRun {
                    len: text.len(),
                    font: style.font(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(
                    text.to_owned().into(),
                    font_size,
                    &[run],
                    None,
                );
                width = width.max(line.width + px(8.));
                (bytes, line)
            })
            .collect::<Vec<_>>();
        let mut style = Style::default();
        style.flex_shrink = 0.;
        style.size = size(
            width.into(),
            (window.line_height() * lines.len() as f32).into(),
        );
        (window.request_layout(style, [], cx), lines)
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        lines: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let rows = lines
            .drain(..)
            .enumerate()
            .map(|(i, (bytes, line))| Row {
                bytes,
                line,
                bounds: Bounds::new(
                    point(
                        bounds.left() + px(4.),
                        bounds.top() + window.line_height() * i as f32,
                    ),
                    size(bounds.size.width, window.line_height()),
                ),
            })
            .collect();
        self.editor.update(cx, |editor, cx| {
            let layout = Layout {
                revision: editor.editor.revision,
                rows,
            };
            let changed_geometry = editor
                .editor
                .layout
                .as_ref()
                .and_then(|l| l.rows.first())
                .map(|r| r.bounds)
                != layout.rows.first().map(|r| r.bounds);
            if editor.editor.reveal_caret {
                if let Some(caret) = layout
                    .rectangles(&TextRange::caret(editor.input().selection().0.end))
                    .first()
                {
                    let visible = editor
                        .editor
                        .scroll
                        .bounds()
                        .intersect(&window.fully_visible_bounds());
                    let mut offset = editor.editor.scroll.offset();
                    if caret.bottom() > visible.bottom() {
                        offset.y -= caret.bottom() - visible.bottom();
                    }
                    if caret.top() < visible.top() {
                        offset.y += visible.top() - caret.top();
                    }
                    if caret.right() > visible.right() {
                        offset.x -= caret.right() - visible.right();
                    }
                    if caret.left() < visible.left() {
                        offset.x += visible.left() - caret.left();
                    }
                    offset.x = offset.x.min(px(0.));
                    offset.y = offset.y.min(px(0.));
                    if offset != editor.editor.scroll.offset() {
                        editor.editor.scroll.set_offset(offset);
                        cx.notify();
                    }
                }
                editor.editor.reveal_caret = false;
            }
            editor.editor.layout = Some(layout);
            if changed_geometry {
                window.notify_text_selection_changed(&editor.editor.focus);
            }
        });
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.editor.update(cx, |editor, cx| {
            window.handle_input(
                &editor.editor.focus,
                ElementInputHandler::new(bounds, self.editor.clone()),
                cx,
            );
            if editor.editor.input_reset {
                editor.editor.input_reset = false;
                let focus = editor.editor.focus.clone();
                window.on_next_frame(move |window, _| window.notify_text_input_changed(&focus));
            }
            let Some(layout) = &editor.editor.layout else {
                return;
            };
            if !editor.input().selection().0.is_empty() {
                for rect in layout.rectangles(&editor.input().selection()) {
                    window.paint_quad(fill(rect, rgba(0x4488ff50)));
                }
            }
            for row in &layout.rows {
                row.line
                    .paint(
                        row.bounds.origin,
                        row.bounds.size.height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .unwrap();
            }
            if let Some(marked) = &editor.input().marked() {
                for rect in layout.rectangles(marked) {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(rect.left(), rect.bottom() - px(2.)),
                            size(rect.size.width, px(1.)),
                        ),
                        rgb(0x2255bb),
                    ));
                }
            }
            if editor.editor.focus.is_focused(window) && editor.input().selection().0.is_empty() {
                for rect in layout.rectangles(&editor.input().selection()) {
                    window.paint_quad(fill(rect, rgb(0x2255bb)));
                }
            }
        });
    }
}
