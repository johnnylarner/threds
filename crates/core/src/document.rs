use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRange(pub Range<usize>);

impl TextRange {
    pub fn parse(text: &str, utf16: Range<usize>) -> Option<Self> {
        fn offset(text: &str, target: usize) -> Option<usize> {
            let mut units = 0;
            for (byte, ch) in text.char_indices() {
                if units == target {
                    return Some(byte);
                }
                units += ch.len_utf16();
            }
            (units == target).then_some(text.len())
        }
        (utf16.start <= utf16.end).then_some(())?;
        Some(Self(offset(text, utf16.start)?..offset(text, utf16.end)?))
    }

    pub fn utf16(&self, text: &str) -> Range<usize> {
        text[..self.0.start].encode_utf16().count()..text[..self.0.end].encode_utf16().count()
    }

    pub fn caret(byte: usize) -> Self {
        Self(byte..byte)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    text: String,
    selection: TextRange,
}

#[derive(Clone)]
struct Composition {
    marked: TextRange,
    before: Snapshot,
}

#[derive(Clone)]
pub struct Document {
    state: Snapshot,
    revision: u64,
    composition: Option<Composition>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Document {
    pub fn new(text: &str) -> Self {
        Self {
            state: Snapshot {
                text: text.into(),
                selection: TextRange::caret(0),
            },
            revision: 0,
            composition: None,
            undo: vec![],
            redo: vec![],
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn text(&self) -> &str {
        &self.state.text
    }
    pub fn selection(&self) -> &TextRange {
        &self.state.selection
    }
    pub fn marked(&self) -> Option<&TextRange> {
        self.composition.as_ref().map(|c| &c.marked)
    }
    pub fn word_count(&self) -> usize {
        self.state.text.split_whitespace().count()
    }

    fn target(&self, range: Option<Range<usize>>) -> Option<TextRange> {
        match range {
            Some(range) => TextRange::parse(&self.state.text, range),
            None => Some(
                self.composition
                    .as_ref()
                    .map(|c| &c.marked)
                    .unwrap_or(&self.state.selection)
                    .clone(),
            ),
        }
    }

    pub fn select(&mut self, range: Range<usize>) -> bool {
        let Some(range) = TextRange::parse(&self.state.text, range) else {
            return false;
        };
        self.state.selection = range;
        true
    }

    pub fn replace(&mut self, range: Option<Range<usize>>, text: &str) -> bool {
        let Some(range) = self.target(range) else {
            return false;
        };
        let before = self
            .composition
            .take()
            .map(|c| c.before)
            .unwrap_or_else(|| self.state.clone());
        if &self.state.text[range.0.clone()] != text {
            self.revision += 1;
        }
        self.state.text.replace_range(range.0.clone(), text);
        self.state.selection = TextRange::caret(range.0.start + text.len());
        if self.state != before {
            self.undo.push(before);
            self.redo.clear();
        }
        true
    }

    pub fn mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
    ) -> bool {
        let Some(range) = self.target(range) else {
            return false;
        };
        let relative = match selection {
            Some(selection) => {
                let Some(selection) = TextRange::parse(text, selection) else {
                    return false;
                };
                selection
            }
            None => TextRange::caret(text.len()),
        };
        let before = self
            .composition
            .take()
            .map(|c| c.before)
            .unwrap_or_else(|| self.state.clone());
        if &self.state.text[range.0.clone()] != text {
            self.revision += 1;
        }
        self.state.text.replace_range(range.0.clone(), text);
        self.state.selection =
            TextRange(range.0.start + relative.0.start..range.0.start + relative.0.end);
        self.composition = Some(Composition {
            marked: TextRange(range.0.start..range.0.start + text.len()),
            before,
        });
        if text.is_empty() {
            self.unmark();
        }
        true
    }

    pub fn unmark(&mut self) {
        if let Some(composition) = self.composition.take() {
            if composition.before != self.state {
                self.undo.push(composition.before);
                self.redo.clear();
            }
        }
    }

    pub fn undo(&mut self) {
        self.unmark();
        if let Some(before) = self.undo.pop() {
            if self.state.text != before.text {
                self.revision += 1;
            }
            self.redo.push(std::mem::replace(&mut self.state, before));
        }
    }

    pub fn redo(&mut self) {
        self.unmark();
        if let Some(after) = self.redo.pop() {
            if self.state.text != after.text {
                self.revision += 1;
            }
            self.undo.push(std::mem::replace(&mut self.state, after));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_utf16_ranges_reject_split_surrogates_and_invalid_offsets() {
        let text = "A🙂e\u{301}\n漢";
        assert_eq!(TextRange::parse(text, 1..3).unwrap().0, 1..5);
        assert!(TextRange::parse(text, 1..2).is_none());
        assert!(TextRange::parse(text, 3..99).is_none());
        assert!(TextRange::parse(text, 4..3).is_none());
        for byte in text.char_indices().map(|(i, _)| i).chain([text.len()]) {
            let range = TextRange::caret(byte);
            assert_eq!(TextRange::parse(text, range.utf16(text)), Some(range));
        }
    }

    #[test]
    fn multiline_replacement_and_undo_restore_selection() {
        let mut doc = Document::new("one\n🙂\nthree\n");
        assert!(doc.select(2..8));
        let before = doc.state.clone();
        assert!(doc.replace(None, "X\nY"));
        assert_eq!(doc.state.text, "onX\nYhree\n");
        doc.undo();
        assert_eq!(doc.state, before);
        doc.redo();
        assert_eq!(doc.state.text, "onX\nYhree\n");
    }

    #[test]
    fn composition_is_one_undo_transaction_and_selection_is_relative() {
        let mut doc = Document::new("A🙂Z");
        doc.select(1..3);
        let before = doc.state.clone();
        assert!(doc.mark(None, "é🙂", Some(1..3)));
        assert_eq!(doc.state.selection.utf16(&doc.state.text), 2..4);
        assert!(!doc.mark(None, "🙂", Some(0..1)));
        assert_eq!(doc.state.text, "Aé🙂Z");
        assert!(doc.mark(None, "漢", Some(1..1)));
        doc.unmark();
        doc.unmark();
        assert_eq!(doc.undo.len(), 1);
        doc.undo();
        assert_eq!(doc.state, before);
    }

    #[test]
    fn commit_replaces_mark_and_empty_mark_deletes_composition() {
        let mut doc = Document::new("AZ");
        doc.select(1..1);
        doc.mark(None, "ni", Some(2..2));
        doc.replace(None, "你");
        assert_eq!(doc.state.text, "A你Z");
        doc.undo();
        assert_eq!(doc.state.text, "AZ");
        doc.mark(None, "draft", None);
        doc.mark(None, "", Some(0..0));
        assert_eq!(doc.state.text, "AZ");
        assert!(doc.composition.is_none());
    }
}
