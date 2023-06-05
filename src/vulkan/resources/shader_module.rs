use filetime::FileTime;
use glsl::{parser::Parse as _, syntax};
use log::{debug, error};
use std::{
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

    if res.status.code() != Some(0) {
        error!("\n{}", str::from_utf8(&res.stderr).unwrap());
    }

    let mut shader_spirv_bytes = Cursor::new(fs::read("shaders/out.spv")?);
    read_spv(&mut shader_spirv_bytes)
}

fn mtime(path: &Path) -> Result<FileTime, Error> {
    let source_metadata = path.metadata()?;
    Ok(FileTime::from_last_modification_time(&source_metadata))
}

fn match_globals(
    type_qualifier: &syntax::TypeQualifier,
    _global_names: &[syntax::Identifier],
) -> Result<LocalSize, Error> {
    let mut local_size = (1, 1, 1);

    let syntax::TypeQualifier {
        qualifiers: syntax::NonEmpty(ref type_qualifier_specs),
    } = type_qualifier;

    for type_qualifier_spec in type_qualifier_specs {
        match type_qualifier_spec {
            // We expect "In" storage for globals.
            syntax::TypeQualifierSpec::Storage(syntax::StorageQualifier::In) => {}
            // The layout contains the local size.
            &syntax::TypeQualifierSpec::Layout(syntax::LayoutQualifier {
                ids: syntax::NonEmpty(ref ids),
            }) => {
                for id in ids {
                    // Unpack layout qualifier spec, expect identifiers only.
                    let (name, value_box) = if let &syntax::LayoutQualifierSpec::Identifier(
                        syntax::Identifier(ref name),
                        Some(ref value_box),
                    ) = id
                    {
                        (name, value_box)
                    } else {
                        let msg = format!("Unexpected layout qualifier spec: {id:?}");
                        return Err(Error::Local(msg));
                    };

                    // Currently we only expect int values.
                    let value = if let syntax::Expr::IntConst(value) = **value_box {
                        value as u32
                    } else {
                        let msg = format!("Unexpected value: {:?}", **value_box);
                        return Err(Error::Local(msg));
                    };

                    match name.as_str() {
                        "local_size_x" => local_size.0 = value,
                        "local_size_y" => local_size.1 = value,
                        "local_size_z" => local_size.2 = value,
                        _other_name => {
                            let msg = format!("Unexpected layout identifier: {name}");
                            return Err(Error::Local(msg));
                        }
                    }
                }
            }
            unexpected => {
                let msg = format!("Unexpected type qualifier spec: {unexpected:?}");
                return Err(Error::Local(msg));
            }
        }
    }

    Ok(local_size)
}

#[derive(Debug)]
pub struct VariableDeclaration {
    pub name: String,
    pub type_specifier: syntax::TypeSpecifierNonArray,
    pub binding: i32,
    pub type_format: Option<String>,
}

fn match_init_declarator_list(
    init_declarator_list: &syntax::InitDeclaratorList,
) -> Result<VariableDeclaration, Error> {
    let &syntax::InitDeclaratorList {
        head:
            syntax::SingleDeclaration {
                ty:
                    syntax::FullySpecifiedType {
                        qualifier: ref type_qualifier,
                        ty:
                            syntax::TypeSpecifier {
                                ty: ref type_specifier,
                                array_specifier: ref inner_array_specifier,
                            },
                    },
                ref name,
                ref array_specifier,
                ref initializer,
            },
        ref tail,
    } = init_declarator_list;

    let type_qualifier_specs = if let Some(syntax::TypeQualifier {
        qualifiers: syntax::NonEmpty(ref type_qualifier_specs),
    }) = type_qualifier
    {
        type_qualifier_specs
    } else {
        let msg = format!("Unexpected type qualifier: {type_qualifier:?}");
        return Err(Error::Local(msg));
    };

    let mut binding = None;
    let mut type_format = None;

    for type_qualifier_spec in type_qualifier_specs {
        match type_qualifier_spec {
            // We assume that the storage is `Uniform`.
            syntax::TypeQualifierSpec::Storage(syntax::StorageQualifier::Uniform) => {}
            // The layout type qualifier contains the binding and the type format.
            syntax::TypeQualifierSpec::Layout(syntax::LayoutQualifier {
                ids: syntax::NonEmpty(ids),
            }) => {
                for id in ids {
                    let (name, maybe_value_box) = if let &syntax::LayoutQualifierSpec::Identifier(
                        syntax::Identifier(ref name),
                        ref maybe_value_box,
                    ) = id
                    {
                        (name, maybe_value_box)
                    } else {
                        let msg = format!("Unexpected layout qualifier spec: {id:?}");
                        return Err(Error::Local(msg));
                    };

                    match (name.as_str(), maybe_value_box) {
                        ("binding", Some(ref value_box)) => {
                            // Currently we only expect int values for bindings.
                            let value = if let syntax::Expr::IntConst(value) = **value_box {
                                value
                            } else {
                                let msg = format!("Unexpected value: {:?}", **value_box);
                                return Err(Error::Local(msg));
                            };

                            binding = Some(value);
                        }
                        ("rgba32f", None) => type_format = Some("rgba32f".to_owned()),
                        unexpected => {
                            let msg = format!("Unexpected layout identifier: {unexpected:?}");
                            return Err(Error::Local(msg));
                        }
                    }
                }
            }
            unexpected => {
                let msg = format!("Unexpected type qualifier spec: {unexpected:?}");
                return Err(Error::Local(msg));
            }
        }
    }

    let binding = binding.ok_or_else(|| Error::Local("No binding found".to_owned()))?;

    let type_specifier = type_specifier.clone();

    if inner_array_specifier.is_some() {
        let msg = format!("Unexpected inner array specifier: {inner_array_specifier:?}");
        return Err(Error::Local(msg));
    }

    let name = if let &Some(syntax::Identifier(ref name)) = name {
        name.clone()
    } else {
        let msg = format!("Unexpected variable name: {name:?}");
        return Err(Error::Local(msg));
    };

    if array_specifier.is_some() {
        let msg = format!("Unexpected array specifier: {array_specifier:?}");
        return Err(Error::Local(msg));
    }

    if initializer.is_some() {
        let msg = format!("Unexpected initializer: {initializer:?}");
        return Err(Error::Local(msg));
    }

    if !tail.is_empty() {
        let msg = format!("Unexpected tail: {tail:?}");
        return Err(Error::Local(msg));
    }

    Ok(VariableDeclaration {
        name,
        type_specifier,
        binding,
        type_format,
    })
}

