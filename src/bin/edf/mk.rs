use crate::{
    io::{Input, Output},
    MkArgs, MkFormat,
};
use edf::{
    layout::{self, font_db::Fonts},
    Style,
};
use embedded_graphics::{
    geometry::{Point, Size},
    primitives::rectangle::Rectangle,
};
use markdown::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::Path;

const LITERATA_REGULAR: &[u8] = include_bytes!("assets/Literata-Regular.ttf");

#[derive(Deserialize)]
struct DeviceConfig {
    ppi: u32,
    width_px: u32,
    height_px: u32,
    top_margin_px: u32,
    left_margin_px: u32,
    bottom_margin_px: u32,
    right_margin_px: u32,
}

impl DeviceConfig {
    fn point_size_to_px(&self, point_size: f32) -> u16 {
        // 1 point is 1/72 of an inch
        (self.ppi as f32 * point_size / 72.0) as u16
    }

    fn bounding_box(&self) -> Rectangle {
        let width = self.width_px - self.left_margin_px - self.right_margin_px;
        let height = self.height_px - self.top_margin_px - self.bottom_margin_px;
        Rectangle::new(Point::new(0, 0), Size::new(width, height))
    }
}

#[derive(Deserialize)]
struct FontConfig {
    fonts: HashMap<String, String>,
}

impl FontConfig {
    fn load_fonts(self, base_path: &Path) -> Result<HashMap<String, Vec<u8>>, Box<dyn Error>> {
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
struct StyleConfig {
    font_name: String,
    point_size: f32,
}

impl StyleConfig {
    fn device_style(&self, device: &DeviceConfig) -> Style {
        Style {
            font_name: self.font_name.clone(),
            em_px: device.point_size_to_px(self.point_size),
        }
    }
}

#[derive(Deserialize)]
struct MarkdownConfig {
    regular: StyleConfig,
    emphasis: Option<StyleConfig>,
    strong: Option<StyleConfig>,
    heading: Option<Vec<StyleConfig>>,
}

impl MarkdownConfig {
    fn into_device_options(self, device: &DeviceConfig) -> layout::markdown::Options {
        layout::markdown::Options::new(self.regular.device_style(device))
            .with_emphasis(self.emphasis.map(|s| s.device_style(device)))
            .with_strong(self.strong.map(|s| s.device_style(device)))
            .with_heading(
                self.heading
                    .map(|v| v.iter().map(|s| s.device_style(device)).collect()),
            )
    }
}

fn mk_markdown<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    fonts: &Fonts,
    device_config: &DeviceConfig,
    markdown_config: MarkdownConfig,
) -> Result<(), Box<dyn Error>> {
    let mut markdown_bytes = Vec::new();
    input.read_to_end(&mut markdown_bytes)?;

    let opts = Default::default();
    let (events, state) = parser::parse(std::str::from_utf8(&markdown_bytes)?, &opts)?;

    let (header, commands) = match layout::markdown::build(
        &events,
        state.bytes,
        device_config.bounding_box(),
        fonts,
        markdown_config.into_device_options(device_config),
    ) {
        Ok(ok) => ok,
        Err(err) => match err {
            layout::markdown::Error::Generic(msg) => return Err(msg.into()),
        },
    };

    edf::write::doc(output, &header, &commands)?;
    Ok(())
}

fn toml_from_file<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Box<dyn Error>> {
    Ok(toml::from_str(std::str::from_utf8(&fs::read(path)?)?)?)
}

pub fn mk(args: MkArgs) -> Result<(), Box<dyn Error>> {
    let device_config = toml_from_file(&args.device_config)?;

    let font_data = match args.font_config {
        Some(cfg) => {
            let font_dir = Path::new(&cfg).parent().unwrap_or(Path::new("/"));
            toml_from_file::<FontConfig>(&cfg)?.load_fonts(font_dir)?
        }
        None => HashMap::from([(String::from("regular"), Vec::from(LITERATA_REGULAR))]),
    };
    let mut fonts = Fonts::new(NonZeroUsize::new(256).unwrap());
    for (name, data) in font_data.iter() {
        fonts.add(name.as_str(), data)?;
    }

    let mut input = match args.input_path {
        None => Input::Stdin(io::stdin()),
        Some(path) => Input::File(File::open(path)?),
    };
    let mut output = match args.output_path {
        None => Output::Stdout(io::stdout()),
        Some(path) => Output::File(File::create(path)?),
    };

    let format = match args.format {
        Some(f) => f,
        None => MkFormat::Markdown,
    };
    match format {
        MkFormat::Markdown => {
            let config = match args.format_config {
                Some(path) => toml_from_file(&path)?,
                None => MarkdownConfig {
                    regular: StyleConfig {
                        font_name: String::from("regular"),
                        point_size: 12.0,
                    },
                    emphasis: None,
                    strong: None,
                    heading: None,
                },
            };
            mk_markdown(&mut input, &mut output, &fonts, &device_config, config)
        }
    }
}
