use crate::{
    common::*,
    io::{Input, Output},
    MkArgs, MkFormat,
};
use edf::{font_db::Fonts, layout};
use hyphenation::{Hyphenator, Language, Load, Standard};
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, Cursor, Read, Write};
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

mod mk_markdown {
    use super::*;
    use markdown::*;

    #[derive(Deserialize)]
    pub struct Config {
        pub regular: StyleConfig,
        pub emphasis: Option<StyleConfig>,
        pub strong: Option<StyleConfig>,
        pub heading: Option<Vec<StyleConfig>>,
    }

    impl Config {
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

    pub fn mk<R: Read, W: Write>(
        input: &mut R,
        output: &mut W,
        fonts: &Fonts,
        hyphenator: &StandardHyphenator,
        device_config: &DeviceConfig,
        markdown_config: Config,
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
}

#[cfg(feature = "epub")]
mod mk_epub {
    use super::*;
    use epub::doc::EpubDoc;

    #[derive(Deserialize)]
    pub struct Config {
        pub regular: StyleConfig,
        pub emphasis: Option<StyleConfig>,
        pub strong: Option<StyleConfig>,
        pub heading: Option<Vec<StyleConfig>>,
    }

    impl Config {
        fn into_device_options(self, device: &DeviceConfig) -> layout::epub::Options {
            layout::epub::Options::new(device.ppi as f32, self.regular.device_style(device))
                .with_emphasis(self.emphasis.map(|s| s.device_style(device)))
                .with_strong(self.strong.map(|s| s.device_style(device)))
                .with_heading(
                    self.heading
                        .map(|v| v.iter().map(|s| s.device_style(device)).collect()),
                )
        }
    }

    pub fn mk<R: Read, W: Write>(
        input: &mut R,
        output: &mut W,
        fonts: &Fonts,
        hyphenator: &StandardHyphenator,
        device_config: &DeviceConfig,
        epub_config: Config,
    ) -> Result<(), Box<dyn Error>> {
        let mut epub_bytes = Vec::new();
        input.read_to_end(&mut epub_bytes)?;

        let mut doc = EpubDoc::from_reader(Cursor::new(epub_bytes))?;

        let (header, commands) = layout::epub::build(
            &mut doc,
            device_config.bounding_box(),
            fonts,
            hyphenator,
            epub_config.into_device_options(device_config),
        )?;

        edf::write::doc(output, &header, &commands)?;
        Ok(())
    }
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

    match args.format {
        Some(MkFormat::Markdown) | None => {
            let config = match args.format_config {
                Some(path) => toml_from_file(&path)?,
                None => mk_markdown::Config {
                    regular: StyleConfig {
                        font_name: String::from("regular"),
                        point_size: 12.0,
                    },
                    emphasis: None,
                    strong: None,
                    heading: None,
                },
            };
            mk_markdown::mk(
                &mut input,
                &mut output,
                &fonts,
                &hyphenator,
                &device_config,
                config,
            )
        }
        #[cfg(feature = "epub")]
        Some(MkFormat::Epub) => {
            let config = match args.format_config {
                Some(path) => toml_from_file(&path)?,
                None => mk_epub::Config {
                    regular: StyleConfig {
                        font_name: String::from("regular"),
                        point_size: 12.0,
                    },
                    emphasis: None,
                    strong: None,
                    heading: None,
                },
            };
            mk_epub::mk(
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
