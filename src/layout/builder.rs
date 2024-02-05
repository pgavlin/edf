use crate::{
    layout::{FontStyle, Fonts},
    Command, Style,
};

use alloc::string::String;
use alloc::{vec, vec::*};
use embedded_graphics::{geometry::Point, primitives::Rectangle};
use text_layout::*;
use unicode_segmentation::UnicodeSegmentation;

pub trait Hyphenator {
    fn hyphenate(&self, word: &str, breaks: &mut Vec<usize>);
}

impl Hyphenator for () {
    fn hyphenate(&self, _word: &str, breaks: &mut Vec<usize>) {
        breaks.clear();
    }
}

#[derive(Debug)]
enum Box<'a> {
    Indent,
    SetStyle {
        id: u16,
        line_height: u16,
        baseline: u16,
    },
    Word {
        text: &'a str,
    },
    Char {
        text: char,
    },
}

#[derive(Debug)]
#[allow(dead_code)]
enum Penalty {
    SoftHyphen,
    HardHyphen,
    HardBreak,
}

// TODO: non-breaking spaces

pub struct Builder<S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> {
    // Static info.
    /// Bounding box.
    bounding_box: Rectangle,
    /// Font store.
    fonts: F,
    /// Default style
    default_style: S,
    /// Hyphenator
    hyphenator: H,

    // Current style.
    style: S,
    /// Style ID.
    style_id: u16,
    /// Line height
    line_height: u16,
    /// Baseline
    baseline: u16,
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

    // Output
    commands: Vec<Command<String>>,
    pages: usize,
}

