use crate::{
    layout::{FontStyle, Fonts},
    Command, Header, Style,
};

use alloc::string::String;
use alloc::{vec, vec::*};
use embedded_graphics::{geometry::Point, primitives::Rectangle};
use text_layout::*;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
enum Box<'a> {
    Indent,
    Word { text: &'a str },
    Char { text: char },
}

#[derive(Debug)]
#[allow(dead_code)]
enum Penalty {
    SoftHyphen,
    HardHyphen,
    HardBreak,
}

// TODO: non-breaking spaces

pub struct Builder<'a, S: FontStyle, F: Fonts<Style = S>> {
    // Static info.
    /// Bounding box.
    bounding_box: Rectangle,
    /// Font store.
    fonts: F,
    /// Default style
    default_style: S,

    // Current style.
    /// Text style.
    style: S,
    /// Style ID.
    style_id: u16,
    /// Line height
    line_height: u16,
    /// Whitespace width.
    whitespace_width: f32,
    /// Whitespace stretch.
    whitespace_stretch: f32,
    /// Whitespace shrink.
    whitespace_shrink: f32,

    /// Current cursor.
    cursor: Point,

    // Styles
    styles: Vec<Style>,

    // Items
    items: Vec<Item<Box<'a>, (), Penalty>>,

    // Output
    commands: Vec<Command<String>>,
    pages: usize,
}

impl<'a, S: FontStyle, F: Fonts<Style = S>> Builder<'a, S, F> {
    /// Create a new document builder.
    pub fn new(bounding_box: Rectangle, fonts: F, default_style: S) -> Self {
        let styles = vec![Style {
            font_name: String::from(default_style.font_name()),
            em_px: default_style.em_px(),
        }];

        let line_height = default_style.line_height();
        let whitespace_width = default_style.em_px() as f32 / 3.0;
        let whitespace_stretch = whitespace_width / 2.0;
        let whitespace_shrink = whitespace_width / 3.0;

        Builder {
            bounding_box,
            fonts,
            default_style: default_style.clone(),
            style: default_style,
            style_id: 0,
            line_height,
            whitespace_width,
            whitespace_stretch,
            whitespace_shrink,
            cursor: Point::new(0, 0),
            styles,
            items: Vec::new(),
            commands: Vec::new(),
            pages: 0,
        }
    }

    fn get_style(&mut self, style: &Style) -> (S, u16) {
        let font_style = match self.fonts.get_style(style) {
            None => return (self.default_style.clone(), 0),
            Some(s) => s,
        };
        for i in 0..self.styles.len() {
            if self.styles[i] == *style {
                return (font_style, i as u16);
            }
        }
        self.styles.push(style.clone());
        (font_style, (self.styles.len() - 1) as u16)
    }

    pub fn finish(self) -> (Header, Vec<Command<String>>) {
        (
            Header {
                styles: self.styles,
            },
            self.commands,
        )
    }

    pub fn page_count(&self) -> usize {
        self.pages
    }

    pub fn paragraph_len(&self) -> usize {
        self.items.len()
    }

    pub fn set_style(&mut self, style: &Style) {
        let (style, id) = self.get_style(style);
        if id != self.style_id {
            let line_height = style.line_height();
            self.whitespace_width = style.em_px() as f32 / 3.0;
            self.whitespace_stretch = self.whitespace_width / 2.0;
            self.whitespace_shrink = self.whitespace_width / 3.0;

            self.commands.push(Command::SetStyle { s: id });
            if line_height != self.line_height {
                self.commands
                    .push(Command::SetLineHeight { h: line_height });
            }

            self.style = style;
            self.style_id = id;
        }
    }

    pub fn indent(&mut self, size: f32) {
        self.items.push(Item::Box {
            width: size * self.whitespace_width,
            data: Box::Indent,
        });
    }

    pub fn hard_line_break(&mut self) {
        // Append glue for a ragged-right terminator.
        self.items.push(Item::Glue {
            width: 0.0,
            stretch: f32::INFINITY,
            shrink: 0.0,
            data: (),
        });
        self.items.push(Item::Penalty {
            width: 0.0,
            cost: f32::NEG_INFINITY,
            flagged: true,
            data: Penalty::HardBreak,
        });
    }

    pub fn soft_line_break(&mut self) {
        self.whitespace();
    }

