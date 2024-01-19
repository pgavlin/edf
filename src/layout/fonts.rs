use crate::Style;
use embedded_graphics::primitives::rectangle::Rectangle;

pub struct TextMetrics {
    pub bounding_box: Rectangle,
}

pub trait Fonts {
    type Style: FontStyle;

    fn get_style(&self, style: &Style) -> Option<Self::Style>;
}

pub trait FontStyle: Clone {
    fn font_name(&self) -> &str;
    fn em_px(&self) -> u16;

    fn line_height(&self) -> u16;
    fn measure_string(&self, text: &str) -> TextMetrics;
}
