use std::{path::Path, process::Command, str};

use filetime::FileTime;

use crate::error::Error;

pub fn mtime(path: &Path) -> Result<FileTime, Error> {
    let metadata = path.metadata().map_err(|err| {
        Error::Local(format!("File '{}' cannot be read: {err:?}", path.display()))
    })?;
    Ok(FileTime::from_last_modification_time(&metadata))
}

/// alpha = 1 uses 100% of a. alpha = 0 uses 100% of b.
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

/// Relative difference.
/// For reference see <https://en.wikipedia.org/wiki/Relative_change_and_difference>
pub fn relative_delta(a: f32, b: f32) -> f32 {
    ((a / b) - 1f32).abs()
}

/// Compute the bin index of a frequency (in Hz) for a dft of size `dft_size`.
///
/// # Example
///
/// ```
/// let sample_rate = 44100;
/// let dft_size = 1024;
/// let fq = 100;
/// let bin_index = dft_index_of_frequency(fq, sample_rate, dft_size);
/// assert!(bin_index == 3);
/// ```
pub fn dft_index_of_frequency(frequency: usize, sample_rate: usize, dft_size: usize) -> usize {
    // For reference see
    // https://stackoverflow.com/questions/4364823/how-do-i-obtain-the-frequencies-of-each-value-in-an-fft
    // 0:   0 * 44100 / 1024 =     0.0 Hz
    // 1:   1 * 44100 / 1024 =    43.1 Hz
    // 2:   2 * 44100 / 1024 =    86.1 Hz
    // 3:   3 * 44100 / 1024 =   129.2 Hz
    (frequency as f32 * dft_size as f32 / sample_rate as f32).round() as usize
}
