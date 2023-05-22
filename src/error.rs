use std::io;

use ash::vk;
use winit::error::OsError;

#[derive(Debug)]
pub enum Error {
    Local(String),
    Vk(vk::Result),
    Os(OsError),
    Io(io::Error),
}

impl From<ash::vk::Result> for Error {
    fn from(value: ash::vk::Result) -> Self {
        Error::Vk(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::Io(value)
    }
}

impl From<winit::error::OsError> for Error {
    fn from(value: winit::error::OsError) -> Self {
        Error::Os(value)
    }
}