    pub fn whitespace(&mut self) {
        self.items.push(Item::Glue {
            width: self.whitespace_width,
            stretch: self.whitespace_stretch,
            shrink: self.whitespace_shrink,
            data: (),
        });
    }

    pub fn text(&mut self, s: &'a str) {
        for word in s.split_word_bounds() {
            self.word(word);
        }
    }

    pub fn word(&mut self, word: &'a str) {
        let is_whitespace = word.chars().all(|c: char| c.is_whitespace());
        if is_whitespace {
            self.whitespace();
        } else {
            let metrics = self.style.measure_string(word);
            let width = metrics.bounding_box.size.width;
            self.items.push(Item::Box {
                width: width as f32,
                data: Box::Word { text: word },
            });
            if word == "-" || word == "–" {
                self.items.push(Item::Penalty {
                    width: 0.0,
                    cost: 50.0,
                    flagged: true,
                    data: Penalty::HardHyphen,
                });
            }
        }
    }

    pub fn char(&mut self, c: char) {
        if c.is_whitespace() {
            self.whitespace();
        } else {
            let mut b = [0; 4];
            let metrics = self.style.measure_string(c.encode_utf8(&mut b));
            let width = metrics.bounding_box.size.width;
            self.items.push(Item::Box {
                width: width as f32,
                data: Box::Char { text: c },
            });
            if c == '-' || c == '–' {
                self.items.push(Item::Penalty {
                    width: 0.0,
                    cost: 50.0,
                    flagged: true,
                    data: Penalty::HardHyphen,
                });
            }
        }
    }

    pub fn paragraph_break(&mut self) {
        match self.items.len() {
            0 => return,
            1 => {
                if let Item::Box {
                    data: Box::Indent, ..
                } = self.items[0]
                {
                    self.items.clear();
                    return;
                }
            }
            _ => {}
        }

        // Append terminating glue.
        self.items.push(Item::Glue {
            width: 0.0,
            stretch: f32::INFINITY,
            shrink: 0.0,
            data: (),
        });
        self.items.push(Item::Penalty {
            width: 0.0,
            cost: f32::NEG_INFINITY,
            flagged: true,
            data: Penalty::HardBreak,
        });

        // Calculate line breaks.
        let breaks = KnuthPlass::new()
            .with_threshold(f32::INFINITY)
            .layout_paragraph(&self.items, self.bounding_box.size.width as f32);

        // Paginate.
        let mut item = 0;
        for b in breaks {
            let items = &self.items[item..=b.break_at];

            if self.commands.is_empty() {
                self.commands.push(Command::SetLineHeight {
                    h: self.line_height,
                });
            }

            self.commands.push(Command::SetAdjustmentRatio {
                r: b.adjustment_ratio,
            });

            // TODO: error diffusion for glue
            if !items.is_empty() {
                let mut text = String::new();
                for i in items.iter().take(items.len() - 1) {
                    match i {
                        Item::Box {
                            data: Box::Indent, ..
                        } => {
                            assert!(text.is_empty());
                            self.commands.push(Command::Advance {
                                dx: (self.whitespace_width * 4.0) as u16,
                            });
                        }
                        Item::Box {
                            data: Box::Word { text: word },
                            ..
                        } => {
                            text.push_str(word);
                        }
                        Item::Box {
                            data: Box::Char { text: char },
                            ..
                        } => {
                            text.push(*char);
                        }
                        Item::Glue { .. } => {
                            text.push(' ');
                        }
                        _ => {}
                    }
                }
                if let Item::Penalty {
                    data: Penalty::SoftHyphen,
                    ..
                } = &items[items.len() - 1]
                {
                    text.push('-');
                }
                self.commands.push(Command::Show { str: text });
            }

            self.advance_line();

            item = b.break_at + 1;
        }

        self.items.clear();
    }

    pub fn advance_line(&mut self) {
        let remaining = self.bounding_box.size.height as i32 - self.cursor.y;
        if remaining < self.line_height as i32 {
            self.page_break();
        } else {
            self.commands.push(Command::LineBreak);
            self.cursor += Point::new(0, self.line_height as i32);
        }
    }

    pub fn page_break(&mut self) {
        self.commands.push(Command::PageBreak);
        self.pages += 1;
        self.commands.push(Command::SetLineHeight {
            h: self.line_height,
        });
        self.commands.push(Command::SetStyle { s: self.style_id });
        self.cursor = Point::new(0, 0);
    }
}
