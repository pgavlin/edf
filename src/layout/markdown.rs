use crate::{
    layout::{Builder, FontStyle, Fonts, Hyphenator, ParagraphBuilder},
    Command, Header, Style,
};

use alloc::string::String;
use alloc::vec::*;
use embedded_graphics::primitives::Rectangle;
use markdown::event::{Event, Kind, Name};
use markdown::util::{
    constant::CHARACTER_REFERENCES,
    slice::{Position as SlicePosition, Slice},
};

#[derive(Debug)]
pub enum Error {
    Generic(&'static str),
}

pub struct Options {
    regular: Style,
    emphasis: Option<Style>,
    strong: Option<Style>,
    heading: Option<Vec<Style>>,
    title: Option<String>,
}

impl Options {
    pub fn new(regular: Style) -> Self {
        Options {
            regular,
            emphasis: None,
            strong: None,
            heading: None,
            title: None,
        }
    }

    pub fn with_emphasis(mut self, emphasis: Option<Style>) -> Self {
        self.emphasis = emphasis;
        self
    }

    pub fn with_strong(mut self, strong: Option<Style>) -> Self {
        self.strong = strong;
        self
    }

    pub fn with_heading(mut self, heading: Option<Vec<Style>>) -> Self {
        self.heading = heading;
        self
    }

    pub fn with_title<S: AsRef<str>>(mut self, title: Option<S>) -> Self {
        self.title = title.map(|s| s.as_ref().into());
        self
    }
}

enum BuilderState<'a, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> {
    None,
    Doc(Builder<S, F, H>),
    Paragraph(ParagraphBuilder<'a, S, F, H>),
}

impl<'a, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> BuilderState<'a, S, F, H> {
    fn map<Fn: FnOnce(Self) -> Self>(&mut self, f: Fn) {
        let next = f(core::mem::replace(self, BuilderState::None));
        let _ = core::mem::replace(self, next);
    }

    fn paragraph(&mut self) -> &mut ParagraphBuilder<'a, S, F, H> {
        match self {
            BuilderState::Paragraph(ref mut p) => p,
            _ => panic!("builder is not in a paragraph"),
        }
    }
}

/// Context used to lay out markdown.
#[allow(clippy::struct_excessive_bools)]
struct LayoutContext<'a, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> {
    events: &'a [Event],
    bytes: &'a [u8],
    options: Options,
    builder: BuilderState<'a, S, F, H>,
    heading_level: u8,
    character_reference_marker: u8,
    // Current event index.
    index: usize,
    in_paragraph: bool,
    in_link_destination: bool,
}

impl<'a, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> LayoutContext<'a, S, F, H> {
    /// Create a new layout context.
    fn new(
        events: &'a [Event],
        bytes: &'a [u8],
        options: Options,
        builder: Builder<S, F, H>,
    ) -> Self {
        LayoutContext {
            events,
            bytes,
            options,
            builder: BuilderState::Doc(builder),
            heading_level: 0,
            character_reference_marker: 0,
            index: 0,
            in_paragraph: false,
            in_link_destination: false,
        }
    }
}

/// Turn events and bytes into an edf document.
pub fn build<S: FontStyle, F: Fonts<Style = S>, H: Hyphenator>(
    events: &[Event],
    bytes: &[u8],
    bounding_box: Rectangle,
    fonts: F,
    hyphenator: H,
    options: Options,
) -> Result<(Header, Vec<Command<String>>), Error> {
    let default_style = match fonts.get_style(&options.regular) {
        None => return Err(Error::Generic("missing font for regular style")),
        Some(s) => s,
    };

    let builder = Builder::new(bounding_box, fonts, default_style, hyphenator);
    let mut context = LayoutContext::new(events, bytes, options, builder);

    let mut index = 0;
    while index < events.len() {
        Handlers::handle(&mut context, index);
        index += 1;
    }

    let builder = match context.builder {
        BuilderState::Paragraph(p) => p.finish(),
        BuilderState::Doc(b) => b,
        _ => panic!("unexpected state"),
    };
    let (styles, commands) = builder.finish();
    let title = context.options.title.unwrap_or("Untitled".into());
    let header = Header { styles, title };
    Ok((header, commands))
}

struct Handlers<S, F, H> {
    phantom: core::marker::PhantomData<(S, F, H)>,
}

impl<S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> Handlers<S, F, H> {
    /// Handle the event at `index`.
    fn handle(context: &mut LayoutContext<S, F, H>, index: usize) {
        context.index = index;

        if context.events[index].kind == Kind::Enter {
            Self::enter(context);
        } else {
            Self::exit(context);
        }
    }

