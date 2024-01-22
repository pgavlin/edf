#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::{string::String, vec::Vec};
use core::convert::AsRef;

#[cfg(feature = "layout")]
pub mod layout;

mod leb128;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Style {
    pub font_name: String,
    pub em_px: u16,
}

pub struct Header {
    pub styles: Vec<Style>,
}

pub struct Trailer {
    pub pages: Vec<u32>,
}

#[derive(Debug, Clone)]
pub enum Command<S: Clone> {
    /// No-op.
    Nop,
    /// Move the cursor `tab_stop` points in the current text direction.
    HTab,
    /// Advance the cursor `line_height` points perpendicular to the current text direction.
    LineBreak,
    /// Move the cursor `tab_stop` points in perpendicular to the current text direction.
    VTab,
    /// End the current page.
    PageBreak,
    /// Draws a UTF-8-encoded string at the current cursor, then advances the cursor by the width
    /// of the string.
    Show { str: S },
    /// Advances the cursor by dx points.
    Advance { dx: u16 },
    /// Moves the cursor to the given position.
    SetCursor { x: u16, y: u16 },
    /// Sets the current style to that indicated by the given index.
    SetStyle { s: u16 },
    /// Sets the current whitespace adjustment ratio to the given amount
    SetAdjustmentRatio { r: f32 },
    /// Sets the current line metrics.
    SetLineMetrics { height: u16, baseline: u16 },
    /// Ends the command stream.
    End,
}

pub mod read {
    extern crate alloc;
    use alloc::{
        string::{self, String},
        vec::Vec,
    };

    use super::*;
    use crate::leb128;
    use core::convert::From;
    use core::fmt;
    use core::num::TryFromIntError;
    use no_std_io::io;

    #[derive(Debug)]
    pub enum Error {
        IoError(io::Error),
        InvalidMagicNumber,
        InvalidEncoding,
        InvalidCommand,
        InvalidStyleIndex,
    }

    impl From<io::Error> for Error {
        fn from(err: io::Error) -> Error {
            Error::IoError(err)
        }
    }

    impl From<core::str::Utf8Error> for Error {
        fn from(_: core::str::Utf8Error) -> Error {
            Error::InvalidEncoding
        }
    }

    impl From<string::FromUtf8Error> for Error {
        fn from(_: string::FromUtf8Error) -> Error {
            Error::InvalidEncoding
        }
    }

    impl From<TryFromIntError> for Error {
        fn from(_: TryFromIntError) -> Error {
            Error::InvalidEncoding
        }
    }

