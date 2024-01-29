use crate::{io::Input, DumpArgs};
use std::error::Error;
use std::fs::File;
use std::io::{self, Cursor, Read};

pub fn dump(args: DumpArgs) -> Result<(), Box<dyn Error>> {
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

    println!("# Header");
    println!("Title: ${:?}", header.title);
    println!("Styles:");
    for style in &header.styles {
        println!("- `{style:?}`");
    }
    println!();

    println!("# Pages");
    for (num, offset) in trailer.pages.iter().enumerate() {
        println!();
        println!("## Page {} @{}", num + 1, offset);

        let page = edf::read::page(&header, &bytes[*offset as usize..])?;

        for command in page {
            println!("- `{command:?}`");
        }
    }

    Ok(())
}
