use crate::{
    display::{CharacterStyle, FontStyle, Fonts},
    Command, Header,
};
use core::convert::AsRef;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Point,
    pixelcolor::{Gray8, GrayColor},
    primitives::{triangle::Triangle, Primitive, PrimitiveStyle},
    text::Text,
    Drawable,
};

pub fn page<Draw, S, F, T>(
    draw: &mut Draw,
    origin: Point,
    debug: bool,
    fonts: F,
    default_style: S,
    header: &Header,
    page: &[Command<T>],
) where
    Draw: DrawTarget<Color = Gray8>,
    S: FontStyle,
    F: Fonts<Style = S>,
    T: AsRef<str> + core::fmt::Debug + Clone,
{
    let mut style = default_style.clone();

    let mut glue_width = style.em_px() as f32 / 3.0;
    let mut glue_stretch = glue_width / 2.0;
    let mut glue_shrink = glue_width / 3.0;

    let mut line_height = style.line_height() as i32;
    let mut line_baseline = style.baseline() as i32;
    let mut baseline_offset = 0;
    let mut cursor = origin;
    let mut whitespace_width = 0.0;
    let mut whitespace_width_quantized = 0;
    let mut error = 0f32;

    for command in page {
        if debug {
            let _ = Triangle::new(
                cursor,
                cursor + Point::new(-3, -7),
                cursor + Point::new(3, -7),
            )
            .into_styled(PrimitiveStyle::with_stroke(Gray8::BLACK, 1))
            .draw(draw);
        }

        match command {
            Command::LineBreak => {
                error = 0.0;
                cursor = Point::new(origin.x, cursor.y + line_height);
            }
            Command::PageBreak => {
                return;
            }
            Command::Advance { dx } => cursor += Point::new(*dx as i32, 0),
            Command::SetCursor { x, y } => cursor = Point::new(*x as i32, *y as i32),
            Command::SetAdjustmentRatio { r } => {
                let r = *r;
                whitespace_width = if r < 0.0 {
                    glue_width + glue_shrink * r
                } else if r > 0.0 {
                    glue_width + glue_stretch * r
                } else {
                    glue_width
                };
                whitespace_width_quantized = unsafe { whitespace_width.to_int_unchecked::<i32>() };
            }
            Command::SetLineMetrics { height, baseline } => {
                line_height = *height as i32;
                line_baseline = *baseline as i32;

                baseline_offset = if (style.baseline() as i32) < line_baseline {
                    line_baseline - style.baseline() as i32
                } else {
                    0
                };
            }
            Command::Show { str } => {
                let mut text_cursor =
                    cursor + Point::new(0, line_height - line_baseline - baseline_offset);
                let character_style = CharacterStyle {
                    style: style.clone(),
                    whitespace_px: whitespace_width_quantized,
                    color: Gray8::BLACK,
                    over: Gray8::WHITE,
                };
                for c in str.as_ref().chars() {
                    let (next_cursor, expected_width, can_charge) = if c.is_whitespace() {
                        (
                            text_cursor + Point::new(whitespace_width_quantized, 0),
                            whitespace_width,
                            true,
                        )
                    } else {
                        let mut buf = [0; 4];
                        let next_cursor =
                            match Text::new(c.encode_utf8(&mut buf), text_cursor, &character_style)
                                .draw(draw)
                            {
                                Ok(point) => point,
                                Err(_) => text_cursor,
                            };
                        (next_cursor, (next_cursor - text_cursor).x as f32, false)
                    };

                    error += expected_width - (next_cursor.x - text_cursor.x) as f32;
                    text_cursor = if can_charge && error >= 1.0 {
                        let error_px = unsafe { error.to_int_unchecked() };
                        error -= error_px as f32;
                        next_cursor + Point::new(error_px, 0)
                    } else {
                        next_cursor
                    };
                }

                cursor = Point::new(text_cursor.x, cursor.y);
            }
            Command::SetStyle { s } => {
                style = match fonts.get_style(&header.styles[*s as usize]) {
                    Some(s) => s,
                    None => default_style.clone(),
                };

                glue_width = style.em_px() as f32 / 3.0;
                glue_stretch = glue_width / 2.0;
                glue_shrink = glue_width / 3.0;

                baseline_offset = if (style.baseline() as i32) < line_baseline {
                    line_baseline - style.baseline() as i32
                } else {
                    0
                };
            }
            _ => {}
        };
    }
}
