use log::debug;
use std::{
    fmt::Display,
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
};

use ash::vk;

use crate::error::Error;

use self::analysis::DescriptorInfo;

use super::device::Device;

pub mod analysis;

fn compile_shader_file(file: &Path) -> Result<shaderc::CompilationArtifact, Error> {
    const MAGIC_NUMBER: u32 = 0x0723_0203;

    let source = fs::read_to_string(file)?;
    let compiler = shaderc::Compiler::new()
        .ok_or_else(|| Error::Local("Failed to create shaderc compiler".to_owned()))?;

    let file_name = file.file_name().unwrap().to_str().unwrap();
    let binary = compiler.compile_into_spirv(
        &source,
        shaderc::ShaderKind::Compute,
        file_name,
        "main",
        None,
    )?;

    binary
        .as_binary()
        .first()
        .is_some_and(|word| *word == MAGIC_NUMBER)
        .then_some(binary)
        .ok_or_else(|| Error::Local("Shader compilation produced invalid output".to_owned()))
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
        for declaration in &self.variable_declarations {
            writeln!(f, "    {}:", declaration.name)?;
            writeln!(f, "      Type:    {:?}", declaration.storage())?;
            writeln!(f, "      Set:     {:?}", declaration.set)?;
            writeln!(f, "      Binding: {:?}", declaration.binding)?;
        }
        writeln!(f, "  Block Declarations:")?;
        for declaration in &self.block_declarations {
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
        let shader_info = vk::ShaderModuleCreateInfo::builder().code(shader_content.as_binary());
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

        // debug!("Compiled shader: {shader_module}");
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
}

impl Drop for ShaderModule {
    fn drop(self: &mut ShaderModule) {
        debug!("Destroying shader module");
        unsafe {
            self.device.destroy_shader_module(self.shader_module, None);
        }
    }
}
