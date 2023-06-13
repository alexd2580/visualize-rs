use log::debug;
use std::{
    fmt::Display,
    fs,
    io::{self, Cursor},
    ops::Deref,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    slice, str,
};

use ash::vk;

use crate::error::Error;

use super::device::Device;

pub mod analysis;

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

fn compile_shader_file(file: &Path) -> Result<Vec<u32>, Error> {
    let res = Command::new("glslc")
        .args([file.to_str().unwrap(), "-o", "shaders/out.spv"])
        .output()?;

    if res.status.code() != Some(0) {
        let msg = str::from_utf8(&res.stderr).unwrap().to_owned();
        return Err(Error::Local(msg));
    }

    let mut shader_spirv_bytes = Cursor::new(fs::read("shaders/out.spv")?);
    Ok(read_spv(&mut shader_spirv_bytes)?)
}

pub struct ShaderModule {
    device: Rc<Device>,
    pub source_path: PathBuf,
    shader_module: vk::ShaderModule,
    pub local_size: analysis::LocalSize,
    pub variable_declarations: Vec<analysis::VariableDeclaration>,
    pub block_declarations: Vec<analysis::BlockDeclaration>,

    pub main_name: String,
}

impl Deref for ShaderModule {
    type Target = vk::ShaderModule;

    fn deref(&self) -> &Self::Target {
        &self.shader_module
    }
}

impl Display for ShaderModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Shader module {:?}:", self.source_path)?;
        writeln!(f, "  Main name:  {}", self.main_name)?;
        writeln!(f, "  Local size: {:?}", self.local_size)?;
        writeln!(f, "  Variable Declarations:")?;
        for declaration in self.variable_declarations.iter() {
            writeln!(f, "    {}:", declaration.name)?;
            writeln!(f, "      Type:    {:?}", vk::DescriptorType::STORAGE_IMAGE)?;
            writeln!(f, "      Set:     {:?}", declaration.set)?;
            writeln!(f, "      Binding: {:?}", declaration.binding)?;
        }
        writeln!(f, "  Block Declarations:")?;
        for declaration in self.block_declarations.iter() {
            writeln!(f, "    {} {:?}:", declaration.name, declaration.identifier)?;
            writeln!(f, "      Type:    {:?}", declaration.storage)?;
            writeln!(f, "      Set:     {:?}", declaration.set)?;
            writeln!(f, "      Binding: {:?}", declaration.binding)?;
        }
        Ok(())
    }
}

impl ShaderModule {
    pub unsafe fn new(device: &Rc<Device>, source_path: &Path) -> Result<Rc<Self>, Error> {
        debug!("Creating shader module");
        let (local_size, variable_declarations, block_declarations) =
            analysis::analyze_shader(source_path)?;

        let device = device.clone();
        let source_path = source_path.to_path_buf();

        debug!("Compiling shader");
        let shader_content = compile_shader_file(&source_path)?;
        let shader_info = vk::ShaderModuleCreateInfo::builder().code(&shader_content);
        let shader_module = device.create_shader_module(&shader_info, None)?;
        let main_name = "main".to_owned();

        let shader_module = ShaderModule {
            device,
            source_path,
            shader_module,
            local_size,
            variable_declarations,
            block_declarations,
            main_name,
        };

        debug!("Compiled shader: {shader_module}");
        Ok(Rc::new(shader_module))
    }

    pub fn push_constants_declaration(&self) -> Option<&analysis::BlockDeclaration> {
        self.block_declarations.iter().find(|declaration| {
            declaration
                .layout_qualifiers
                .iter()
                .any(|qualifier| qualifier == "push_constant")
        })
    }

    pub fn variable_declaration(&self, name: &str) -> Option<&analysis::VariableDeclaration> {
        self.variable_declarations
            .iter()
            .find(|declaration| declaration.name == name)
    }

    pub fn block_declaration(&self, name: &str) -> Option<&analysis::BlockDeclaration> {
        self.block_declarations.iter().find(|declaration| {
            declaration
                .identifier
                .as_ref()
                .is_some_and(|val| val == name)
        })
    }
}

impl Drop for ShaderModule {
    fn drop(self: &mut ShaderModule) {
        debug!("Destroying shader module");
        unsafe {
            self.device.destroy_shader_module(self.shader_module, None);
        }
    }
}
