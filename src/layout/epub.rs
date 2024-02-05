#[allow(dead_code)]
use crate::{
    layout::{Align, Builder, FontStyle, Fonts, Hyphenator, ParagraphBuilder, ParagraphOptions},
    Command, Header, Style,
};

use ego_tree::NodeRef;
use embedded_graphics::primitives::Rectangle;
use epub::doc::EpubDoc;
use scraper::{html::Html, node::Text, Node};
use selectors::matching;
use servo_arc::Arc;
use servo_url::ServoUrl;
use std::error::Error;
use std::io::{Read, Seek};
use style::{
    context::QuirksMode,
    media_queries::MediaList,
    shared_lock::{Locked, SharedRwLock},
    stylesheet_set::DocumentStylesheetSet,
    stylesheets::{
        AllowImportRules, CssRule, DocumentStyleSheet, Origin, StyleRule, Stylesheet,
        StylesheetInDocument,
    },
};
use url::Url;

mod element;
use element::Element;
mod element_style;
use element_style::{ComputeContext, ComputedStyle, FontAngle, TextAlign};

pub struct Options {
    pixels_per_inch: f32,
    regular: Style,
    emphasis: Option<Style>,
    strong: Option<Style>,
    heading: Option<Vec<Style>>,
    title: Option<String>,
}

impl Options {
    pub fn new(pixels_per_inch: f32, regular: Style) -> Self {
        Options {
            pixels_per_inch,
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

    fn take(self) -> Builder<S, F, H> {
        match self {
            BuilderState::Doc(builder) => builder,
            _ => panic!("builder is in a paragraph"),
        }
    }

    fn paragraph(&mut self) -> &mut ParagraphBuilder<'a, S, F, H> {
        match self {
            BuilderState::Paragraph(ref mut p) => p,
            _ => panic!("builder is not in a paragraph"),
        }
    }

    fn if_paragraph<Fn: FnOnce(&mut ParagraphBuilder<'a, S, F, H>)>(&mut self, func: Fn) {
        if let BuilderState::Paragraph(ref mut p) = self {
            func(p);
        }
    }

    fn if_doc<Fn: FnOnce(&mut Builder<S, F, H>)>(&mut self, func: Fn) {
        if let BuilderState::Doc(ref mut d) = self {
            func(d);
        }
    }
}

/// Context used to lay out epub documents.
#[allow(clippy::struct_excessive_bools)]
struct LayoutContext<'a, R: Read + Seek, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> {
    doc: &'a mut EpubDoc<R>,
    options: &'a Options,
    base_url: &'a Url,
    builder: BuilderState<'a, S, F, H>,
    content_width: u32,
    lock: SharedRwLock,
    stylesheets: DocumentStylesheetSet<DocumentStyleSheet>,
    computed_style: Vec<ComputedStyle>,
}

impl<'a, R: Read + Seek, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator>
    LayoutContext<'a, R, S, F, H>
{
    /// Create a new layout context.
    fn new(
        doc: &'a mut EpubDoc<R>,
        options: &'a Options,
        base_url: &'a Url,
        builder: Builder<S, F, H>,
        content_width: u32,
    ) -> Self {
        let computed_style = ComputedStyle::new(options.regular.em_px as f32);
        LayoutContext {
            doc,
            options,
            base_url,
            builder: BuilderState::Doc(builder),
            content_width,
            lock: SharedRwLock::new(),
            stylesheets: DocumentStylesheetSet::new(),
            computed_style: vec![computed_style],
        }
    }

    fn push_style(&mut self, element: Element) -> Style {
        let mut nth_index_cache = Default::default();
        let mut context = matching::MatchingContext::new(
            matching::MatchingMode::Normal,
            None,
            &mut nth_index_cache,
            matching::QuirksMode::NoQuirks,
            matching::NeedsSelectorFlags::No,
        );

        let guard = self.lock.read();
        let styles = self
            .stylesheets
            .iter()
            .map(|sheet| {
                sheet.0.rules(&guard).iter().filter_map(|rule| match rule {
                    CssRule::Style(style) => Some(style),
                    _ => None,
                })
            })
            .flatten()
            .filter_map(|style| {
                style.read_with(&guard).selectors.0.iter().find_map(|sel| {
                    matching::matches_selector(sel, 0, None, &element, &mut context)
                        .then_some((style, sel))
                })
            });

        let mut style: Option<(Arc<Locked<StyleRule>>, u32)> = None;
        for this_style in styles {
            style = match style {
                Some(style) if style.1 > this_style.1.specificity() => Some(style),
                _ => Some((this_style.0.clone(), this_style.1.specificity())),
            };
        }

        let top = self.computed_style[self.computed_style.len() - 1];
        let style = match style {
            None => top.clone(),
            Some((ref style, _)) => {
                let guard = self.lock.read();
                let block = style.read_with(&guard).block.read_with(&guard);
                top.compute(
                    block,
                    &ComputeContext {
                        pixels_per_inch: self.options.pixels_per_inch,
                        container_width: self.content_width as f32,
                    },
                )
            }
        };

        eprintln!("push({:?})", style);

        self.computed_style.push(style);
        self.as_style(&self.computed_style[self.computed_style.len() - 1])
    }

    fn pop_style(&mut self) -> Style {
        self.computed_style.pop();
        self.as_style(&self.computed_style[self.computed_style.len() - 1])
    }

    fn as_style(&self, style: &ComputedStyle) -> Style {
        let font_name = match style.font_style {
            FontAngle::Normal => match &self.options.strong {
                Some(strong) if style.font_weight.0 >= 600.0 => strong.font_name.clone(),
                _ => self.options.regular.font_name.clone(),
            },
            FontAngle::Italic => match &self.options.emphasis {
                None => self.options.regular.font_name.clone(),
                Some(emphasis) => emphasis.font_name.clone(),
            },
        }
        .clone();

        let em_px: u16 = style.font_size.0 as u16;

        Style { font_name, em_px }
    }
}

