use crate::Style;
use embedded_graphics::{draw_target::DrawTarget, geometry::Point, pixelcolor::Gray8};

pub trait Fonts {
    type Style: FontStyle;

    fn get_style(&self, style: &Style) -> Option<Self::Style>;
}

pub trait FontStyle: crate::fonts::FontStyle {
    fn glyph_advance(&self, code_point: char) -> i32;
    fn draw_glyph<Draw: DrawTarget<Color = Gray8>>(
        &self,
        draw: &mut Draw,
        position: Point,
        code_point: char,
    ) -> Result<Point, Draw::Error>;
}
