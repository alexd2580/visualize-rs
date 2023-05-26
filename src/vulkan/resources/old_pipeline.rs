impl<PushConstants> Pipeline<PushConstants> {


//
//     let write_descriptor_sets: Vec<WriteDescriptorSet> = image_infos_vec
//         .iter()
//         .zip(descriptor_sets.iter())
//         .map(|(image_infos, &descriptor_set)| {
//             let a = WriteDescriptorSet::builder()
//                 .descriptor_type(DescriptorType::STORAGE_IMAGE)
//                 .image_info(image_infos)
//                 .dst_set(descriptor_set)
//                 .dst_binding(0)
//                 .dst_array_element(0)
//                 .build();
//
//             return a;
//         })
//         .collect();
//
//     unsafe { device.update_descriptor_sets(&write_descriptor_sets, &[]) };
//

