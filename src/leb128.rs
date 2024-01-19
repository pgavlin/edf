pub(crate) const CONTINUATION_BIT: u8 = 1 << 7;

#[inline]
pub(crate) fn low_bits_of_byte(byte: u8) -> u8 {
    byte & !CONTINUATION_BIT
}

#[inline]
pub(crate) fn low_bits_of_u64(val: u64) -> u8 {
    let byte = val & (core::u8::MAX as u64);
    low_bits_of_byte(byte as u8)
}

/// A module for reading LEB128-encoded signed and unsigned integers.
pub mod read {
    use super::{low_bits_of_byte, CONTINUATION_BIT};
    use core::fmt;
    use no_std_io::io;

    /// An error type for reading LEB128-encoded values.
    #[derive(Debug)]
    pub enum Error {
        /// There was an underlying IO error.
        IoError(io::Error),
        /// The number being read is larger than can be represented.
        Overflow,
    }

    impl From<io::Error> for Error {
        fn from(e: io::Error) -> Self {
            Error::IoError(e)
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
            match *self {
                Error::IoError(ref e) => e.fmt(f),
                Error::Overflow => {
                    write!(f, "The number being read is larger than can be represented")
                }
            }
        }
    }

    /// Decode an unsigned LEB128-encoded number from the `no_std_io::io::Read` stream
    /// `r`.
    ///
    /// On success, return the number.
    pub fn unsigned<R>(r: &mut R) -> Result<u64, Error>
    where
        R: ?Sized + io::Read,
    {
        let mut result = 0;
        let mut shift = 0;

        loop {
            let mut buf = [0];
            r.read_exact(&mut buf)?;

            if shift == 63 && buf[0] != 0x00 && buf[0] != 0x01 {
                while buf[0] & CONTINUATION_BIT != 0 {
                    r.read_exact(&mut buf)?;
                }
                return Err(Error::Overflow);
            }

            let low_bits = low_bits_of_byte(buf[0]) as u64;
            result |= low_bits << shift;

            if buf[0] & CONTINUATION_BIT == 0 {
                return Ok(result);
            }

            shift += 7;
        }
    }
}

/// A module for writing LEB128-encoded signed and unsigned integers.
pub mod write {
    use super::{low_bits_of_u64, CONTINUATION_BIT};
    use no_std_io::io;

    /// Write `val` to the `no_std_io::io::Write` stream `w` as an unsigned LEB128 value.
    ///
    /// On success, return the number of bytes written to `w`.
    pub fn unsigned<W>(w: &mut W, mut val: u64) -> Result<usize, io::Error>
    where
        W: ?Sized + io::Write,
    {
        let mut bytes_written = 0;
        loop {
            let mut byte = low_bits_of_u64(val);
            val >>= 7;
            if val != 0 {
                // More bytes to come, so set the continuation bit.
                byte |= CONTINUATION_BIT;
            }

            let buf = [byte];
            w.write_all(&buf)?;
            bytes_written += 1;

            if val == 0 {
                return Ok(bytes_written);
            }
        }
    }
}
