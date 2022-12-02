use std::io;

use ash::vk;
use winit::error::OsError;

#[derive(Debug)]
pub enum Error {
    LocalError(String),
    VkError(vk::Result),
    OsError(OsError),
    IoError(io::Error),
}
