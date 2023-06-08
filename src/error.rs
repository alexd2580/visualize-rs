#[derive(Debug)]
pub enum CpalError {
    SupportedStreamConfigsError(cpal::SupportedStreamConfigsError),
    BuildStreamError(cpal::BuildStreamError),
    PlayStreamError(cpal::PlayStreamError),
}

#[derive(Debug)]
pub enum Error {
    Local(String),
    Vk(ash::vk::Result),
    Os(winit::error::OsError),
    Io(std::io::Error),
    Parse(glsl::parser::ParseError),
    Cpal(CpalError),
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
        Self::Cpal(CpalError::SupportedStreamConfigsError(value))
    }
}

impl From<cpal::BuildStreamError> for Error {
    fn from(value: cpal::BuildStreamError) -> Self {
        Self::Cpal(CpalError::BuildStreamError(value))
    }
}

impl From<cpal::PlayStreamError> for Error {
    fn from(value: cpal::PlayStreamError) -> Self {
        Self::Cpal(CpalError::PlayStreamError(value))
    }
}