    /// Handle [`Enter`][Kind::Enter].
    fn enter(context: &mut LayoutContext<S, F, H>) {
        let event = &context.events[context.index];

        match event.name {
            // Flow content

            // Block quote
            //
            // Add block quoute level.
            Name::BlockQuote => {} // TODO

            // Setext Heading
            //
            // Reset the heading level.
            Name::HeadingSetext => context.heading_level = 0,

            // Heading text
            //
            // Begin new paragraph.
            Name::HeadingAtxText | Name::HeadingSetextText => Self::on_enter_heading(context),

            // List
            //
            // Add list level.
            Name::ListOrdered | Name::ListUnordered => {} // TODO

            // Paragraph
            //
            // Begin a new paragraph + add an indent.
            Name::Paragraph => Self::on_enter_paragraph(context),

            // List content

            // ListItem
            //
            // Unclear. Something about bboxes. Ignore for now.
            Name::ListItem => {} // TODO

            // Text content
            Name::CharacterReferenceMarker => context.character_reference_marker = b'&',
            Name::CharacterReferenceMarkerHexadecimal => context.character_reference_marker = b'x',
            Name::CharacterReferenceMarkerNumeric => context.character_reference_marker = b'#',

            // Emphasis
            //
            // Push bold style.
            Name::Emphasis => {
                if let Some(ref style) = context.options.emphasis {
                    context.builder.paragraph().set_style(style);
                }
            }

            // Label
            //
            // Push label style, if any.
            Name::Label => {} // TODO

            // Reference, Resource
            //
            // Ignore.
            Name::Reference | Name::Resource => context.in_link_destination = true,

            // Strong
            //
            // Push bold style.
            Name::Strong => {
                if let Some(ref style) = context.options.strong {
                    context.builder.paragraph().set_style(style);
                }
            }

            // Ignore everything else.
            _ => {}
        };
    }

    /// Handle [`Exit`][Kind::Exit].
    fn exit(context: &mut LayoutContext<S, F, H>) {
        match context.events[context.index].name {
            // Flow content

            // Block quote
            //
            // Remove block quoute level.
            Name::BlockQuote => {} // TODO

            // Code block
            //
            // Layout code block.
            Name::CodeFlowChunk => {} // TODO

            // Atx Heading Sequence
            //
            // Set the current heading level.
            Name::HeadingAtxSequence => Self::on_exit_atx_heading_sequence(context),

            // Heading
            //
            // Layout current paragraph and pop heading style.
            Name::HeadingAtx | Name::HeadingSetext => Self::on_exit_heading(context),

            // List
            //
            // Remove list level.
            Name::ListOrdered | Name::ListUnordered => {} // TODO

            // Paragraph
            //
            // Layout current paragraph.
            Name::Paragraph => Self::on_exit_paragraph(context),

            // Thematic break
            //
            // Draw a thematic break.
            Name::ThematicBreak => Self::on_exit_thematic_break(context),

            // Text content

            // Autolink values
            //
            // Layout content in label style, if any.
            Name::AutolinkEmail | Name::AutolinkProtocol => Self::on_exit_autolink(context),

            // Verbatim text
            //
            // Itemize into current paragraph.
            Name::CharacterEscapeValue | Name::Data => Self::on_exit_data(context),

            // Character references
            //
            // Process reference and itemize into current paragraph.
            Name::CharacterReferenceValue => Self::on_exit_character_reference(context),

            // Inline code
            //
            // Itemize as a single box into current paragraph.
            Name::CodeTextData => {} // TODO

            // Emphasis
            //
            // Pop bold style.
            Name::Emphasis => context
                .builder
                .paragraph()
                .set_style(&context.options.regular),

            // Hard breaks
            //
            // Push a mandatory break into the current paragraph.
            Name::HardBreakEscape | Name::HardBreakTrailing => Self::on_exit_hard_break(context),

            // Image
            //
            // Unclear. Replace with label for now.
            Name::Image => {} // TODO

            // Label
            //
            // Push label style, if any.
            Name::Label => {} // TODO

            // Link
            //
            // Unclear. Replace with label for now.
            Name::Link => {} // TODO

            // Reference, Resource
            //
            // Ignore.
            Name::Reference | Name::Resource => context.in_link_destination = false,

            // Strong
            //
            // Pop bold style.
            Name::Strong => context
                .builder
                .paragraph()
                .set_style(&context.options.regular),

            // Line endings
            Name::LineEnding => Self::on_exit_line_ending(context),

            // Ignore everything else.
            _ => {}
        };
    }

