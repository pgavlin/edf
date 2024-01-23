use edf::Style;
use embedded_graphics::{
    geometry::{Point, Size},
    primitives::rectangle::Rectangle,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

pub const LITERATA_REGULAR: &[u8] = include_bytes!("assets/Literata-Regular.ttf");

#[derive(Deserialize)]
pub struct DeviceConfig {
    pub ppi: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub top_margin_px: u32,
    pub left_margin_px: u32,
    pub bottom_margin_px: u32,
    pub right_margin_px: u32,
}

impl DeviceConfig {
    pub fn point_size_to_px(&self, point_size: f32) -> u16 {
        // 1 point is 1/72 of an inch
        (self.ppi as f32 * point_size / 72.0) as u16
    }

    pub fn bounding_box(&self) -> Rectangle {
        let width = self.width_px - self.left_margin_px - self.right_margin_px;
        let height = self.height_px - self.top_margin_px - self.bottom_margin_px;
        Rectangle::new(Point::new(0, 0), Size::new(width, height))
    }
}

#[derive(Deserialize)]
pub struct FontConfig {
    pub fonts: HashMap<String, String>,
}

impl FontConfig {
    pub fn load_fonts(self, base_path: &Path) -> Result<HashMap<String, Vec<u8>>, Box<dyn Error>> {
        let mut data = HashMap::new();
        data.reserve(self.fonts.len());

        for (name, path) in self.fonts.into_iter() {
            let path = Path::new(&path);
            let font_data = if path.is_absolute() {
                fs::read(path)?
            } else {
                fs::read(base_path.join(path))?
            };

            data.insert(name, font_data);
        }

        Ok(data)
    }
}

#[derive(Deserialize)]
pub struct StyleConfig {
    pub font_name: String,
    pub point_size: f32,
}

impl StyleConfig {
    pub fn device_style(&self, device: &DeviceConfig) -> Style {
        Style {
            font_name: self.font_name.clone(),
            em_px: device.point_size_to_px(self.point_size),
        }
    }
}

pub fn toml_from_file<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Box<dyn Error>> {
    Ok(toml::from_str(std::str::from_utf8(&fs::read(path)?)?)?)
}
