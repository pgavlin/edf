use crate::{fonts, Style};

#[cfg(feature = "layout")]
use crate::layout;

#[cfg(feature = "layout")]
use embedded_graphics::{geometry::Size, primitives::rectangle::Rectangle};

#[cfg(feature = "display")]
use crate::display;

#[cfg(feature = "display")]
use embedded_graphics::draw_target::DrawTarget;

#[cfg(any(feature = "layout", feature = "display"))]
use embedded_graphics::geometry::Point;

extern crate alloc;
use alloc::vec::Vec;
use core::cell::{RefCell, RefMut};
use core::num::NonZeroUsize;
use hashbrown::HashMap;
use lru::LruCache;
use ttf_parser::{Face, FaceParsingError, OutlineBuilder};
use zeno::{Command, Mask, Origin, Placement, Transform, Vector};

pub struct Glyph {
    pub placement: Placement,
    pub data: Vec<u8>,
}

#[derive(PartialEq, Eq, Hash)]
struct GlyphCacheKey {
    font_id: usize,
    size_px: u16,
    code_point: char,
}

struct Font<'data> {
    id: usize,
    name: &'data str,
    face: Face<'data>,
}

pub struct Fonts<'data> {
    fonts: HashMap<&'data str, Font<'data>>,
    glyph_cache: RefCell<LruCache<GlyphCacheKey, Glyph>>,
}

impl<'data> Fonts<'data> {
    pub fn new(glyph_cache_size: NonZeroUsize) -> Self {
        Fonts {
            fonts: HashMap::new(),
            glyph_cache: RefCell::new(LruCache::new(glyph_cache_size)),
        }
    }

    pub fn add(&mut self, name: &'data str, data: &'data [u8]) -> Result<usize, FaceParsingError> {
        let id = self.fonts.len();
        self.fonts.insert(
            name,
            Font {
                id,
                name,
                face: Face::parse(data, 0)?,
            },
        );
        Ok(id)
    }

    fn render_glyph(font: &Font, pixels_per_em: f32, code_point: char) -> Glyph {
        let glyph_id = match font.face.glyph_index(code_point) {
            None => {
                return Glyph {
                    placement: Default::default(),
                    data: Vec::new(),
                }
            }
            Some(id) => id,
        };

        let mut path = Path::new();
        if font.face.outline_glyph(glyph_id, &mut path).is_none() {
            return Glyph {
                placement: Default::default(),
                data: Vec::new(),
            };
        }

        let units_per_em: f32 = font.face.units_per_em().into();
        let pixels_per_unit = pixels_per_em / units_per_em;

        let (data, placement) = Mask::new(&path.commands)
            .origin(Origin::TopLeft)
            .transform(Some(Transform::scale(pixels_per_unit, pixels_per_unit)))
            .render();

        Glyph { placement, data }
    }

    fn glyph(&self, style: &FontStyle, code_point: char) -> RefMut<Glyph> {
        let cache_key = GlyphCacheKey {
            font_id: style.font.id,
            size_px: style.size_px,
            code_point,
        };
        RefMut::map(self.glyph_cache.borrow_mut(), |cache| {
            cache.get_or_insert_mut(cache_key, || {
                Fonts::render_glyph(style.font, style.size_px.into(), code_point)
            })
        })
    }

    pub fn get_style<'s>(&'s self, style: &Style) -> Option<FontStyle<'s, 'data>>
    where
        'data: 's,
    {
        let font = match self.fonts.get(style.font_name.as_str()) {
            None => return None,
            Some(f) => f,
        };

        let face = &font.face;
        let pixels_per_em: f32 = style.em_px.into();
        let units_per_em: f32 = face.units_per_em().into();
        let pixels_per_unit = pixels_per_em / units_per_em;
        let line_units: i16 = face.ascender() - face.descender() + face.line_gap();
        let line_height_px: u16 =
            unsafe { (line_units as f32 * pixels_per_unit).to_int_unchecked() };
        let baseline_px: u16 =
            unsafe { ((line_units - face.ascender()) as f32 * pixels_per_unit).to_int_unchecked() };

        Some(FontStyle {
            fonts: self,
            font,
            size_px: style.em_px,
            line_height_px,
            baseline_px,
        })
    }
}

