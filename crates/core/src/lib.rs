mod document;
mod store;
pub use document::{Document, TextRange};
pub use store::{LoadOrigin, LoadedNote, NoteName, NoteStore, ScanReport, Summary};

pub fn title(source: &str, fallback: &str) -> String {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let mut heading = None;
    for event in Parser::new(source.trim_start_matches('\u{feff}')) {
        match event {
            Event::Start(Tag::Heading { .. }) => heading = Some(String::new()),
            Event::Text(text) | Event::Code(text) if heading.is_some() => {
                heading.as_mut().unwrap().push_str(&text)
            }
            Event::SoftBreak | Event::HardBreak if heading.is_some() => {
                heading.as_mut().unwrap().push(' ')
            }
            Event::End(TagEnd::Heading(_)) => {
                let title = heading.take().unwrap();
                if !title.trim().is_empty() {
                    return title.trim().into();
                }
            }
            _ => {}
        }
    }

    fallback.trim_end_matches(".md").into()
}

#[cfg(test)]
mod summary_tests {
    use super::*;
    #[test]
    fn headings_are_display_only_and_skip_fences() {
        assert_eq!(
            title(
                "\u{feff}```md\r\n# Hidden\r\n```\r\nReal title\r\n===\r\n",
                "old.md"
            ),
            "Real title"
        );
        assert_eq!(
            title("~~~\n# Hidden\n~~~\n### Actual ###\n", "old.md"),
            "Actual"
        );
        assert_eq!(title("# C#", "old.md"), "C#");
        assert_eq!(title("    indented\n---\n", "old.md"), "old");
        assert_eq!(title("#nospace\nbody\n", "old.md"), "old");
    }
    #[test]
    fn edits_preserve_bom_crlf_and_untouched_bytes() {
        let original = "\u{feff}# Café\r\n\r\nA🙂Z\r\n";
        let mut document = Document::new(original);
        let before = original.find('🙂').unwrap();
        let offset = original[..before].encode_utf16().count();
        assert!(document.replace(Some(offset..offset + 2), "漢"));
        assert_eq!(
            document.text().as_bytes(),
            "\u{feff}# Café\r\n\r\nA漢Z\r\n".as_bytes()
        );
        document.undo();
        assert_eq!(document.text().as_bytes(), original.as_bytes());
        assert!(!document.replace(Some(offset..offset + 1), "x"));
        assert_eq!(document.text(), original);
    }
    #[test]
    fn no_op_edit_does_not_change_source() {
        let mut document = Document::new("\u{feff}a\r\n");
        assert!(document.replace(Some(1..2), "a"));
        assert_eq!(document.text(), "\u{feff}a\r\n");
        assert_eq!(document.word_count(), 1);
    }
}