#[derive(Debug)]
pub struct BlockField {
    _name: String,
    type_specifier: syntax::TypeSpecifierNonArray,
    _offset: Option<i32>,
    _dimensions: Option<Vec<Option<i32>>>,
}

impl BlockField {
    pub fn byte_size(&self) -> Option<u32> {
        let item_size = match &self.type_specifier {
            syntax::TypeSpecifierNonArray::Void => 1,
            syntax::TypeSpecifierNonArray::Bool => 1,
            syntax::TypeSpecifierNonArray::Int => 4,
            syntax::TypeSpecifierNonArray::UInt => 4,
            syntax::TypeSpecifierNonArray::Float => 4,
            syntax::TypeSpecifierNonArray::Double => 8,
            syntax::TypeSpecifierNonArray::Vec2 => 8,
            syntax::TypeSpecifierNonArray::Vec3 => 12,
            syntax::TypeSpecifierNonArray::Vec4 => 16,
            syntax::TypeSpecifierNonArray::IVec2 => 8,
            syntax::TypeSpecifierNonArray::IVec3 => 12,
            syntax::TypeSpecifierNonArray::IVec4 => 16,
            syntax::TypeSpecifierNonArray::UVec2 => 8,
            syntax::TypeSpecifierNonArray::UVec3 => 12,
            syntax::TypeSpecifierNonArray::UVec4 => 16,
            syntax::TypeSpecifierNonArray::Mat2 => 4 * 4,
            syntax::TypeSpecifierNonArray::Mat3 => 9 * 4,
            syntax::TypeSpecifierNonArray::Mat4 => 16 * 4,
            syntax::TypeSpecifierNonArray::Mat23 => 6 * 4,
            syntax::TypeSpecifierNonArray::Mat24 => 8 * 4,
            syntax::TypeSpecifierNonArray::Mat32 => 6 * 4,
            syntax::TypeSpecifierNonArray::Mat34 => 12 * 4,
            syntax::TypeSpecifierNonArray::Mat42 => 8 * 4,
            syntax::TypeSpecifierNonArray::Mat43 => 12 * 4,
            unexpected => panic!("Haven't implemented size map for type {unexpected:?}"),
        };

        Some(item_size)
    }
}