/// Turn events and bytes into an edf document.
pub fn build<R: Read + Seek, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator>(
    doc: &mut EpubDoc<R>,
    bounding_box: Rectangle,
    fonts: F,
    hyphenator: H,
    options: Options,
) -> Result<(Header, Vec<Command<String>>), Box<dyn Error>> {
    let default_style = match fonts.get_style(&options.regular) {
        None => return Err("missing font for regular style".into()),
        Some(s) => s,
    };

    let root_url = Url::parse("epub://").unwrap();
    let mut builder = Builder::new(bounding_box, fonts, default_style, hyphenator);

    while doc.go_next() {
        let path = match doc.get_current_path() {
            None => continue,
            Some(path) => path,
        };
        let content = match doc.get_current_str() {
            None => continue,
            Some((content, _)) => content,
        };

        if !builder.is_empty() {
            builder.page_break();
        }

        let base_path = path
            .as_path()
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("");
        let base_url = Url::parse(&format!("epub:///{}/", base_path)).unwrap_or(root_url.clone());

        let mut context =
            LayoutContext::new(doc, &options, &base_url, builder, bounding_box.size.width);
        let doc = Html::parse_document(&content);
        let root = doc
            .tree
            .root()
            .children()
            .find(|child| child.value().is_element())
            .expect("html node missing");
        Handlers::html(Element::new(root), &mut context);
        builder = context.builder.take();
    }

    let (styles, commands) = builder.finish();
    let title = options.title.unwrap_or("Untitled".into());
    let header = Header { styles, title };
    Ok((header, commands))
}

struct Handlers<R, S, F, H> {
    phantom: core::marker::PhantomData<(R, S, F, H)>,
}

