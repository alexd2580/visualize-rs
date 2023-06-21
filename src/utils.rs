use std::{path::Path, process::Command, str};

use filetime::FileTime;

use crate::error::Error;

pub fn mtime(path: &Path) -> Result<FileTime, Error> {
    let metadata = path.metadata().map_err(|err| {
        Error::Local(format!("File '{}' cannot be read: {err:?}", path.display()))
    })?;
    Ok(FileTime::from_last_modification_time(&metadata))
}

pub fn mix(a: f32, b: f32, alpha: f32) -> f32 {
    a * alpha + b * (1f32 - alpha)
}

pub fn exec_command(command: &[&str]) -> Result<String, Error> {
    let output = Command::new(command[0]).args(&command[1..]).output()?;

    if output.status.code() == Some(0) {
        let stdout = str::from_utf8(&output.stdout).unwrap().to_owned();
        Ok(stdout)
    } else {
        let msg = str::from_utf8(&output.stderr).unwrap().to_owned();
        Err(Error::Local(msg))
    }
}
