use ash::vk;

use filetime::FileTime;
use log::debug;

use std::{
    ffi::CStr,
    fs,
    io::{self, Cursor},
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    slice,
};

use crate::error::Error;

use super::device::Device;

/// Decode SPIR-V from bytes.
///
/// This function handles SPIR-V of arbitrary endianness gracefully, and returns correctly aligned
/// storage.
///
/// # Examples
/// ```no_run
/// // Decode SPIR-V from a file
/// let mut file = std::fs::File::open("/path/to/shader.spv").unwrap();
/// let words = ash::util::read_spv(&mut file).unwrap();
/// ```
/// ```
/// // Decode SPIR-V from memory
/// const SPIRV: &[u8] = &[
///     // ...
/// #   0x03, 0x02, 0x23, 0x07,
/// ];
/// let words = ash::util::read_spv(&mut std::io::Cursor::new(&SPIRV[..])).unwrap();
/// ```
pub fn read_spv<R: io::Read + io::Seek>(x: &mut R) -> io::Result<Vec<u32>> {
    let size = x.seek(io::SeekFrom::End(0))?;
    if size % 4 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "input length not divisible by 4",
        ));
    }
    if size > usize::max_value() as u64 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "input too long"));
    }
    let words = (size / 4) as usize;
    // https://github.com/MaikKlein/ash/issues/354:
    // Zero-initialize the result to prevent read_exact from possibly
    // reading uninitialized memory.
    let mut result = vec![0u32; words];
    x.seek(io::SeekFrom::Start(0))?;
    x.read_exact(unsafe { slice::from_raw_parts_mut(result.as_mut_ptr() as *mut u8, words * 4) })?;
    const MAGIC_NUMBER: u32 = 0x0723_0203;
    if !result.is_empty() && result[0] == MAGIC_NUMBER.swap_bytes() {
        for word in &mut result {
            *word = word.swap_bytes();
        }
    }
    if result.is_empty() || result[0] != MAGIC_NUMBER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "input missing SPIR-V magic number",
        ));
    }
    Ok(result)
}

fn compile_shader_file(file: &Path) -> io::Result<Vec<u32>> {
    let res = Command::new("glslc")
        .args([file.to_str().unwrap(), "-o", "shaders/out.spv"])
        .output()?;

    debug!("{:?}", res);

    let mut shader_spirv_bytes = Cursor::new(fs::read("shaders/out.spv")?);
    read_spv(&mut shader_spirv_bytes)
}

pub struct ShaderModule {
    device: Rc<Device>,
    source_path: PathBuf,

    last_mtime: FileTime,

    pub shader_module: vk::ShaderModule,
    pub shader_stage_create_info: vk::PipelineShaderStageCreateInfo,
}

impl ShaderModule {
    fn mtime(path: &Path) -> Result<FileTime, Error> {
        let source_metadata = path.metadata()?;
        Ok(FileTime::from_last_modification_time(&source_metadata))
    }

    fn build(device: &Rc<Device>, source_path: &Path) -> Result<ShaderModule, Error> {
        let last_mtime = Self::mtime(source_path)?;

        debug!("Compiling shader");
        let shader_content = compile_shader_file(source_path)?;

        debug!("Initializing shader module");
        let shader_info = vk::ShaderModuleCreateInfo::builder().code(&shader_content);
        let shader_module = unsafe { device.create_shader_module(&shader_info, None) }?;

        let shader_entry_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") };
        let shader_stage_create_info = vk::PipelineShaderStageCreateInfo {
            module: shader_module,
            p_name: shader_entry_name.as_ptr(),
            stage: vk::ShaderStageFlags::COMPUTE,
            ..Default::default()
        };

        Ok(ShaderModule {
            device: device.clone(),
            source_path: source_path.to_path_buf(),
            last_mtime,
            shader_module,
            shader_stage_create_info,
        })
    }

    pub fn new(device: Rc<Device>) -> Result<Self, Error> {
        debug!("Initializing shader module");
        let source_path = PathBuf::from("shaders/debug.comp");
        ShaderModule::build(&device, &source_path)
    }

    pub fn was_modified(&self) -> bool {
        Self::mtime(&self.source_path).unwrap() > self.last_mtime
    }

    pub fn rebuild(&self) -> Result<ShaderModule, Error> {
        ShaderModule::build(&self.device, &self.source_path)
    }
}

impl Drop for ShaderModule {
    fn drop(self: &mut ShaderModule) {
        debug!("Destroying shader module");
        unsafe { self.device.destroy_shader_module(self.shader_module, None) };
    }
}
