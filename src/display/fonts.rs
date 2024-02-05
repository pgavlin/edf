use crate::Style;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{self, GrayColor, PixelColor, RgbColor},
    primitives::rectangle::Rectangle,
    text::{
        renderer::{TextMetrics, TextRenderer},
        Baseline,
    },
    Pixel,
};
use zeno::Placement;

pub trait Color: PixelColor {
    fn blend(&self, alpha: u8, over: Self) -> Self;
}

macro_rules! impl_rgb_color {
    ($type_name:ident) => {
        impl Color for pixelcolor::$type_name {
            fn blend(&self, alpha: u8, over: Self) -> Self {
                let r = self.r() as u16 * alpha as u16 / 255u16;
                let g = self.g() as u16 * alpha as u16 / 255u16;
                let b = self.b() as u16 * alpha as u16 / 255u16;

                let or = over.r() as u16 * (255 - alpha) as u16 / 255u16;
                let og = over.g() as u16 * (255 - alpha) as u16 / 255u16;
                let ob = over.b() as u16 * (255 - alpha) as u16 / 255u16;

                pixelcolor::$type_name::new((r + or) as u8, (g + og) as u8, (b + ob) as u8)
            }
        }
    };
}

macro_rules! impl_gray_color {
    ($type_name:ident) => {
        impl Color for pixelcolor::$type_name {
            fn blend(&self, alpha: u8, over: Self) -> Self {
                let l = self.luma() as u16 * alpha as u16 / 255u16;
                let ol = over.luma() as u16 * (255 - alpha) as u16 / 255u16;
                pixelcolor::$type_name::new((l + ol) as u8)
            }
        }
    };
}

impl_rgb_color!(Bgr555);
impl_rgb_color!(Bgr565);
impl_rgb_color!(Bgr666);
impl_rgb_color!(Bgr888);
impl_rgb_color!(Rgb555);
impl_rgb_color!(Rgb565);
impl_rgb_color!(Rgb666);
impl_rgb_color!(Rgb888);
impl_gray_color!(Gray2);
impl_gray_color!(Gray4);
impl_gray_color!(Gray8);

pub trait Fonts {
    type Style: FontStyle;

    fn get_style(&self, style: &Style) -> Option<Self::Style>;
}

pub trait FontStyle: crate::fonts::FontStyle {
    fn glyph_advance(&self, code_point: char) -> i32;
    fn draw_glyph<C: Color, Draw: DrawTarget<Color = C>>(
        &self,
        draw: &mut Draw,
        origin: Point,
        color: C,
        over: C,
        code_point: char,
    ) -> Result<Point, Draw::Error>;
}

pub fn draw_glyph<C: Color, Draw: DrawTarget<Color = C>>(
    draw: &mut Draw,
    origin: Point,
    color: C,
    over: C,
    placement: Placement,
    mut data: &[u8],
) -> Result<Point, Draw::Error> {
    let glyph_origin = origin + Point::new(placement.left, -placement.top);

    for y in 0..placement.height {
        let row = &data[..(placement.width as usize)];

        let pixels = row.iter().enumerate().map(|(x, alpha)| {
            Pixel(
                Point::new(glyph_origin.x + x as i32, glyph_origin.y - y as i32),
                color.blend(*alpha, over),
            )
        });

        draw.draw_iter(pixels)?;

        data = &data[(placement.width as usize)..];
    }

    Ok(Point::new(
        glyph_origin.x + placement.width as i32,
        origin.y,
    ))
}

pub struct CharacterStyle<S, C> {
    pub style: S,
    pub whitespace_px: i32,
    pub color: C,
    pub over: C,
}

impl<S: FontStyle, C> CharacterStyle<S, C> {
    pub fn new(style: S, color: C, over: C) -> Self {
        let whitespace_px = style.em_px() / 3;
        CharacterStyle {
            style,
            whitespace_px: whitespace_px.into(),
            color,
            over,
        }
    }
}

impl<S: FontStyle, C: Color> TextRenderer for &CharacterStyle<S, C> {
    type Color = C;

    fn draw_string<D: DrawTarget<Color = C>>(
        &self,
        text: &str,
        position: Point,
        _baseline: Baseline,
        target: &mut D,
    ) -> Result<Point, D::Error> {
        let mut origin = position;

        // TODO: baseline

        for c in text.chars() {
            if c.is_whitespace() {
                origin.x += self.whitespace_px;
            } else {
                origin = self
                    .style
                    .draw_glyph(target, origin, self.color, self.over, c)?;
            }
        }

        Ok(origin)
    }

    fn draw_whitespace<D: DrawTarget<Color = C>>(
        &self,
        width: u32,
        position: Point,
        _baseline: Baseline,
        _target: &mut D,
    ) -> Result<Point, D::Error> {
        Ok(position + Point::new(width as i32, 0))
    }

    fn measure_string(&self, text: &str, position: Point, _baseline: Baseline) -> TextMetrics {
        let mut origin = position;

        // TODO: baseline

        for c in text.chars() {
            if c.is_whitespace() {
                origin.x += self.whitespace_px;
            } else {
                origin.x += self.style.glyph_advance(c);
            }
        }

        let bounding_box = Rectangle::new(
            position,
            Size::new(
                (origin.x - position.x) as u32,
                self.style.line_height() as u32,
            ),
        );
        TextMetrics {
            bounding_box,
            next_position: origin,
        }
    }

    fn line_height(&self) -> u32 {
        self.style.line_height() as u32
    }
}