fn match_block_field(block_field: &syntax::StructFieldSpecifier) -> Result<BlockField, Error> {
    let &syntax::StructFieldSpecifier {
        ref qualifier,
        ty:
            syntax::TypeSpecifier {
                ty: ref type_specifier,
                array_specifier: ref type_array_specifier,
            },
        identifiers: syntax::NonEmpty(ref identifiers),
    } = block_field;

    let mut offset = None;

    if let &Some(syntax::TypeQualifier {
        qualifiers: syntax::NonEmpty(ref type_qualifier_specs),
    }) = qualifier
    {
        for type_qualifier_spec in type_qualifier_specs {
            match type_qualifier_spec {
                &syntax::TypeQualifierSpec::Layout(syntax::LayoutQualifier {
                    ids: syntax::NonEmpty(ref ids),
                }) => {
                    for id in ids {
                        let (name, maybe_value_box) =
                            if let &syntax::LayoutQualifierSpec::Identifier(
                                syntax::Identifier(ref name),
                                ref maybe_value_box,
                            ) = id
                            {
                                (name, maybe_value_box)
                            } else {
                                let msg = format!("Unexpected layout qualifier spec: {id:?}");
                                return Err(Error::Local(msg));
                            };

                        match (name.as_str(), maybe_value_box) {
                            ("offset", Some(ref value_box)) => {
                                // Currently we only expect int values for bindings.
                                let value = if let syntax::Expr::IntConst(value) = **value_box {
                                    value
                                } else {
                                    let msg = format!("Unexpected value: {:?}", **value_box);
                                    return Err(Error::Local(msg));
                                };

                                offset = Some(value);
                            }
                            unexpected => {
                                let msg = format!("Unexpected layout identifier: {unexpected:?}");
                                return Err(Error::Local(msg));
                            }
                        }
                    }
                }
                unexpected => {
                    let msg = format!("Unexpected type qualifier spec: {unexpected:?}");
                    return Err(Error::Local(msg));
                }
            }
        }
    }

    let type_specifier = type_specifier.clone();
    if type_array_specifier.is_some() {
        let msg = format!("Unexpected type array specifier: {type_array_specifier:?}");
        return Err(Error::Local(msg));
    }

    let (name, dimensions) = if let [syntax::ArrayedIdentifier {
        ident: syntax::Identifier(ref name),
        ref array_spec,
    }] = identifiers[..]
    {
        let name = name.clone();
        let dimensions = if let &Some(syntax::ArraySpecifier {
            dimensions: syntax::NonEmpty(ref dimensions),
        }) = array_spec
        {
            Some(
                dimensions
                    .iter()
                    .map(|sizing| {
                        if let syntax::ArraySpecifierDimension::ExplicitlySized(expr_box) = sizing {
                            if let syntax::Expr::IntConst(value) = **expr_box {
                                Ok(Some(value))
                            } else {
                                let msg =
                                    format!("Unexpected array dimension value: {:?}", **expr_box);
                                Err(Error::Local(msg))
                            }
                        } else {
                            Ok(None)
                        }
                    })
                    .collect::<Result<Vec<Option<i32>>, Error>>()?,
            )
        } else {
            None
        };

        (name, dimensions)
    } else {
        let msg = format!("Unexpected identifiers: {identifiers:?}");
        return Err(Error::Local(msg));
    };

    Ok(BlockField {
        _name: name,
        type_specifier,
        _offset: offset,
        _dimensions: dimensions,
    })
}

#[derive(Debug)]
pub struct BlockDeclaration {
    pub name: String,
    pub identifier: Option<String>,
    pub storage: vk::DescriptorType,
    pub binding: Option<u32>,
    pub layout_qualifiers: Vec<String>,
    pub fields: Vec<BlockField>,
}

impl BlockDeclaration {
    pub fn byte_size(&self) -> Option<u32> {
        self.fields.iter().fold(Some(0), |acc, item| {
            acc.and_then(|acc| item.byte_size().map(|item| acc + item))
        })
    }
}

