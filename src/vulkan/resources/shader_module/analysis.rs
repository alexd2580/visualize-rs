use glsl::{parser::Parse as _, syntax};
use log::warn;
use std::{fs, path::Path};

use ash::vk;

use crate::error::Error;

fn simplify_layout_qualifiers(
    layout_qualifier_specs: &[syntax::LayoutQualifierSpec],
) -> impl Iterator<Item = Result<(&str, Option<&syntax::Expr>), Error>> {
    layout_qualifier_specs.iter().map(|layout_qualifier_spec| {
        // Unpack layout qualifier spec, expect identifiers only.
        if let syntax::LayoutQualifierSpec::Identifier(syntax::Identifier(name), maybe_value_box) =
            layout_qualifier_spec
        {
            let maybe_value = maybe_value_box.as_ref().map(|x| &**x);
            Ok((name.as_str(), maybe_value))
        } else {
            let msg = format!("Unexpected layout qualifier spec: {layout_qualifier_spec:?}");
            Err(Error::Local(msg))
        }
    })
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
                for id in simplify_layout_qualifiers(ids) {
                    let (name, maybe_value) = id?;

                    // Currently we only expect int values.
                    let Some(&syntax::Expr::IntConst(value)) = maybe_value else {
                        let msg = format!("Unexpected value: {maybe_value:?}");
                        return Err(Error::Local(msg));
                    };
                    let value = usize::try_from(value).unwrap();

                    match name {
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
                let msg = format!("Unexpected global type qualifier spec: {unexpected:?}");
                return Err(Error::Local(msg));
            }
        }
    }

    Ok(local_size)
}

pub trait DescriptorInfo {
    fn storage(&self) -> vk::DescriptorType;
    fn set_index(&self) -> usize;
    fn binding(&self) -> Result<usize, Error>;
    fn name(&self) -> &str;
}

#[derive(Debug)]
pub struct VariableDeclaration {
    pub name: String,
    pub type_specifier: syntax::TypeSpecifierNonArray,
    pub binding: Option<usize>,
    pub set: Option<usize>,
    pub type_format: Option<String>,
}

impl DescriptorInfo for VariableDeclaration {
    fn storage(&self) -> vk::DescriptorType {
        match self.type_specifier {
            syntax::TypeSpecifierNonArray::Image2D => vk::DescriptorType::STORAGE_IMAGE,
            syntax::TypeSpecifierNonArray::Sampler2D => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            _ => {
                warn!("Assuming STORAGE_IMAGE for {:?}", self.type_specifier);
                vk::DescriptorType::STORAGE_IMAGE
            }
        }
    }

    fn set_index(&self) -> usize {
        self.set.unwrap_or_else(|| {
            warn!("Assuming set=0 for variable {}", self.name);
            0
        })
    }

    fn binding(&self) -> Result<usize, Error> {
        self.binding.ok_or_else(|| {
            let msg = format!("Block '{}' does not specify a binding.", self.name);
            Error::Local(msg)
        })
    }

    fn name(&self) -> &str {
        &self.name
    }
}

fn match_init_declarator_list(
    init_declarator_list: &syntax::InitDeclaratorList,
) -> Result<Option<VariableDeclaration>, Error> {
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

    let Some(syntax::TypeQualifier {
        qualifiers: syntax::NonEmpty(ref type_qualifier_specs),
    }) = type_qualifier
    else {
        let msg = format!("Unexpected type qualifier: {type_qualifier:?}");
        return Err(Error::Local(msg));
    };

    let mut binding = None;
    let mut set = None;
    let mut type_format = None;

    for type_qualifier_spec in type_qualifier_specs {
        match type_qualifier_spec {
            syntax::TypeQualifierSpec::Storage(syntax::StorageQualifier::Const) => return Ok(None),
            // We assume that the storage is `Uniform`.
            syntax::TypeQualifierSpec::Storage(syntax::StorageQualifier::Uniform) => {}
            // The layout type qualifier contains the binding and the type format.
            syntax::TypeQualifierSpec::Layout(syntax::LayoutQualifier {
                ids: syntax::NonEmpty(ids),
            }) => {
                for id in simplify_layout_qualifiers(ids) {
                    let (name, maybe_value) = id?;
                    match (name, maybe_value) {
                        // Currently we only expect int values for bindings.
                        ("binding", Some(&syntax::Expr::IntConst(value))) => {
                            binding = Some(usize::try_from(value).unwrap());
                        }
                        ("rgba32f", None) => type_format = Some(name.to_owned()),
                        ("set", Some(&syntax::Expr::IntConst(value))) => {
                            set = Some(usize::try_from(value).unwrap());
                        }
                        unexpected => {
                            let msg = format!("Unexpected layout identifier: {unexpected:?}");
                            return Err(Error::Local(msg));
                        }
                    }
                }
            }
            unexpected => {
                let msg = format!("Unexpected variable type qualifier spec: {unexpected:?}");
                return Err(Error::Local(msg));
            }
        }
    }

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
        warn!("Unhandled array specifier: {array_specifier:?}");
    }

    if initializer.is_some() {
        warn!("Unhandled initializer: {initializer:?}");
    }

    if !tail.is_empty() {
        let msg = format!("Unexpected tail: {tail:?}");
        return Err(Error::Local(msg));
    }

    Ok(Some(VariableDeclaration {
        name,
        type_specifier,
        binding,
        set,
        type_format,
    }))
}

#[derive(Debug)]
pub struct BlockField {
    pub name: String,
    pub type_specifier: syntax::TypeSpecifierNonArray,
    pub offset: Option<usize>,
    pub dimensions: Option<Vec<Option<usize>>>,
}

