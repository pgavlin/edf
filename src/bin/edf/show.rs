use crate::{common::*, io::Input, ShowArgs};
use edf::{display, font_db};
use embedded_graphics::{
    geometry::{Point, Size},
    pixelcolor::Gray8,
};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, Window};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, Cursor, Read};
use std::num::NonZeroUsize;
use std::path::Path;

pub fn show(args: ShowArgs) -> Result<(), Box<dyn Error>> {
    let device_config: DeviceConfig = toml_from_file(&args.device_config)?;

    let mut input = match args.input_path {
        None => Input::Stdin(io::stdin()),
        Some(path) => Input::File(File::open(path)?),
    };

    let mut bytes = Vec::new();
    input.read_to_end(&mut bytes)?;

    let mut cursor = Cursor::new(&bytes);

    let header = edf::read::header(&mut cursor)?;
    edf::read::seek_trailer(&mut cursor)?;
    let trailer = edf::read::trailer(&mut cursor)?;

    let mut sim =
        SimulatorDisplay::<Gray8>::new(Size::new(device_config.width_px, device_config.height_px));

    let offset = trailer.pages[args.page_num as usize - 1];
    let page = edf::read::page(&header, &bytes[offset as usize..])?;

    let font_data = match args.font_config {
        Some(cfg) => {
            let font_dir = Path::new(&cfg).parent().unwrap_or(Path::new("/"));
            toml_from_file::<FontConfig>(&cfg)?.load_fonts(font_dir)?
        }
        None => HashMap::from([(String::from("regular"), Vec::from(LITERATA_REGULAR))]),
    };
    let mut fonts = font_db::Fonts::new(NonZeroUsize::new(256).unwrap());
    for (name, data) in font_data.iter() {
        fonts.add(name.as_str(), data)?;
    }

    let default_style = match fonts.get_style(&header.styles[0]) {
        None => return Err("missing font for default style".into()),
        Some(s) => s,
    };

    display::page(
        &mut sim,
        Point::new(
            device_config.left_margin_px as i32,
            device_config.top_margin_px as i32,
        ),
        &fonts,
        default_style,
        &header,
        &page,
    );

    let output_settings = OutputSettingsBuilder::new().build();
    Window::new(&format!("Page {}", args.page_num), &output_settings).show_static(&sim);

    Ok(())
}
