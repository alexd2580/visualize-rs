use std::fmt::Display;

#[derive(Debug)]
pub enum Cpal {
    SupportedStreamConfigs(cpal::SupportedStreamConfigsError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
}

#[derive(Debug)]
pub enum Error {
    Local(String),
    Vk(ash::vk::Result),
    Os(winit::error::OsError),
    Io(std::io::Error),
    Parse(glsl::parser::ParseError),
    Cpal(Cpal),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Local(str) => write!(f, "{str}"),
            Error::Vk(code) => write!(f, "{code}"),
            Error::Os(os_error) => write!(f, "OS Error\n{os_error}"),
            Error::Io(io_error) => write!(f, "IO Error\n{io_error}"),
            Error::Parse(glsl::parser::ParseError { info }) => {
                write!(f, "Failed to parse GLSL\n{info}")
            }
            Error::Cpal(cpal_error) => write!(f, "CPAL Error\n{cpal_error:?}"),
        }
    }
}

impl From<ash::vk::Result> for Error {
    fn from(value: ash::vk::Result) -> Self {
        Self::Vk(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<winit::error::OsError> for Error {
    fn from(value: winit::error::OsError) -> Self {
        Self::Os(value)
    }
}

impl From<glsl::parser::ParseError> for Error {
    fn from(value: glsl::parser::ParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<cpal::SupportedStreamConfigsError> for Error {
    fn from(value: cpal::SupportedStreamConfigsError) -> Self {
        Self::Cpal(Cpal::SupportedStreamConfigs(value))
    }
}

impl From<cpal::BuildStreamError> for Error {
    fn from(value: cpal::BuildStreamError) -> Self {
        Self::Cpal(Cpal::BuildStream(value))
    }
}

impl From<cpal::PlayStreamError> for Error {
    fn from(value: cpal::PlayStreamError) -> Self {
        Self::Cpal(Cpal::PlayStream(value))
    }
}
