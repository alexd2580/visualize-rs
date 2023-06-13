use std::path::Path;

use filetime::FileTime;

use crate::error::Error;

pub fn mtime(path: &Path) -> Result<FileTime, Error> {
    let metadata = path.metadata().map_err(|err| {
        Error::Local(format!("File '{}' cannot be read: {err:?}", path.display()))
    })?;
    Ok(FileTime::from_last_modification_time(&metadata))
}

pub fn map_snd<A, B, C>(f: &dyn Fn(B) -> C) -> impl Fn((A, B)) -> (A, C) + '_ {
    |(a, b)| (a, f(b))
}
