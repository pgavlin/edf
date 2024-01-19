use std::fs::File;
use std::io::*;

pub enum Input {
    Stdin(Stdin),
    File(File),
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self {
            Input::Stdin(stdin) => stdin.read(buf),
            Input::File(file) => file.read(buf),
        }
    }
}

pub enum Output {
    Stdout(Stdout),
    File(File),
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        match self {
            Output::Stdout(stdout) => stdout.write(buf),
            Output::File(file) => file.write(buf),
        }
    }

    fn flush(&mut self) -> Result<()> {
        match self {
            Output::Stdout(stdout) => stdout.flush(),
            Output::File(file) => file.flush(),
        }
    }
}
