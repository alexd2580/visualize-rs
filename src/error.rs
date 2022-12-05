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