    impl From<leb128::read::Error> for Error {
        fn from(err: leb128::read::Error) -> Error {
            match err {
                leb128::read::Error::IoError(err) => Error::IoError(err),
                _ => Error::InvalidEncoding,
            }
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Error::IoError(err) => write!(f, "I/O error: {}", err),
                Error::InvalidMagicNumber => write!(f, "invalid magic number"),
                Error::InvalidEncoding => write!(f, "invalid encoding"),
                Error::InvalidCommand => write!(f, "invalid command"),
                Error::InvalidStyleIndex => write!(f, "invalid style index"),
            }
        }
    }

    #[cfg(feature = "std")]
    impl std::error::Error for Error {}

    fn read_string<R: io::Read>(r: &mut R) -> Result<String, Error> {
        let len: u32 = leb128::read::unsigned(r)?.try_into()?;
        let mut bytes = Vec::new();
        bytes.reserve_exact(len as usize);
        bytes.resize(len as usize, 0);
        r.read_exact(bytes.as_mut_slice())?;
        Ok(String::from_utf8(bytes)?)
    }

    fn read_style<R: io::Read>(r: &mut R) -> Result<Style, Error> {
        let font_name = read_string(r)?;
        let em_px: u16 = leb128::read::unsigned(r)?.try_into()?;
        Ok(Style { font_name, em_px })
    }

    pub fn header<R: io::Read>(r: &mut R) -> Result<Header, Error> {
        // check magic number
        let mut buf = [0; 4];
        r.read_exact(&mut buf)?;
        if buf != [0x0e, 0xdf, 0x01, 0x00] {
            return Err(Error::InvalidMagicNumber);
        }

        // read style vector
        let len: u32 = leb128::read::unsigned(r)?.try_into()?;
        let mut styles = Vec::new();
        styles.reserve_exact(len as usize);
        for _ in 0..len {
            styles.push(read_style(r)?);
        }

        Ok(Header { styles })
    }

    pub fn seek_trailer<R: io::Read + io::Seek>(r: &mut R) -> Result<u64, Error> {
        r.seek(io::SeekFrom::End(-4))?;
        let mut buf = [0; 4];
        r.read_exact(&mut buf)?;
        let offset = i32::from_le_bytes(buf) - 4;
        if offset >= 0 {
            Err(Error::InvalidEncoding)
        } else {
            Ok(r.seek(io::SeekFrom::End(offset as i64))?)
        }
    }

    pub fn trailer<R: io::Read>(r: &mut R) -> Result<Trailer, Error> {
        // read page vector
        let len: u32 = leb128::read::unsigned(r)?.try_into()?;
        let mut pages = Vec::new();
        pages.reserve_exact(len as usize);
        for _ in 0..len {
            let offset: u32 = leb128::read::unsigned(r)?.try_into()?;
            pages.push(offset);
        }

        Ok(Trailer { pages })
    }

    fn decode_command<'a>(
        header: &Header,
        source: &'a [u8],
    ) -> Result<(Command<&'a str>, usize), Error> {
        let code = source[0];
        let source = &source[1..];

        let (command, len) = match code {
            0x09 => (Command::HTab, 0),
            0x0a => (Command::LineBreak, 0),
            0x0b => (Command::VTab, 0),
            0x0c => (Command::PageBreak, 0),
            0x80 => (Command::Nop, 0),
            0x81 => {
                let mut r = io::Cursor::new(source);
                let dx: u16 = leb128::read::unsigned(&mut r)?.try_into()?;
                (Command::Advance { dx }, r.position() as usize)
            }
            0x82 => {
                let mut r = io::Cursor::new(source);
                let x: u16 = leb128::read::unsigned(&mut r)?.try_into()?;
                let y: u16 = leb128::read::unsigned(&mut r)?.try_into()?;
                (Command::SetCursor { x, y }, r.position() as usize)
            }
            0x83 => {
                let mut r = io::Cursor::new(source);
                let s: u16 = leb128::read::unsigned(&mut r)?.try_into()?;
                if (s as usize) >= header.styles.len() {
                    return Err(Error::InvalidStyleIndex);
                }
                (Command::SetStyle { s }, r.position() as usize)
            }
            0x84 => {
                if source.len() < 4 {
                    return Err(Error::InvalidEncoding);
                }
                let bytes = [source[0], source[1], source[2], source[3]];
                (
                    Command::SetAdjustmentRatio {
                        r: f32::from_le_bytes(bytes),
                    },
                    4,
                )
            }
            0x85 => {
                let mut r = io::Cursor::new(source);
                let height: u16 = leb128::read::unsigned(&mut r)?.try_into()?;
                let baseline: u16 = leb128::read::unsigned(&mut r)?.try_into()?;
                (
                    Command::SetLineMetrics { height, baseline },
                    r.position() as usize,
                )
            }
            0xbf => (Command::End, 0),
            _ => return Err(Error::InvalidCommand),
        };

        Ok((command, len + 1))
    }

    pub fn page<'a>(header: &Header, mut source: &'a [u8]) -> Result<Vec<Command<&'a str>>, Error> {
        let mut commands = Vec::new();

        while !source.is_empty() {
            let mut i = 0;
            while i < source.len() {
                if source[i] < 0x20 || source[i] > 0x7f && source[i] < 0xc0 {
                    break;
                }
                i += UTF8_CHAR_WIDTH[source[i] as usize] as usize;
            }
            if i != 0 {
                commands.push(Command::Show {
                    str: core::str::from_utf8(&source[..i])?,
                });
            }
            if i != source.len() {
                let (command, advance) = decode_command(header, &source[i..])?;
                commands.push(command);
                if matches!(
                    commands[commands.len() - 1],
                    Command::PageBreak | Command::End
                ) {
                    break;
                }
                i += advance;
            }
            source = &source[i..];
        }

        Ok(commands)
    }
}

pub mod write {
    use super::*;
    use crate::leb128;
    use no_std_io::io;

    fn write_all<W: io::Write>(w: &mut W, bytes: &[u8]) -> Result<usize, io::Error> {
        w.write_all(bytes)?;
        Ok(bytes.len())
    }