impl BlockField {
    // We will check for dimensions and then this will be None-able.
    #[allow(clippy::unnecessary_wraps)]
    pub fn byte_size(&self) -> Option<usize> {
        #[allow(clippy::match_same_arms)]
        Some(match &self.type_specifier {
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
        })
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
                    for id in simplify_layout_qualifiers(ids) {
                        match id? {
                            ("offset", Some(&syntax::Expr::IntConst(value))) => {
                                offset = Some(usize::try_from(value).unwrap());
                            }
                            unexpected => {
                                let msg = format!("Unexpected layout identifier: {unexpected:?}");
                                return Err(Error::Local(msg));
                            }
                        }
                    }
                }
                unexpected => {
                    let msg = format!("Unexpected block field type qualifier spec: {unexpected:?}");
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
                                Ok(Some(usize::try_from(value).unwrap()))
                            } else {
                                let msg =
                                    format!("Unexpected array dimension value: {:?}", **expr_box);
                                Err(Error::Local(msg))
                            }
                        } else {
                            Ok(None)
                        }
                    })
                    .collect::<Result<Vec<Option<usize>>, Error>>()?,
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
        name,
        type_specifier,
        offset,
        dimensions,
    })
}

#[derive(Debug)]
pub struct BlockDeclaration {
    pub name: String,
    pub identifier: Option<String>,
    pub storage: vk::DescriptorType,
    pub binding: Option<usize>,
    pub set: Option<usize>,
    pub layout_qualifiers: Vec<String>,
    pub fields: Vec<BlockField>,
}

impl DescriptorInfo for BlockDeclaration {
    fn storage(&self) -> vk::DescriptorType {
        self.storage
    }

    fn set_index(&self) -> usize {
        self.set.unwrap_or_else(|| {
            warn!("Assuming set=0 for block {}", self.name);
            0 // TODO move this to parsing stage.
        })
    }

    fn binding(&self) -> Result<usize, Error> {
        self.binding.ok_or_else(|| {
            let msg = format!("Block '{}' does not specify a binding.", self.name);
            Error::Local(msg)
        }) // TODO move to parsing stage?
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Alignment cannot be less than 4. Even booleans should be aligned to 4 bytes...
fn alignment(x: usize) -> usize {
    let exp = ((x as f32).log2().ceil() as u32).max(2);
    2usize.pow(exp)
}

impl BlockDeclaration {
    pub fn byte_size(&self) -> Option<usize> {
        let mut max_size = 0;
        for field in &self.fields {
            let byte_size = field.byte_size()?;
            let offset = field.offset?;

            max_size = max_size.max(offset + alignment(byte_size));
        }
        Some(max_size)
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
    let mut set = None;
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
                });
            }
            syntax::TypeQualifierSpec::Layout(syntax::LayoutQualifier {
                ids: syntax::NonEmpty(ids),
            }) => {
                for id in simplify_layout_qualifiers(ids) {
                    let (name, maybe_value) = id?;
                    match (name, maybe_value) {
                        // Currently we only expect int values for bindings.
                        ("binding", Some(&syntax::Expr::IntConst(value))) => {
                            binding = Some(usize::try_from(value).unwrap());
                        }
                        ("push_constant" | "std140", None) => {
                            layout_qualifiers.push(name.to_owned());
                        }
                        ("set", Some(&syntax::Expr::IntConst(value))) => {
                            set = Some(usize::try_from(value).unwrap());
                        }
                        unexpected => {
                            let msg = format!("Unexpected layout identifier: {unexpected:?}");
                            return Err(Error::Local(msg));
                        }
                    }
                }
            }
            unexpected => {
                let msg = format!("Unexpected block type qualifier spec: {unexpected:?}");
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
        set,
        layout_qualifiers,
        fields,
    })
}

pub type LocalSize = (usize, usize, usize);
pub type ShaderIO = (LocalSize, Vec<VariableDeclaration>, Vec<BlockDeclaration>);

pub fn analyze_shader(path: &Path) -> Result<ShaderIO, Error> {
    let shader_code = fs::read_to_string(path).map_err(|err| {
        Error::Local(format!("File '{}' cannot be read: {err:?}", path.display()))
    })?;
    let syntax::TranslationUnit(syntax::NonEmpty(external_declarations)) =
        syntax::ShaderStage::parse(shader_code)?;

    let mut local_size = (1, 1, 1);
    let mut declarations = Vec::new();
    let mut blocks = Vec::new();

    for external_declaration in &external_declarations {
        match external_declaration {
            syntax::ExternalDeclaration::Declaration(declaration) => match declaration {
                // Global declarations include the local size of the shader.
                // This is relevant for the dispatch size.
                syntax::Declaration::Global(type_qualifier, global_names) => {
                    local_size = match_globals(type_qualifier, global_names)?;
                }
                // Init declarator lists define images accessed via samplers.
                syntax::Declaration::InitDeclaratorList(init_declarator_list) => {
                    match_init_declarator_list(init_declarator_list)?
                        .into_iter()
                        .for_each(|declaration| declarations.push(declaration));
                }
                syntax::Declaration::Block(block) => blocks.push(match_block(block)?),
                // Ignore the following.
                syntax::Declaration::Precision(..) | syntax::Declaration::FunctionPrototype(..) => {
                }
            },
            // Ignore the following.
            syntax::ExternalDeclaration::Preprocessor(..)
            | syntax::ExternalDeclaration::FunctionDefinition(..) => {}
        }
    }

    Ok((local_size, declarations, blocks))
}