#[cfg(feature = "layout")]
impl<'a, 'b> layout::Fonts for &'a Fonts<'b>
where
    'b: 'a,
{
    type Style = FontStyle<'a, 'b>;

    fn get_style(&self, style: &Style) -> Option<Self::Style> {
        Fonts::get_style(self, style)
    }
}

#[cfg(feature = "display")]
impl<'a, 'b> display::Fonts for &'a Fonts<'b>
where
    'b: 'a,
{
    type Style = FontStyle<'a, 'b>;

    fn get_style(&self, style: &Style) -> Option<Self::Style> {
        Fonts::get_style(self, style)
    }
}

#[derive(Clone)]
pub struct FontStyle<'a, 'b>
where
    'b: 'a,
{
    fonts: &'a Fonts<'b>,
    font: &'a Font<'b>,
    size_px: u16,
    line_height_px: u16,
    baseline_px: u16,
}

impl<'a, 'b> FontStyle<'a, 'b>
where
    'b: 'a,
{
    pub fn glyph(&self, code_point: char) -> RefMut<Glyph> {
        self.fonts.glyph(self, code_point)
    }
}

impl<'a, 'b> fonts::FontStyle for FontStyle<'a, 'b>
where
    'b: 'a,
{
    fn font_name(&self) -> &str {
        self.font.name
    }

    fn em_px(&self) -> u16 {
        self.size_px
    }

    fn line_height(&self) -> u16 {
        self.line_height_px
    }

    fn baseline(&self) -> u16 {
        self.baseline_px
    }
}

#[cfg(feature = "layout")]
impl<'a, 'b> layout::FontStyle for FontStyle<'a, 'b>
where
    'b: 'a,
{
    fn measure_string(&self, text: &str) -> layout::TextMetrics {
        let origin = Point::new(0, 0);
        let mut cursor = origin;

        for c in text.chars() {
            let glyph = self.fonts.glyph(self, c);
            let glyph_origin = cursor + Point::new(glyph.placement.left, glyph.placement.top);
            cursor.x = glyph_origin.x + glyph.placement.width as i32;
        }

        let bounding_box = Rectangle::new(
            origin,
            Size::new((cursor.x - origin.x) as u32, self.line_height_px as u32),
        );
        layout::TextMetrics { bounding_box }
    }
}

#[cfg(feature = "display")]
impl<'a, 'b> display::FontStyle for FontStyle<'a, 'b>
where
    'b: 'a,
{
    fn glyph_advance(&self, c: char) -> i32 {
        let glyph = self.fonts.glyph(self, c);
        glyph.placement.left + glyph.placement.width as i32
    }

    fn draw_glyph<C: display::Color, Draw: DrawTarget<Color = C>>(
        &self,
        draw: &mut Draw,
        origin: Point,
        color: C,
        c: char,
    ) -> Result<Point, Draw::Error> {
        let glyph = self.fonts.glyph(self, c);
        display::draw_glyph(draw, origin, color, glyph.placement, &glyph.data)
    }
}

struct Path {
    commands: Vec<Command>,
}

impl Path {
    fn new() -> Self {
        Path {
            commands: Vec::new(),
        }
    }
}

impl OutlineBuilder for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(Command::MoveTo(Vector::new(x, y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(Command::LineTo(Vector::new(x, y)));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands
            .push(Command::QuadTo(Vector::new(x1, y1), Vector::new(x, y)));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(Command::CurveTo(
            Vector::new(x1, y1),
            Vector::new(x2, y2),
            Vector::new(x, y),
        ));
    }

    fn close(&mut self) {
        self.commands.push(Command::Close);
    }
}