impl<S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> Builder<S, F, H> {
    /// Create a new document builder.
    pub fn new(bounding_box: Rectangle, fonts: F, default_style: S, hyphenator: H) -> Self {
        let styles = vec![Style {
            font_name: String::from(default_style.font_name()),
            em_px: default_style.em_px(),
        }];

        let cursor = Point::new(0, default_style.em_px() as i32);

        let line_height = default_style.line_height();
        let baseline = default_style.baseline();
        let whitespace_width = default_style.em_px() as f32 / 3.0;
        let whitespace_stretch = whitespace_width / 2.0;
        let whitespace_shrink = whitespace_width / 3.0;

        Builder {
            bounding_box,
            fonts,
            default_style: default_style.clone(),
            hyphenator,
            style: default_style,
            style_id: 0,
            line_height,
            baseline,
            whitespace_width,
            whitespace_stretch,
            whitespace_shrink,
            cursor,
            styles,
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

    pub fn set_style(&mut self, style: &Style) {
        let (style, id) = self.get_style(style);
        if id != self.style_id {
            self.line_height = style.line_height();
            self.baseline = style.baseline();

            self.whitespace_width = style.em_px() as f32 / 3.0;
            self.whitespace_stretch = self.whitespace_width / 2.0;
            self.whitespace_shrink = self.whitespace_width / 3.0;

            self.style = style;
            self.style_id = id;

            self.commands.push(Command::SetStyle { s: id });
            self.commands.push(Command::SetLineMetrics {
                height: self.line_height,
                baseline: self.baseline,
            });
        }
    }

    pub fn finish(self) -> (Vec<Style>, Vec<Command<String>>) {
        (self.styles, self.commands)
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn page_count(&self) -> usize {
        self.pages
    }

    pub fn paragraph<'a>(self) -> ParagraphBuilder<'a, S, F, H> {
        let style = self.style.clone();
        let style_id = self.style_id;
        let whitespace_width = self.whitespace_width;
        let whitespace_stretch = self.whitespace_stretch;
        let whitespace_shrink = self.whitespace_shrink;

        ParagraphBuilder {
            builder: self,
            style,
            style_id,
            whitespace_width,
            whitespace_stretch,
            whitespace_shrink,
            breaks: Vec::new(),
            items: Vec::new(),
        }
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
        self.commands.push(Command::SetStyle { s: self.style_id });
        self.commands.push(Command::SetLineMetrics {
            height: self.line_height,
            baseline: self.baseline,
        });
        self.cursor = Point::new(0, 0);
    }
}

pub struct ParagraphBuilder<'a, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> {
    builder: Builder<S, F, H>,

    // Current style.
    style: S,
    /// Style ID.
    style_id: u16,
    /// Whitespace width.
    whitespace_width: f32,
    /// Whitespace stretch.
    whitespace_stretch: f32,
    /// Whitespace shrink.
    whitespace_shrink: f32,

    // Hyphenation buffer
    breaks: Vec<usize>,

    // Items
    items: Vec<Item<Box<'a>, (), Penalty>>,
}

impl<'a, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> ParagraphBuilder<'a, S, F, H> {
    pub fn set_style(&mut self, style: &Style) {
        let (style, id) = self.builder.get_style(style);
        if id != self.style_id {
            self.whitespace_width = style.em_px() as f32 / 3.0;
            self.whitespace_stretch = self.whitespace_width / 2.0;
            self.whitespace_shrink = self.whitespace_width / 3.0;

            self.items.push(Item::Box {
                width: 0.0,
                data: Box::SetStyle {
                    id,
                    line_height: style.line_height(),
                    baseline: style.baseline(),
                },
            });

            self.style = style;
            self.style_id = id;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn indent(&mut self, size: f32) {
        self.items.push(Item::Box {
            width: size * self.whitespace_width,
            data: Box::Indent,
        });
    }

    pub fn indent_px(&mut self, size: f32) {
        self.items.push(Item::Box {
            width: size,
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
            self.builder.hyphenator.hyphenate(word, &mut self.breaks);
            let word = if self.breaks.is_empty() {
                word
            } else {
                let mut last = 0;
                for offset in &self.breaks {
                    let sub = &word[last..*offset];
                    let metrics = self.style.measure_string(sub);
                    let width = metrics.bounding_box.size.width;
                    self.items.push(Item::Box {
                        width: width as f32,
                        data: Box::Word { text: sub },
                    });
                    self.items.push(Item::Penalty {
                        width: 0.0,
                        cost: 50.0,
                        flagged: true,
                        data: Penalty::SoftHyphen,
                    });
                    last = *offset;
                }
                &word[last..]
            };

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

    fn paragraph_break(&mut self) {
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
            .layout_paragraph(&self.items, self.builder.bounding_box.size.width as f32);

        let breaks = if breaks.is_empty() {
            FirstFit::new()
                .with_threshold(f32::INFINITY)
                .allow_overflow(true)
                .layout_paragraph(&self.items, self.builder.bounding_box.size.width as f32)
        } else {
            breaks
        };

        if breaks.is_empty() {
            for i in &self.items {
                println!("{:?}", i);
            }
            panic!("layout failed");
        }

        // Line metrics
        let mut current_line_height = self.builder.line_height;
        let mut current_baseline = self.builder.baseline;

        // Paginate.
        let mut item = 0;
        for b in breaks {
            let items = &self.items[item..=b.break_at];

            let mut commands = Vec::new();

            // TODO: error diffusion for glue
            let mut any_text = false;
            let mut push_line_metrics = false;
            if !items.is_empty() {
                let mut text = String::new();
                for i in items.iter().take(items.len() - 1) {
                    match i {
                        Item::Box {
                            data:
                                Box::SetStyle {
                                    id,
                                    line_height,
                                    baseline,
                                },
                            ..
                        } => {
                            if !text.is_empty() {
                                commands.push(Command::Show { str: text });
                                text = String::new();
                                any_text = true;
                            }
                            commands.push(Command::SetStyle { s: *id });

                            if !any_text && *line_height != current_line_height
                                || *line_height > current_line_height
                            {
                                current_line_height = *line_height;
                                current_baseline = *baseline;
                                push_line_metrics = true;
                            }
                        }
                        Item::Box {
                            width,
                            data: Box::Indent,
                        } => {
                            assert!(text.is_empty());
                            commands.push(Command::Advance { dx: *width as u16 });
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
                if !text.is_empty() {
                    commands.push(Command::Show { str: text });
                }

                if push_line_metrics {
                    self.builder.commands.push(Command::SetLineMetrics {
                        height: current_line_height,
                        baseline: current_baseline,
                    });
                }

                self.builder.commands.push(Command::SetAdjustmentRatio {
                    r: b.adjustment_ratio,
                });
                self.builder.commands.append(&mut commands);

                self.builder.line_height = current_line_height;
                self.builder.baseline = current_baseline;
            }

            self.builder.advance_line();

            item = b.break_at + 1;
        }

        self.items.clear();
    }

    pub fn finish(mut self) -> Builder<S, F, H> {
        self.paragraph_break();

        self.builder.style = self.style;
        self.builder.style_id = self.style_id;
        self.builder.whitespace_width = self.whitespace_width;
        self.builder.whitespace_stretch = self.whitespace_stretch;
        self.builder.whitespace_shrink = self.whitespace_shrink;

        self.builder
    }
}