    fn encode_string<W: io::Write>(w: &mut W, s: &str) -> Result<usize, io::Error> {
        let n = leb128::write::unsigned(w, s.len() as u64)?;
        write_all(w, s.as_bytes())?;
        Ok(n + s.len())
    }

    fn encode_style<W: io::Write>(w: &mut W, s: &Style) -> Result<usize, io::Error> {
        let mut n = encode_string(w, &s.font_name)?;
        n += leb128::write::unsigned(w, s.em_px as u64)?;
        Ok(n)
    }

    fn encode_header<W: io::Write>(w: &mut W, h: &Header) -> Result<usize, io::Error> {
        // write magic
        let magic = [0x0e, 0xdf, 0x01, 0x00];
        let mut n = write_all(w, &magic[..])?;

        // write style vector
        n += leb128::write::unsigned(w, h.styles.len() as u64)?;
        for s in &h.styles {
            n += encode_style(w, s)?;
        }

        Ok(n)
    }

    fn encode_pages<W: io::Write, S: AsRef<str> + Clone>(
        w: &mut W,
        at: usize,
        pages: &[Command<S>],
    ) -> Result<(Vec<u32>, usize), io::Error> {
        let mut page_offsets = Vec::new();
        page_offsets.push(at as u32);

        let mut n = 0;
        for c in pages {
            match c {
                Command::Nop => n += write_all(w, &[0x80])?,
                Command::HTab => n += write_all(w, &[0x09])?,
                Command::LineBreak => n += write_all(w, &[0x0a])?,
                Command::VTab => n += write_all(w, &[0x0b])?,
                Command::PageBreak => {
                    n += write_all(w, &[0x0c])?;

                    // TODO: make this checked
                    page_offsets.push((at + n) as u32);
                }
                Command::Show { str } => n += write_all(w, str.as_ref().as_bytes())?,
                Command::Advance { dx } => {
                    n += write_all(w, &[0x81])? + leb128::write::unsigned(w, *dx as u64)?;
                }
                Command::SetCursor { x, y } => {
                    n += write_all(w, &[0x82])?;
                    n += leb128::write::unsigned(w, *x as u64)?;
                    n += leb128::write::unsigned(w, *y as u64)?;
                }
                Command::SetStyle { s } => {
                    n += write_all(w, &[0x83])? + leb128::write::unsigned(w, *s as u64)?;
                }
                Command::SetAdjustmentRatio { r } => {
                    n += write_all(w, &[0x84])? + write_all(w, &r.to_le_bytes())?;
                }
                Command::SetLineMetrics { height, baseline } => {
                    n += write_all(w, &[0x85])?
                        + leb128::write::unsigned(w, *height as u64)?
                        + leb128::write::unsigned(w, *baseline as u64)?;
                }
                _ => {}
            };
        }
        n += write_all(w, &[0xbf])?;
        Ok((page_offsets, n))
    }

    fn encode_trailer<W: io::Write>(w: &mut W, pages: Vec<u32>) -> Result<usize, io::Error> {
        // Encode page vector
        let mut n = leb128::write::unsigned(w, pages.len() as u64)?;
        for p in pages {
            n += leb128::write::unsigned(w, p as u64)?;
        }

        // TODO: make this checked
        let offset = (-(n as i32)).to_le_bytes();
        n += write_all(w, &offset[..])?;

        Ok(n)
    }

    pub fn doc<W: io::Write, S: AsRef<str> + Clone>(
        w: &mut W,
        h: &Header,
        pages: &[Command<S>],
    ) -> Result<usize, io::Error> {
        let header_len = encode_header(w, h)?;
        let (page_offsets, commands_len) = encode_pages(w, header_len, pages)?;
        let trailer_len = encode_trailer(w, page_offsets)?;
        Ok(header_len + commands_len + trailer_len)
    }
}

// https://tools.ietf.org/html/rfc3629
const UTF8_CHAR_WIDTH: &[u8; 256] = &[
    // 1  2  3  4  5  6  7  8  9  A  B  C  D  E  F
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 0
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 1
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 2
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 3
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 4
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 5
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 6
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 7
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 8
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 9
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // A
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // B
    0, 0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // C
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // D
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, // E
    4, 4, 4, 4, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // F
];
