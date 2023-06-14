use std::{
    fmt,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use log::{debug, warn};

use ash::vk;

use crate::error::Error;

use super::{buffer::Buffer, image_view::ImageView, sampler::Sampler, shader_module::ShaderModule};

fn write_descriptor_set_builder_stub(
    descriptor_binding: u32,
    storage_type: vk::DescriptorType,
) -> vk::WriteDescriptorSetBuilder<'static> {
    vk::WriteDescriptorSet::builder()
        .descriptor_type(storage_type)
        .dst_binding(descriptor_binding)
        .dst_array_element(0)
}

/// The info object needs to be boxed so that `vk::WriteDescriptorSet`
/// can reference that memory via pointer.
type DescriptorBindingImageData = (
    Rc<ImageView>,
    Rc<Sampler>,
    Box<[vk::DescriptorImageInfo; 1]>,
);

/// The info object needs to be boxed so that `vk::WriteDescriptorSet`
/// can reference that memory via pointer.
type DescriptorBindingBufferData = (Rc<Buffer>, Box<[vk::DescriptorBufferInfo; 1]>);

/// Images and buffers have different bind data.
enum DescriptorBindingData {
    Image(DescriptorBindingImageData),
    Buffer(DescriptorBindingBufferData),
}

impl fmt::Debug for DescriptorBindingData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DescriptorBindingData::")?;
        match self {
            DescriptorBindingData::Image(..) => write!(f, "Image(..)"),
            DescriptorBindingData::Buffer(..) => write!(f, "Buffer(..)"),
        }
    }
}

#[derive(Debug)]
pub struct DescriptorBindingInstance {
    #[allow(dead_code)]
    data: DescriptorBindingData,
    /// A cached version of `vk::WriteDescriptorSet`. Can be used with
    pub write_info: vk::WriteDescriptorSet,
}

#[derive(Debug)]
pub struct DescriptorBinding {
    /// Name of the object.
    pub name: String,

    /// Binding index of the object (specified in the shader).
    binding: u32,

    /// The type of the underlying buffer/image.
    storage_type: vk::DescriptorType,

    /// Instances, actual data, to be bound. Created and linked in application code.
    pub instances: Vec<DescriptorBindingInstance>,
}

impl DescriptorBinding {
    pub fn as_descriptor_set_layout_binding(&self) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding: self.binding,
            descriptor_type: self.storage_type,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            ..Default::default()
        }
    }
}

/// Descriptor sets have multiple instances which can be bound. This is per-shader data, binding
/// indices do not need to be consistent across shaders. Currently the final mapping is done via
/// buffer/image name.
pub struct Descriptors(Vec<DescriptorBinding>);

impl Deref for Descriptors {
    type Target = [DescriptorBinding];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Descriptors {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Descriptors {
    pub fn new(shader_module: &ShaderModule) -> Result<Self, Error> {
        debug!("Creating descriptor bindings");

        // TODO immutable samplers, what are immutable samplers???
        // Are these always storage images?
        let vars = shader_module
            .variable_declarations
            .iter()
            .filter(|declaration| declaration.binding.is_some())
            .map(|declaration| DescriptorBinding {
                name: declaration.name.to_owned(),
                binding: declaration.binding.unwrap(),
                storage_type: vk::DescriptorType::STORAGE_IMAGE, // TODO Could be texture type.
                instances: Vec::new(),
            });

        let blocks = shader_module
            .block_declarations
            .iter()
            .filter(|declaration| declaration.binding.is_some())
            .map(|declaration| DescriptorBinding {
                name: declaration.identifier.as_ref().unwrap().to_owned(),
                binding: declaration.binding.unwrap(),
                storage_type: declaration.storage,
                instances: Vec::new(),
            });

        Ok(Descriptors(vars.chain(blocks).collect()))
    }

    pub fn link_image(&mut self, name: &str, images: &[(Rc<ImageView>, Rc<Sampler>)]) {
        fn make_descriptor_binding_image_data(
            (image_view, sampler): &(Rc<ImageView>, Rc<Sampler>),
        ) -> DescriptorBindingImageData {
            (
                image_view.clone(),
                sampler.clone(),
                Box::new([vk::DescriptorImageInfo::builder()
                    .image_view(***image_view)
                    .sampler(***sampler)
                    .image_layout(vk::ImageLayout::GENERAL)
                    .build()]),
            )
        }

        self.iter_mut()
            .filter(|binding| binding.name == name)
            .for_each(|binding| {
                if !binding.instances.is_empty() {
                    warn!("Rebinding {name}");
                    binding.instances.clear();
                }

                // TODO check storage type.

                let make_descriptor_binding_instance = |image_data: DescriptorBindingImageData| {
                    let write_info =
                        write_descriptor_set_builder_stub(binding.binding, binding.storage_type)
                            .image_info(image_data.2.as_ref())
                            .build();
                    let data = DescriptorBindingData::Image(image_data);

                    DescriptorBindingInstance { data, write_info }
                };
                binding.instances.extend(
                    images
                        .iter()
                        .map(make_descriptor_binding_image_data)
                        .map(make_descriptor_binding_instance),
                );
            });
    }

    pub fn link_buffer(&mut self, name: &str, buffers: &[Rc<Buffer>]) {
        fn make_descriptor_binding_buffer_data(buffer: &Rc<Buffer>) -> DescriptorBindingBufferData {
            (
                buffer.clone(),
                Box::new([vk::DescriptorBufferInfo::builder()
                    .buffer(***buffer)
                    .offset(0)
                    .range(buffer.size)
                    .build()]),
            )
        }

        self.iter_mut()
            .filter(|binding| binding.name == name)
            .for_each(|binding| {
                if !binding.instances.is_empty() {
                    warn!("Rebinding {name}");
                    binding.instances.clear();
                }

                // TODO check storage type.

                let make_descriptor_binding_instance =
                    |buffer_data: DescriptorBindingBufferData| {
                        let write_info = write_descriptor_set_builder_stub(
                            binding.binding,
                            binding.storage_type,
                        )
                        .buffer_info(buffer_data.1.as_ref())
                        .build();
                        let data = DescriptorBindingData::Buffer(buffer_data);

                        DescriptorBindingInstance { data, write_info }
                    };
                binding.instances.extend(
                    buffers
                        .iter()
                        .map(make_descriptor_binding_buffer_data)
                        .map(make_descriptor_binding_instance),
                );
            });
    }
}
