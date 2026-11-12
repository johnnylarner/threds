use crate::editor::EditorView;
use gpui::{prelude::*, *};
use std::{path::PathBuf, time::Duration};
use threds_core::{Document, LoadOrigin, LoadedNote, NoteName, NoteStore, Summary};

pub struct Palette {
    pub background: Hsla,
    pub surface: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub line: Hsla,
    pub accent: Hsla,
}
impl Palette {
    pub fn for_window(window: &Window) -> Self {
        if matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ) {
            Self {
                background: rgb(0x171918).into(),
                surface: rgb(0x1e211f).into(),
                text: rgb(0xe8ebe6).into(),
                muted: rgb(0xa2aaa2).into(),
                line: rgb(0x353b36).into(),
                accent: rgb(0xa7c8ac).into(),
            }
        } else {
            Self {
                background: rgb(0xf6f5f0).into(),
                surface: rgb(0xffffff).into(),
                text: rgb(0x242d27).into(),
                muted: rgb(0x6c766e).into(),
                line: rgb(0xe2e5de).into(),
                accent: rgb(0x3f6c4b).into(),
            }
        }
    }
}

enum SaveState {
    Saved(u64),
    Unsaved,
    Failed(String),
}
struct ActiveNote {
    name: NoteName,
    document: Document,
    origin: LoadOrigin,
    recovery_reason: Option<String>,
    save: SaveState,
    observed_revision: u64,
}
impl ActiveNote {
    fn loaded(note: LoadedNote) -> Self {
        Self {
            name: note.name,
            document: Document::new(&note.source),
            save: if note.origin == LoadOrigin::Primary {
                SaveState::Saved(0)
            } else {
                SaveState::Unsaved
            },
            origin: note.origin,
            recovery_reason: note.recovery_reason,
            observed_revision: 0,
        }
    }
}

pub struct AppSession {
    root: PathBuf,
    store: Option<NoteStore>,
    active: Option<ActiveNote>,
    query: Document,
    summaries: Vec<Summary>,
    problems: Vec<String>,
    error: Option<String>,
    pub editor: EditorView,
    library_scroll: ScrollHandle,
    pending_save: Option<Task<()>>,
    _lifecycle: Subscription,
}
impl AppSession {
    pub fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        let weak = cx.weak_entity();
        let lifecycle = cx.on_app_lifecycle(move |phase, cx| {
            if matches!(
                phase,
                AppLifecyclePhase::Inactive
                    | AppLifecyclePhase::Background
                    | AppLifecyclePhase::Disconnected
            ) {
                let _ = weak.update(cx, |session, cx| {
                    session.pending_save = None;
                    session.flush(cx);
                });
            }
        });
        let mut session = Self {
            root,
            store: None,
            active: None,
            query: Document::new(""),
            summaries: vec![],
            problems: vec![],
            error: None,
            editor: EditorView::new(cx),
            library_scroll: ScrollHandle::new(),
            pending_save: None,
            _lifecycle: lifecycle,
        };
        session.refresh();
        session
    }
    pub fn input(&self) -> &Document {
        self.active
            .as_ref()
            .map(|note| &note.document)
            .unwrap_or(&self.query)
    }
    pub fn input_mut(&mut self) -> &mut Document {
        self.active
            .as_mut()
            .map(|note| &mut note.document)
            .unwrap_or(&mut self.query)
    }
    pub fn editing_note(&self) -> bool {
        self.active.is_some()
    }
    pub fn input_changed(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.active.as_mut() else {
            cx.notify();
            return;
        };
        let revision = note.document.revision();
        if revision == note.observed_revision {
            return;
        }
        note.observed_revision = revision;
        note.save = SaveState::Unsaved;
        let name = note.name.clone();
        self.pending_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ =
                this.update(cx, |session, cx| {
                    if session.active.as_ref().is_some_and(|note| {
                        note.name == name && note.document.revision() == revision
                    }) {
                        session.flush(cx);
                    }
                });
        }));
    }
    fn refresh(&mut self) {
        if self.store.is_none() {
            match NoteStore::open(self.root.clone()) {
                Ok(store) => {
                    self.store = Some(store);
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(format!("Could not open notes. {error}"));
                    return;
                }
            }
        }
        let report = self.store.as_ref().unwrap().scan();
        self.summaries = report.notes;
        self.problems = report.problems;
    }
    fn flush(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(note) = self.active.as_mut() else {
            return true;
        };
        let revision = note.document.revision();
        if matches!(note.save, SaveState::Saved(saved) if saved == revision) {
            return true;
        }
        let result = self
            .store
            .as_ref()
            .ok_or_else(|| std::io::Error::other("Notes folder unavailable"))
            .and_then(|store| store.save(&note.name, note.document.text(), note.origin));
        match result {
            Ok(()) => {
                note.save = SaveState::Saved(revision);
                note.origin = LoadOrigin::Primary;
                note.recovery_reason = None;
                self.refresh();
                cx.notify();
                true
            }
            Err(error) => {
                note.save = SaveState::Failed(error.to_string());
                cx.notify();
                false
            }
        }
    }
    fn leave_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_save = None;
        if !self.flush(cx) {
            return;
        }
        window.blur(cx);
        self.active = None;
        self.editor.reset(cx);
        window.dismiss_virtual_keyboard();
        self.refresh();
        cx.notify();
    }
    fn open_note(&mut self, name: &NoteName, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_save = None;
        if !self.flush(cx) {
            return;
        }
        let Some(store) = &self.store else {
            return;
        };
        match store.load(name) {
            Ok(note) => {
                window.blur(cx);
                self.active = Some(ActiveNote::loaded(note));
                self.error = None;
                self.editor.reset(cx);
                window.dismiss_virtual_keyboard();
            }
            Err(error) => self.error = Some(format!("Could not open note. {error}")),
        }
        cx.notify();
    }
    fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_save = None;
        if !self.flush(cx) {
            return;
        }
        let Some(store) = &self.store else {
            return;
        };
        match store.create() {
            Ok(note) => {
                window.blur(cx);
                self.active = Some(ActiveNote::loaded(note));
                self.error = None;
                self.editor.reset(cx);
                window.focus(&self.editor.focus, cx);
                window.request_virtual_keyboard();
            }
            Err(error) => self.error = Some(format!("Could not create note. {error}")),
        }
        cx.notify();
    }
    fn retry(&mut self, cx: &mut Context<Self>) {
        self.pending_save = None;
        if self.active.is_some() {
            self.flush(cx);
        } else {
            self.refresh();
        }
        cx.notify();
    }
}

