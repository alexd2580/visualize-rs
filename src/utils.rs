use std::path::Path;

use filetime::FileTime;

use crate::error::Error;

pub fn mtime(path: &Path) -> Result<FileTime, Error> {
    let metadata = path.metadata().map_err(|err| {
        Error::Local(format!("File '{}' cannot be read: {err:?}", path.display()))
    })?;
    Ok(FileTime::from_last_modification_time(&metadata))
}
