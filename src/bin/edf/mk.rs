use crate::{
    common::*,
    io::{Input, Output},
    MkArgs, MkFormat,
};
use edf::{font_db::Fonts, layout};
use hyphenation::{Hyphenator, Language, Load, Standard};
use markdown::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::Path;

struct StandardHyphenator(Standard);

impl layout::Hyphenator for &StandardHyphenator {
    fn hyphenate(&self, word: &str, breaks: &mut Vec<usize>) {
        let hyphenated = self.0.hyphenate(word);
        breaks.clear();
        breaks.extend(hyphenated.breaks);
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
    hyphenator: &StandardHyphenator,
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
        hyphenator,
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

    let hyphenator = StandardHyphenator(Standard::from_embedded(Language::EnglishUS)?);

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
            mk_markdown(
                &mut input,
                &mut output,
                &fonts,
                &hyphenator,
                &device_config,
                config,
            )
        }
    }
}