impl Render for AppSession {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::for_window(window);
        let visible = window.fully_visible_bounds();
        let editing = self.active.is_some();
        let mut body = div()
            .absolute()
            .left(visible.left())
            .top(visible.top())
            .w(visible.size.width)
            .h(visible.size.height)
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .text_color(p.text);
        if let Some(note) = &self.active {
            let title = threds_core::title(note.document.text(), note.name.as_str());
            let status = match &note.save {
                SaveState::Saved(_) => "Saved on device",
                SaveState::Unsaved => "Unsaved",
                SaveState::Failed(_) => "Could not save",
            };
            let failed = matches!(note.save, SaveState::Failed(_));
            let error = match &note.save {
                SaveState::Failed(error) => Some(error.clone()),
                _ => None,
            };
            let recovered = note.recovery_reason.clone();
            let words = note.document.word_count();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .id("back")
                            .py_2()
                            .pr_3()
                            .text_color(p.accent)
                            .child("‹ Notes")
                            .on_click(cx.listener(Self::leave_note_click)),
                    )
                    .child(
                        div()
                            .id("done")
                            .py_2()
                            .pl_3()
                            .text_color(p.accent)
                            .child("Done")
                            .on_click(cx.listener(Self::leave_note_click)),
                    ),
            );
            body = body.child(div().text_size(px(22.)).child(title));
            body = body.child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child(status)
                    .child(format!("{words} words")),
            );
            if let Some(reason) = recovered {
                body = body.child(
                    div()
                        .text_size(px(13.))
                        .child(format!("Recovered the last good copy. {reason}")),
                );
            }
            if failed || note.origin == LoadOrigin::Recovery {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .text_size(px(13.))
                        .child(div().flex_1().child(error.unwrap_or_else(|| {
                            "Save this recovered copy to repair the note.".into()
                        })))
                        .child(
                            div()
                                .id("retry-save")
                                .p_2()
                                .text_color(p.accent)
                                .child("Retry")
                                .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                        ),
                );
            }
        } else {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_size(px(32.)).child("Threds"))
                    .child(
                        div()
                            .id("new")
                            .py_2()
                            .pl_4()
                            .text_size(px(17.))
                            .text_color(p.accent)
                            .child("+ New")
                            .on_click(cx.listener(|this, _, w, cx| this.new_note(w, cx))),
                    ),
            );
            body = body.child(
                div()
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child("On this device · GitHub not connected"),
            );
            body = body.child(
                div()
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child("Search notes"),
            );
        }
        if let Some(error) = &self.error {
            body = body.child(
                div().text_size(px(13.)).child(error.clone()).child(
                    div()
                        .id("retry-open")
                        .py_2()
                        .text_color(p.accent)
                        .child("Retry")
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                ),
            );
        }
        if editing {
            body = body.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.editor_element(window, cx)),
            );
            body = body.child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child("Markdown source · Swipe sideways for long lines"),
            );
        } else {
            body = body.child(
                div()
                    .h(px(46.))
                    .flex_none()
                    .px_2()
                    .py_1()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.line)
                    .child(self.editor_element(window, cx)),
            );
            let query = self.query.text();
            let rows = self
                .summaries
                .iter()
                .filter(|note| note.matches(query))
                .cloned()
                .collect::<Vec<_>>();
            let mut list = div()
                .id("notes")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.library_scroll);
            if rows.is_empty() {
                list = list.child(div().py_6().text_color(p.muted).child(
                    if query.trim().is_empty() {
                        "A place for your next thought. Tap New to begin."
                    } else {
                        "No notes match your search."
                    },
                ));
            }
            for row in rows {
                let name = row.name.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(row.name.as_str().to_owned()))
                        .py_4()
                        .border_b_1()
                        .border_color(p.line)
                        .child(div().text_size(px(19.)).child(row.title))
                        .child(div().mt_1().text_size(px(14.)).text_color(p.muted).child(
                            if row.excerpt.is_empty() {
                                "Empty note".into()
                            } else {
                                row.excerpt
                            },
                        ))
                        .when(row.recovered, |row| {
                            row.child(
                                div()
                                    .mt_1()
                                    .text_size(px(12.))
                                    .text_color(p.accent)
                                    .child("Recovery copy available"),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_note(&name, window, cx)
                        })),
                );
            }
            for problem in &self.problems {
                list = list.child(
                    div()
                        .py_3()
                        .text_size(px(12.))
                        .text_color(p.muted)
                        .child(format!("Could not read a note. {problem}")),
                );
            }
            body = body.child(list);
            if !self.problems.is_empty() {
                body = body.child(
                    div()
                        .id("rescan")
                        .py_2()
                        .text_color(p.accent)
                        .child("Retry reading notes")
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                );
            }
        }
        div()
            .size_full()
            .bg(p.background)
            .font_family(".AppleSystemUIFont")
            .child(body)
    }
}
impl AppSession {
    fn leave_note_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_note(window, cx);
    }
}
