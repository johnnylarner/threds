use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineFormat {
    Bold,
    Italic,
    Strikethrough,
    Underline,
}

impl InlineFormat {
    pub(crate) fn delimiters(self) -> (&'static str, &'static str) {
        match self {
            Self::Bold => ("**", "**"),
            Self::Italic => ("*", "*"),
            Self::Strikethrough => ("~~", "~~"),
            Self::Underline => ("<u>", "</u>"),
        }
    }
    pub(crate) fn html_delimiters(self) -> (&'static str, &'static str) {
        match self {
            Self::Bold => ("<strong>", "</strong>"),
            Self::Italic => ("<em>", "</em>"),
            Self::Strikethrough => ("<s>", "</s>"),
            Self::Underline => ("<u>", "</u>"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDecoration {
    pub format: InlineFormat,
    pub source: Range<usize>,
    pub content: Range<usize>,
}

pub fn source_decorations(source: &str) -> Vec<SourceDecoration> {
    let mut decorations = Vec::new();
    let mut html = Vec::new();
    for (event, range) in Parser::new_ext(source, Options::ENABLE_STRIKETHROUGH).into_offset_iter()
    {
        let format = match event {
            Event::Start(Tag::Strong) => Some((InlineFormat::Bold, 2)),
            Event::Start(Tag::Emphasis) => Some((InlineFormat::Italic, 1)),
            Event::Start(Tag::Strikethrough) => Some((
                InlineFormat::Strikethrough,
                if source[range.clone()].starts_with("~~") {
                    2
                } else {
                    1
                },
            )),
            Event::InlineHtml(ref tag) => {
                let opening = match tag.as_ref() {
                    "<u>" => Some(InlineFormat::Underline),
                    "<strong>" => Some(InlineFormat::Bold),
                    "<em>" => Some(InlineFormat::Italic),
                    "<s>" => Some(InlineFormat::Strikethrough),
                    _ => None,
                };
                let closing = match tag.as_ref() {
                    "</u>" => Some(InlineFormat::Underline),
                    "</strong>" => Some(InlineFormat::Bold),
                    "</em>" => Some(InlineFormat::Italic),
                    "</s>" => Some(InlineFormat::Strikethrough),
                    _ => None,
                };
                if let Some(format) = opening {
                    html.push((format, range.clone()));
                }
                if let Some(format) = closing {
                    if let Some((_, open)) = html.last().filter(|(opened, _)| *opened == format) {
                        decorations.push(SourceDecoration {
                            format,
                            content: open.end..range.start,
                            source: open.start..range.end,
                        });
                        html.pop();
                    }
                }
                None
            }
            _ => None,
        };
        if let Some((format, width)) = format {
            decorations.push(SourceDecoration {
                format,
                content: range.start + width..range.end - width,
                source: range,
            });
        }
    }
    decorations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn single_tilde_ranges_remain_valid() {
        let decorations = source_decorations("~a~");
        assert_eq!(decorations[0].content, 1..2);
        let mut doc = Document::new("~a~");
        doc.select(1..2);
        doc.toggle_format(InlineFormat::Strikethrough);
        assert_eq!(doc.text(), "a");
    }

    #[test]
    fn formatting_selection_preserves_unicode_and_one_undo_restores_selection() {
        for (format, expected) in [
            (InlineFormat::Bold, "A<strong>🙂e\u{301}</strong>Z"),
            (InlineFormat::Italic, "A<em>🙂e\u{301}</em>Z"),
            (InlineFormat::Strikethrough, "A~~🙂e\u{301}~~Z"),
            (InlineFormat::Underline, "A<u>🙂e\u{301}</u>Z"),
        ] {
            let mut doc = Document::new("A🙂e\u{301}Z");
            doc.select(1..5);
            doc.toggle_format(format);
            assert_eq!(doc.text(), expected);
            let selection = doc.selection().clone();
            assert_eq!(&doc.text()[selection.0.clone()], "🙂e\u{301}");
            assert!(
                source_decorations(doc.text())
                    .iter()
                    .any(|d| d.format == format),
                "{format:?}: {:?}",
                source_decorations(doc.text())
            );
            doc.toggle_format(format);
            assert_eq!(doc.text(), "A🙂e\u{301}Z");
            doc.undo();
            assert_eq!(doc.text(), expected);
            assert_eq!(doc.selection(), &selection);
            doc.undo();
            assert_eq!(doc.text(), "A🙂e\u{301}Z");
            assert_eq!(doc.selection().utf16(doc.text()), 1..5);
        }
    }

    #[test]
    fn empty_caret_pairs_keep_subsequent_typing_inside_format() {
        let mut doc = Document::new("AZ");
        doc.select(1..1);
        doc.toggle_format(InlineFormat::Underline);
        assert_eq!(doc.text(), "A<u></u>Z");
        doc.replace(None, "漢🙂");
        assert_eq!(doc.text(), "A<u>漢🙂</u>Z");
        doc.undo();
        assert_eq!(doc.text(), "A<u></u>Z");
        doc.undo();
        assert_eq!(doc.text(), "AZ");
        assert_eq!(doc.selection().utf16(doc.text()), 1..1);
    }

    #[test]
    fn empty_format_pairs_toggle_off_and_keep_emoji_inside() {
        for format in [
            InlineFormat::Bold,
            InlineFormat::Italic,
            InlineFormat::Strikethrough,
            InlineFormat::Underline,
        ] {
            for source in ["", "AZ"] {
                let mut doc = Document::new(source);
                let caret = usize::from(!source.is_empty());
                doc.select(caret..caret);
                doc.toggle_format(format);
                let paired = doc.text().to_owned();
                doc.toggle_format(format);
                assert_eq!(doc.text(), source, "{format:?}");
                assert_eq!(doc.selection().utf16(doc.text()), caret..caret);
                doc.undo();
                assert_eq!(doc.text(), paired);
                doc.replace(None, "🙂");
                let decorations = source_decorations(doc.text());
                assert_eq!(decorations[0].format, format);
                assert_eq!(&doc.text()[decorations[0].content.clone()], "🙂");
            }
        }
    }

    #[test]
    fn collapsed_intraword_format_remains_visible_when_typing_emoji() {
        let mut doc = Document::new("AZ");
        doc.select(1..1);
        doc.toggle_format(InlineFormat::Bold);
        doc.replace(None, "🙂");
        assert_eq!(doc.text(), "A<strong>🙂</strong>Z");
        assert_eq!(source_decorations(doc.text())[0].format, InlineFormat::Bold);
    }

    #[test]
    fn italic_inside_bold_does_not_remove_bold_delimiters() {
        let mut doc = Document::new("**word**");
        doc.select(2..6);
        doc.toggle_format(InlineFormat::Italic);
        assert_eq!(doc.text(), "**<em>word</em>**");
        assert!(
            source_decorations(doc.text())
                .iter()
                .any(|d| d.format == InlineFormat::Bold)
        );
        doc.toggle_format(InlineFormat::Italic);
        assert_eq!(doc.text(), "**word**");
        assert_eq!(&doc.text()[doc.selection().0.clone()], "word");
        let mut nested = Document::new("***word***");
        nested.select(3..7);
        nested.toggle_format(InlineFormat::Italic);
        assert_eq!(nested.text(), "**word**");
    }

    #[test]
    fn literal_code_and_escaped_markers_are_not_recognized_as_format() {
        assert!(
            source_decorations("`**code** <u>code</u>`\n\n```\n~~code~~\n```\n\\*literal\\*")
                .is_empty()
        );
        assert!(source_decorations("<u>unclosed").is_empty());
    }

    #[test]
    fn multiline_format_is_one_transaction_and_preserves_crlf() {
        let mut doc = Document::new("one\r\n🙂 two\r\n\r\nthree");
        let end = doc.text().encode_utf16().count();
        doc.select(0..end);
        doc.toggle_format(InlineFormat::Bold);
        assert_eq!(doc.text(), "**one**\r\n**🙂 two**\r\n\r\n**three**");
        doc.toggle_format(InlineFormat::Bold);
        assert_eq!(doc.text(), "one\r\n🙂 two\r\n\r\nthree");
        doc.undo();
        assert_eq!(doc.text(), "**one**\r\n**🙂 two**\r\n\r\n**three**");
        doc.undo();
        assert_eq!(doc.text(), "one\r\n🙂 two\r\n\r\nthree");
        assert_eq!(doc.selection().utf16(doc.text()), 0..end);
    }

    #[test]
    fn toolbar_commits_composition_then_formats_current_selection() {
        let mut doc = Document::new("AZ");
        doc.select(1..1);
        doc.mark(None, "漢🙂", Some(0..3));
        doc.toggle_format(InlineFormat::Bold);
        assert_eq!(doc.text(), "A<strong>漢🙂</strong>Z");
        assert!(doc.marked().is_none());
        doc.undo();
        assert_eq!(doc.text(), "A漢🙂Z");
        doc.undo();
        assert_eq!(doc.text(), "AZ");
    }
}