impl<'a, R: Read + Seek, S: FontStyle, F: Fonts<Style = S>, H: Hyphenator> Handlers<R, S, F, H> {
    fn push_style(element: Element, context: &mut LayoutContext<'a, R, S, F, H>) {
        let style = context.push_style(element);
        match context.builder {
            BuilderState::Doc(ref mut doc) => doc.set_style(&style),
            BuilderState::Paragraph(ref mut p) => p.set_style(&style),
            _ => unreachable!(),
        };
    }

    fn pop_style(context: &mut LayoutContext<'a, R, S, F, H>) {
        let style = context.pop_style();
        match context.builder {
            BuilderState::Doc(ref mut doc) => doc.set_style(&style),
            BuilderState::Paragraph(ref mut p) => p.set_style(&style),
            _ => unreachable!(),
        };
    }

    fn metadata_content(node: NodeRef<'a, Node>, context: &mut LayoutContext<'a, R, S, F, H>) {
        node.value().is_element().then(|| {
            let elem = Element::new(node);
            eprintln!("metadata: {}", elem.value().name());
            match elem.value().name() {
                "base" => Self::base(elem, context),
                "link" => Self::link(elem, context),
                "meta" => Self::meta(elem, context),
                "noscript" => Self::noscript(elem, context),
                "script" => Self::script(elem, context),
                "style" => Self::style(elem, context),
                "template" => Self::template(elem, context),
                "title" => Self::title(elem, context),
                _ => (),
            }
        });
    }

    fn flow_content(node: NodeRef<'a, Node>, context: &mut LayoutContext<'a, R, S, F, H>) {
        match node.value() {
            Node::Text(text) => Self::text(text, context),
            Node::Element(_) => {
                let elem = Element::new(node);
                Self::push_style(elem, context);
                eprintln!("flow: {}", elem.value().name());
                match elem.value().name() {
                    "a" => Self::a(elem, Self::flow_content, context),
                    "abbr" => Self::abbr(elem, context),
                    "address" => Self::address(elem, context),
                    "area" => Self::area(elem, context),
                    "article" => Self::article(elem, context),
                    "aside" => Self::aside(elem, context),
                    "audio" => Self::audio(elem, context),
                    "b" => Self::b(elem, context),
                    "bdi" => Self::bdi(elem, context),
                    "bdo" => Self::bdo(elem, context),
                    "blockquote" => Self::blockquote(elem, context),
                    "br" => Self::br(elem, context),
                    "button" => Self::button(elem, context),
                    "canvas" => Self::canvas(elem, context),
                    "cite" => Self::cite(elem, context),
                    "code" => Self::code(elem, context),
                    "data" => Self::data(elem, context),
                    "datalist" => Self::datalist(elem, context),
                    "del" => Self::del(elem, context),
                    "details" => Self::details(elem, context),
                    "dfn" => Self::dfn(elem, context),
                    "dialog" => Self::dialog(elem, context),
                    "div" => Self::div(elem, context),
                    "dl" => Self::dl(elem, context),
                    "em" => Self::em(elem, context),
                    "embed" => Self::embed(elem, context),
                    "fieldset" => Self::fieldset(elem, context),
                    "figure" => Self::figure(elem, context),
                    "footer" => Self::footer(elem, context),
                    "form" => Self::form(elem, context),
                    "h1" => Self::h1(elem, context),
                    "h2" => Self::h2(elem, context),
                    "h3" => Self::h3(elem, context),
                    "h4" => Self::h4(elem, context),
                    "h5" => Self::h5(elem, context),
                    "h6" => Self::h6(elem, context),
                    "header" => Self::header(elem, context),
                    "hgroup" => Self::hgroup(elem, context),
                    "hr" => Self::hr(elem, context),
                    "i" => Self::i(elem, context),
                    "iframe" => Self::iframe(elem, context),
                    "img" => Self::img(elem, context),
                    "input" => Self::input(elem, context),
                    "ins" => Self::ins(elem, context),
                    "kbd" => Self::kbd(elem, context),
                    "label" => Self::label(elem, context),
                    "link" => Self::link(elem, context),
                    "main" => Self::main(elem, context),
                    "map" => Self::map(elem, context),
                    "mark" => Self::mark(elem, context),
                    "menu" => Self::menu(elem, context),
                    "meta" => Self::meta(elem, context),
                    "meter" => Self::meter(elem, context),
                    "nav" => Self::nav(elem, context),
                    "noscript" => Self::noscript(elem, context),
                    "object" => Self::object(elem, context),
                    "ol" => Self::ol(elem, context),
                    "output" => Self::output(elem, context),
                    "p" => Self::p(elem, context),
                    "picture" => Self::picture(elem, context),
                    "pre" => Self::pre(elem, context),
                    "progress" => Self::progress(elem, context),
                    "q" => Self::q(elem, context),
                    "ruby" => Self::ruby(elem, context),
                    "s" => Self::s(elem, context),
                    "samp" => Self::samp(elem, context),
                    "script" => Self::script(elem, context),
                    "search" => Self::search(elem, context),
                    "section" => Self::section(elem, context),
                    "select" => Self::select(elem, context),
                    "slot" => Self::slot(elem, context),
                    "small" => Self::small(elem, context),
                    "span" => Self::span(elem, context),
                    "strong" => Self::strong(elem, context),
                    "sub" => Self::sub(elem, context),
                    "sup" => Self::sup(elem, context),
                    "table" => Self::table(elem, context),
                    "template" => Self::template(elem, context),
                    "textarea" => Self::textarea(elem, context),
                    "time" => Self::time(elem, context),
                    "u" => Self::u(elem, context),
                    "ul" => Self::ul(elem, context),
                    "var" => Self::var(elem, context),
                    "video" => Self::video(elem, context),
                    "wbr" => Self::wbr(elem, context),
                    _ => (),
                }
                Self::pop_style(context);
            }
            _ => (),
        }
    }

    fn phrasing_content(node: NodeRef<'a, Node>, context: &mut LayoutContext<'a, R, S, F, H>) {
        match node.value() {
            Node::Text(text) => Self::text(text, context),
            Node::Element(_) => {
                let elem = Element::new(node);
                Self::push_style(elem, context);
                eprintln!("phrasing: {}", elem.value().name());
                match elem.value().name() {
                    "a" => Self::a(elem, Self::phrasing_content, context),
                    "abbr" => Self::abbr(elem, context),
                    "area" => Self::area(elem, context),
                    "audio" => Self::audio(elem, context),
                    "b" => Self::b(elem, context),
                    "bdi" => Self::bdi(elem, context),
                    "bdo" => Self::bdo(elem, context),
                    "br" => Self::br(elem, context),
                    "button" => Self::button(elem, context),
                    "canvas" => Self::canvas(elem, context),
                    "cite" => Self::cite(elem, context),
                    "code" => Self::code(elem, context),
                    "data" => Self::data(elem, context),
                    "datalist" => Self::datalist(elem, context),
                    "del" => Self::del(elem, context),
                    "dfn" => Self::dfn(elem, context),
                    "em" => Self::em(elem, context),
                    "embed" => Self::embed(elem, context),
                    "i" => Self::i(elem, context),
                    "iframe" => Self::iframe(elem, context),
                    "img" => Self::img(elem, context),
                    "input" => Self::input(elem, context),
                    "ins" => Self::ins(elem, context),
                    "kbd" => Self::kbd(elem, context),
                    "label" => Self::label(elem, context),
                    "link" => Self::link(elem, context),
                    "map" => Self::map(elem, context),
                    "mark" => Self::mark(elem, context),
                    "meta" => Self::meta(elem, context),
                    "meter" => Self::meter(elem, context),
                    "noscript" => Self::noscript(elem, context),
                    "object" => Self::object(elem, context),
                    "output" => Self::output(elem, context),
                    "picture" => Self::picture(elem, context),
                    "progress" => Self::progress(elem, context),
                    "q" => Self::q(elem, context),
                    "ruby" => Self::ruby(elem, context),
                    "s" => Self::s(elem, context),
                    "samp" => Self::samp(elem, context),
                    "script" => Self::script(elem, context),
                    "select" => Self::select(elem, context),
                    "slot" => Self::slot(elem, context),
                    "small" => Self::small(elem, context),
                    "span" => Self::span(elem, context),
                    "strong" => Self::strong(elem, context),
                    "sub" => Self::sub(elem, context),
                    "sup" => Self::sup(elem, context),
                    "template" => Self::template(elem, context),
                    "textarea" => Self::textarea(elem, context),
                    "time" => Self::time(elem, context),
                    "u" => Self::u(elem, context),
                    "var" => Self::var(elem, context),
                    "video" => Self::video(elem, context),
                    "wbr" => Self::wbr(elem, context),
                    _ => (),
                }
                Self::pop_style(context);
            }
            _ => (),
        }
    }

    // Document

    fn html(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        // Find the head element, if any
        let head = elem.children().find(|c| match c.value() {
            Node::Element(elem) => elem.name() == "head",
            _ => false,
        });
        if let Some(head) = head {
            Self::head(Element::new(head), context);
        }

        // Find the body element, if any
        let body = elem.children().find(|c| match c.value() {
            Node::Element(elem) => elem.name() == "body",
            _ => false,
        });
        if let Some(body) = body {
            Self::push_style(Element::new(body), context);
            Self::body(Element::new(body), context);
            Self::pop_style(context);
        }
    }

    // Document metadata

    fn head(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        for c in elem.children() {
            Self::metadata_content(c, context);
        }
    }

    fn title(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        // Ignored
    }

    fn base(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        // Ignored
    }

    fn link(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        let rel = match elem.attr("rel") {
            None => return,
            Some(rel) => rel,
        };
        let mut types = rel.split(char::is_whitespace);
        if !types.any(|t| t == "stylesheet") {
            return;
        }

        let href = match elem.attr("href") {
            None => return,
            Some(href) => match Url::options().base_url(Some(context.base_url)).parse(href) {
                Ok(href) => href,
                Err(err) => {
                    eprintln!("failed to parse stylesheet href: {}", err);
                    return;
                }
            },
        };
        if href.scheme() != "epub" {
            return;
        }
        let path = &href.path()[1..];

        let stylesheet_text = match context.doc.get_resource_str_by_path(path) {
            None => {
                eprintln!("stylesheet {} not found", href);
                return;
            }
            Some(text) => text,
        };

        let stylesheet = Arc::new(Stylesheet::from_str(
            &stylesheet_text,
            ServoUrl::from_url(context.base_url.to_owned()),
            Origin::Author,
            Arc::new(context.lock.wrap(MediaList::empty())),
            context.lock.clone(),
            None,
            None,
            QuirksMode::NoQuirks,
            0,
            AllowImportRules::No,
        ));
        let guard = context.lock.read();
        context
            .stylesheets
            .append_stylesheet(None, DocumentStyleSheet(stylesheet), &guard);
    }

    fn meta(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        // Ignored
    }

    fn style(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // Sections

    fn body(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        for c in elem.children() {
            Self::flow_content(c, context);
        }
    }

    fn article(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn section(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn nav(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn aside(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn h1(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn h2(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn h3(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn h4(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn h5(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn h6(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn hgroup(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn header(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn footer(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn address(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // Grouping content

    fn p(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        let style = &context.computed_style[context.computed_style.len() - 1];
        let align = match style.text_align {
            TextAlign::Left => Align::Left,
            TextAlign::Right => Align::Right,
            TextAlign::Center => Align::Center,
            TextAlign::Justify => Align::Justify,
        };

        let options = ParagraphOptions {
            align,
            margin_bottom_px: style.margin_bottom.0,
            margin_left_px: style.margin_left.0,
            margin_right_px: style.margin_right.0,
            margin_top_px: style.margin_top.0,
        };

        let indent_px = style.text_indent.0;

        context.builder.map(|b| match b {
            BuilderState::Doc(doc) => BuilderState::Paragraph(doc.paragraph(Some(options))),
            BuilderState::Paragraph(p) => {
                BuilderState::Paragraph(p.finish().paragraph(Some(options)))
            }
            _ => unreachable!(),
        });

        // TODO: remove this and take margins into account.
        if indent_px > 0.0 {
            context.builder.paragraph().indent_px(indent_px);
        }

        for c in elem.children() {
            Self::flow_content(c, context);
        }

        context.builder.map(|b| match b {
            BuilderState::Paragraph(p) => BuilderState::Doc(p.finish()),
            doc => doc,
        });
    }

    fn hr(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn pre(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn blockquote(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn ol(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn ul(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn menu(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn li(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn dl(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn dt(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn dd(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn figure(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn figcaption(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn main(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn search(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn div(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        for c in elem.children() {
            Self::flow_content(c, context);
        }
    }

    // Text-level semantics

    fn a(
        elem: Element<'a>,
        content: fn(NodeRef<'a, Node>, &mut LayoutContext<'a, R, S, F, H>),
        context: &mut LayoutContext<'a, R, S, F, H>,
    ) {
        for c in elem.children() {
            content(c, context)
        }
    }

    fn em(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn strong(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn small(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn s(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn cite(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn q(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn dfn(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn abbr(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn ruby(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn rt(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn rp(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn data(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn time(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn code(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn var(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn samp(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn kbd(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn sub(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn sup(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn i(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        for c in elem.children() {
            Self::phrasing_content(c, context);
        }
    }

    fn b(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn u(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn mark(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn bdi(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn bdo(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn span(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        for c in elem.children() {
            Self::phrasing_content(c, context);
        }
    }

    fn br(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {
        context.builder.if_paragraph(|p| {
            p.hard_line_break();
        });
    }

    fn wbr(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // Edits

    fn ins(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn del(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // Embedded content

    fn picture(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn source(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn img(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn iframe(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn embed(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn object(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn video(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn audio(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn track(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn map(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn area(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // TODO: mathml, svg

    // Tabular data

    fn table(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn caption(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn colgroup(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn col(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn tbody(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn thead(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn tfoot(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn tr(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn td(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn th(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // Forms

    fn form(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn label(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn input(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn button(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn select(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn datalist(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn optgroup(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn option(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn textarea(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn output(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn progress(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn meter(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn fieldset(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn legend(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // Interactive elements

    fn details(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn summary(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn dialog(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // Scripting

    fn script(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn noscript(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn template(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn slot(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    fn canvas(elem: Element<'a>, context: &mut LayoutContext<'a, R, S, F, H>) {}

    // Text

    fn text(text: &'a Text, context: &mut LayoutContext<'a, R, S, F, H>) {
        context.builder.if_paragraph(|p| {
            eprintln!("text: {:?}", text);
            if text.trim() != "" {
                p.text(text);
            }
        });
    }
}