    fn on_exit_atx_heading_sequence(context: &mut LayoutContext<S, F, H>) {
        let event_pos = SlicePosition::from_exit_event(context.events, context.index);
        let slice = Slice::from_position(context.bytes, &event_pos);
        context.heading_level = slice.as_str().len() as u8;
    }

    fn on_enter_heading(context: &mut LayoutContext<S, F, H>) {
        // Start a new paragraph.
        context.in_paragraph = true;

        context.builder.map(|b| match b {
            BuilderState::Doc(mut doc) => {
                if !doc.is_empty() {
                    doc.page_break();
                }
                BuilderState::Paragraph(doc.paragraph())
            }
            _ => panic!("expected a document builder"),
        });

        if let Some(ref heading) = context.options.heading {
            let level = context.heading_level.saturating_sub(1) as usize;
            if level < heading.len() {
                context
                    .builder
                    .paragraph()
                    .set_style(&heading[level]);
            }
        }
    }

    fn on_exit_heading(context: &mut LayoutContext<S, F, H>) {
        context.in_paragraph = false;
        context.builder.map(|b| match b {
            BuilderState::Paragraph(p) => {
                let mut doc = p.finish();
                doc.advance_line();
                doc.set_style(&context.options.regular);
                BuilderState::Doc(doc)
            }
            _ => panic!("expected a paragraph builder"),
        });
    }

    fn on_enter_paragraph(context: &mut LayoutContext<S, F, H>) {
        // Start a new paragraph and push an indent.
        context.in_paragraph = true;

        context.builder.map(|b| match b {
            BuilderState::Doc(doc) => BuilderState::Paragraph(doc.paragraph()),
            _ => panic!("expected a document builder"),
        });

        context.builder.paragraph().indent(4.0);
    }

    fn on_exit_paragraph(context: &mut LayoutContext<S, F, H>) {
        context.in_paragraph = false;

        context.builder.map(|b| match b {
            BuilderState::Paragraph(p) => BuilderState::Doc(p.finish()),
            _ => panic!("expected a paragraph builder"),
        });
    }

    fn on_exit_thematic_break(_context: &mut LayoutContext<S, F, H>) {
        // TODO
    }

    fn on_exit_autolink(context: &mut LayoutContext<S, F, H>) {
        // TODO: push label style
        Self::on_exit_data(context);
        // TODO: pop label style
    }

    fn on_exit_data(context: &mut LayoutContext<S, F, H>) {
        if !context.in_link_destination {
            let event_pos = SlicePosition::from_exit_event(context.events, context.index);
            let slice = Slice::from_position(context.bytes, &event_pos);
            context.builder.paragraph().text(slice.as_str());

            if context.heading_level == 1 && context.options.title.is_none() {
                context.options.title = Some(slice.as_str().into());
            }
        }
    }

    fn on_exit_character_reference(context: &mut LayoutContext<S, F, H>) {
        let event_pos = SlicePosition::from_exit_event(context.events, context.index);
        let slice = Slice::from_position(context.bytes, &event_pos);
        match context.character_reference_marker {
            b'#' => context
                .builder
                .paragraph()
                .char(decode_numeric_char(slice.as_str(), 10)),
            b'x' => context
                .builder
                .paragraph()
                .char(decode_numeric_char(slice.as_str(), 16)),
            b'&' => {
                if let Some(v) = decode_named_char(slice.as_str()) {
                    context.builder.paragraph().word(v);
                }
            }
            _ => unreachable!("Unexpected marker `{}`", context.character_reference_marker),
        };
    }

    fn on_exit_hard_break(context: &mut LayoutContext<S, F, H>) {
        context.builder.paragraph().hard_line_break();
    }

    fn on_exit_line_ending(context: &mut LayoutContext<S, F, H>) {
        if context.in_paragraph {
            context.builder.paragraph().soft_line_break();
        }
    }
}

fn decode_numeric_char(value: &str, radix: u32) -> char {
    if let Some(c) = char::from_u32(u32::from_str_radix(value, radix).unwrap()) {
        if !matches!(c,
            // C0 except for HT, LF, FF, CR, space.
            '\0'..='\u{08}' | '\u{0B}' | '\u{0E}'..='\u{1F}' |
            // Control character (DEL) of C0, and C1 controls.
            '\u{7F}'..='\u{9F}'
            // Lone surrogates, noncharacters, and out of range are handled by
            // Rust.
        ) {
            return c;
        }
    }

    char::REPLACEMENT_CHARACTER
}

fn decode_named_char(value: &str) -> Option<&'static str> {
    CHARACTER_REFERENCES
        .iter()
        .find(|d| d.0 == value)
        .map(|d| d.1)
}
