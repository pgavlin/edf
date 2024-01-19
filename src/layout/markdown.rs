use crate::{
    layout::{Builder, FontStyle, Fonts},
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
}

impl Options {
    pub fn new(regular: Style) -> Self {
        Options {
            regular,
            emphasis: None,
            strong: None,
            heading: None,
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
}

/// Context used to lay out markdown.
#[allow(clippy::struct_excessive_bools)]
struct LayoutContext<'a, S: FontStyle, F: Fonts<Style = S>> {
    events: &'a [Event],
    bytes: &'a [u8],
    options: Options,
    builder: Builder<'a, S, F>,
    heading_level: u8,
    character_reference_marker: u8,
    // Current event index.
    index: usize,
    in_paragraph: bool,
}

impl<'a, S: FontStyle, F: Fonts<Style = S>> LayoutContext<'a, S, F> {
    /// Create a new layout context.
    fn new(
        events: &'a [Event],
        bytes: &'a [u8],
        options: Options,
        builder: Builder<'a, S, F>,
    ) -> Self {
        LayoutContext {
            events,
            bytes,
            options,
            builder,
            heading_level: 0,
            character_reference_marker: 0,
            index: 0,
            in_paragraph: false,
        }
    }
}

/// Turn events and bytes into an edf document.
pub fn build<S: FontStyle, F: Fonts<Style = S>>(
    events: &[Event],
    bytes: &[u8],
    bounding_box: Rectangle,
    fonts: F,
    options: Options,
) -> Result<(Header, Vec<Command<String>>), Error> {
    let default_style = match fonts.get_style(&options.regular) {
        None => return Err(Error::Generic("missing font for regular style")),
        Some(s) => s,
    };

    let builder = Builder::new(bounding_box, fonts, default_style);
    let mut context = LayoutContext::new(events, bytes, options, builder);

    let mut index = 0;
    while index < events.len() {
        handle(&mut context, index);
        index += 1;
    }

    Ok(context.builder.finish())
}

/// Handle the event at `index`.
fn handle<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>, index: usize) {
    context.index = index;

    if context.events[index].kind == Kind::Enter {
        enter(context);
    } else {
        exit(context);
    }
}

/// Handle [`Enter`][Kind::Enter].
fn enter<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
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
        Name::HeadingAtxText | Name::HeadingSetextText => on_enter_heading(context),

        // List
        //
        // Add list level.
        Name::ListOrdered | Name::ListUnordered => {} // TODO

        // Paragraph
        //
        // Begin a new paragraph + add an indent.
        Name::Paragraph => on_enter_paragraph(context),

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
                context.builder.set_style(style);
            }
        }

        // Label
        //
        // Push label style, if any.
        Name::Label => {} // TODO

        // Strong
        //
        // Push bold style.
        Name::Strong => {
            if let Some(ref style) = context.options.strong {
                context.builder.set_style(style);
            }
        }

        // Ignore everything else.
        _ => {}
    };
}

/// Handle [`Exit`][Kind::Exit].
fn exit<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
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
        Name::HeadingAtxSequence => on_exit_atx_heading_sequence(context),

        // Heading
        //
        // Layout current paragraph and pop heading style.
        Name::HeadingAtx | Name::HeadingSetext => on_exit_heading(context),

        // List
        //
        // Remove list level.
        Name::ListOrdered | Name::ListUnordered => {} // TODO

        // Paragraph
        //
        // Layout current paragraph.
        Name::Paragraph => on_exit_paragraph(context),

        // Thematic break
        //
        // Draw a thematic break.
        Name::ThematicBreak => on_exit_thematic_break(context),

        // Text content

        // Autolink values
        //
        // Layout content in label style, if any.
        Name::AutolinkEmail | Name::AutolinkProtocol => on_exit_autolink(context),

        // Verbatim text
        //
        // Itemize into current paragraph.
        Name::CharacterEscapeValue | Name::Data => on_exit_data(context),

        // Character references
        //
        // Process reference and itemize into current paragraph.
        Name::CharacterReferenceValue => on_exit_character_reference(context),

        // Inline code
        //
        // Itemize as a single box into current paragraph.
        Name::CodeTextData => {} // TODO

        // Emphasis
        //
        // Pop bold style.
        Name::Emphasis => context.builder.set_style(&context.options.regular),

        // Hard breaks
        //
        // Push a mandatory break into the current paragraph.
        Name::HardBreakEscape | Name::HardBreakTrailing => on_exit_hard_break(context),

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

        // Strong
        //
        // Pop bold style.
        Name::Strong => context.builder.set_style(&context.options.regular),

        // Line endings
        Name::LineEnding => on_exit_line_ending(context),

        // Ignore everything else.
        _ => {}
    };
}

fn on_exit_atx_heading_sequence<S: FontStyle, F: Fonts<Style = S>>(
    context: &mut LayoutContext<S, F>,
) {
    let event_pos = SlicePosition::from_exit_event(context.events, context.index);
    let slice = Slice::from_position(context.bytes, &event_pos);
    context.heading_level = match slice.as_str().len() {
        0 => 0,
        n => (n - 1) as u8,
    };
}

fn on_enter_heading<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
    // Start a new paragraph.
    context.in_paragraph = true;
    assert!(context.builder.paragraph_len() == 0);

    if context.builder.page_count() != 0 {
        context.builder.page_break();
    }

    if let Some(ref heading) = context.options.heading {
        if (context.heading_level as usize) < heading.len() {
            context
                .builder
                .set_style(&heading[context.heading_level as usize]);
        }
    }
}

fn on_exit_heading<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
    context.in_paragraph = false;
    context.builder.paragraph_break();
    context.builder.advance_line();
    context.builder.set_style(&context.options.regular);
}

fn on_enter_paragraph<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
    // Start a new paragraph and push an indent.
    context.in_paragraph = true;
    assert!(context.builder.paragraph_len() == 0);

    context.builder.indent(4.0);
}

fn on_exit_paragraph<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
    context.in_paragraph = false;
    context.builder.paragraph_break();
}

fn on_exit_thematic_break<S: FontStyle, F: Fonts<Style = S>>(_context: &mut LayoutContext<S, F>) {
    // TODO
}

fn on_exit_autolink<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
    // TODO: push label style
    on_exit_data(context);
    // TODO: pop label style
}

fn on_exit_data<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
    let event_pos = SlicePosition::from_exit_event(context.events, context.index);
    let slice = Slice::from_position(context.bytes, &event_pos);
    context.builder.text(slice.as_str());
}

fn on_exit_character_reference<S: FontStyle, F: Fonts<Style = S>>(
    context: &mut LayoutContext<S, F>,
) {
    let event_pos = SlicePosition::from_exit_event(context.events, context.index);
    let slice = Slice::from_position(context.bytes, &event_pos);
    match context.character_reference_marker {
        b'#' => context
            .builder
            .char(decode_numeric_char(slice.as_str(), 10)),
        b'x' => context
            .builder
            .char(decode_numeric_char(slice.as_str(), 16)),
        b'&' => {
            if let Some(v) = decode_named_char(slice.as_str()) {
                context.builder.word(v);
            }
        }
        _ => unreachable!("Unexpected marker `{}`", context.character_reference_marker),
    };
}

fn on_exit_hard_break<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
    context.builder.hard_line_break();
}

fn on_exit_line_ending<S: FontStyle, F: Fonts<Style = S>>(context: &mut LayoutContext<S, F>) {
    if context.in_paragraph {
        context.builder.soft_line_break();
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