fn match_block(block: &syntax::Block) -> Result<BlockDeclaration, Error> {
    let syntax::Block {
        qualifier:
            syntax::TypeQualifier {
                qualifiers: syntax::NonEmpty(ref type_qualifier_specs),
            },
        name: syntax::Identifier(ref name),
        ref fields,
        ref identifier,
    } = block;

    let name = name.clone();

    let identifier = if let &Some(syntax::ArrayedIdentifier {
        ident: syntax::Identifier(ref identifier),
        ref array_spec,
    }) = identifier
    {
        if array_spec.is_some() {
            let msg = format!("Unexpected array spec: {array_spec:?}");
            return Err(Error::Local(msg));
        }
        Some(identifier.clone())
    } else {
        None
    };

    let mut storage = None;
    let mut binding = None;
    let mut layout_qualifiers = Vec::new();

    for type_qualifier_spec in type_qualifier_specs {
        match type_qualifier_spec {
            syntax::TypeQualifierSpec::Storage(ref storage_qualifier) => {
                storage = Some(match storage_qualifier {
                    syntax::StorageQualifier::Uniform => vk::DescriptorType::UNIFORM_BUFFER,
                    syntax::StorageQualifier::Buffer => vk::DescriptorType::STORAGE_BUFFER,
                    unexpected => {
                        let msg = format!("Unexpected storage qualifier: {unexpected:?}");
                        return Err(Error::Local(msg));
                    }
                })
            }
            syntax::TypeQualifierSpec::Layout(syntax::LayoutQualifier {
                ids: syntax::NonEmpty(ids),
            }) => {
                for id in ids {
                    let (name, maybe_value_box) = if let &syntax::LayoutQualifierSpec::Identifier(
                        syntax::Identifier(ref name),
                        ref maybe_value_box,
                    ) = id
                    {
                        (name, maybe_value_box)
                    } else {
                        let msg = format!("Unexpected layout qualifier spec: {id:?}");
                        return Err(Error::Local(msg));
                    };

                    match (name.as_str(), maybe_value_box) {
                        ("binding", Some(ref value_box)) => {
                            // Currently we only expect int values for bindings.
                            let value = if let syntax::Expr::IntConst(value) = **value_box {
                                value
                            } else {
                                let msg = format!("Unexpected value: {:?}", **value_box);
                                return Err(Error::Local(msg));
                            };

                            binding = Some(value as u32);
                        }
                        ("push_constant", None) => layout_qualifiers.push(name.clone()),
                        ("std140", None) => layout_qualifiers.push(name.clone()),
                        unexpected => {
                            let msg = format!("Unexpected layout identifier: {unexpected:?}");
                            return Err(Error::Local(msg));
                        }
                    }
                }
            }
            unexpected => {
                let msg = format!("Unexpected type qualifier spec: {unexpected:?}");
                return Err(Error::Local(msg));
            }
        }
    }

    let storage = storage.ok_or_else(|| Error::Local("No storage qualifier found".to_owned()))?;

    let fields = fields
        .iter()
        .map(match_block_field)
        .collect::<Result<Vec<BlockField>, Error>>()?;

    Ok(BlockDeclaration {
        name,
        identifier,
        storage,
        binding,
        layout_qualifiers,
        fields,
    })
}

type LocalSize = (u32, u32, u32);
type ShaderIO = (LocalSize, Vec<VariableDeclaration>, Vec<BlockDeclaration>);

fn analyze_shader(file: &Path) -> Result<ShaderIO, Error> {
    let shader_code = fs::read_to_string(file)?;
    let syntax::TranslationUnit(syntax::NonEmpty(external_declarations)) =
        syntax::ShaderStage::parse(shader_code)?;

    let mut local_size = (1, 1, 1);
    let mut declarations = Vec::new();
    let mut blocks = Vec::new();

    for external_declaration in external_declarations.iter() {
        match external_declaration {
            syntax::ExternalDeclaration::Declaration(declaration) => match declaration {
                // Global declarations include the local size of the shader.
                // This is relevant for the dispatch size.
                syntax::Declaration::Global(type_qualifier, global_names) => {
                    local_size = match_globals(type_qualifier, global_names)?
                }
                // Init declarator lists define images accessed via samplers.
                syntax::Declaration::InitDeclaratorList(init_declarator_list) => {
                    declarations.push(match_init_declarator_list(init_declarator_list)?)
                }
                syntax::Declaration::Block(block) => blocks.push(match_block(block)?),
                // Ignore the following.
                syntax::Declaration::Precision(..) => {}
                syntax::Declaration::FunctionPrototype(..) => {}
            },
            // Ignore the following.
            syntax::ExternalDeclaration::Preprocessor(..) => {}
            syntax::ExternalDeclaration::FunctionDefinition(..) => {}
        }
    }

    Ok((local_size, declarations, blocks))
}

pub struct ShaderModule {
    device: Rc<Device>,
    source_path: PathBuf,
    mtime: FileTime,
    shader_module: vk::ShaderModule,
    pub local_size: LocalSize,
    pub variable_declarations: Vec<VariableDeclaration>,
    pub block_declarations: Vec<BlockDeclaration>,
}

impl Deref for ShaderModule {
    type Target = vk::ShaderModule;

    fn deref(&self) -> &Self::Target {
        &self.shader_module
    }
}

impl ShaderModule {
    pub unsafe fn new(device: &Rc<Device>, source_path: &Path) -> Result<Rc<Self>, Error> {
        debug!("Creating shader module");
        let device = device.clone();
        let source_path = source_path.to_path_buf();
        let mtime = mtime(&source_path)?;

        debug!("Compiling shader");
        let shader_content = compile_shader_file(&source_path)?;

        let shader_info = vk::ShaderModuleCreateInfo::builder().code(&shader_content);
        let shader_module = device.create_shader_module(&shader_info, None)?;

        let (local_size, variable_declarations, block_declarations) = analyze_shader(&source_path)?;

        Ok(Rc::new(ShaderModule {
            device,
            source_path,
            mtime,
            shader_module,
            local_size,
            variable_declarations,
            block_declarations,
        }))
    }

    pub fn was_modified(&self) -> bool {
        mtime(&self.source_path).unwrap() > self.mtime
    }

    pub unsafe fn rebuild(&self) -> Result<Rc<Self>, Error> {
        ShaderModule::new(&self.device, &self.source_path)
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
