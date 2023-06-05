#[derive(Debug)]
pub enum Error {
    Local(String),
    Vk(ash::vk::Result),
    Os(winit::error::OsError),
    Io(std::io::Error),
    Parse(glsl::parser::ParseError),
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

impl From<glsl::parser::ParseError> for Error {
    fn from(value: glsl::parser::ParseError) -> Self {
        Error::Parse(value)
    }
}
