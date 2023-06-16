# About

Rewrite of [oscilloscope-visualizer](https://github.com/alexd2580/oscilloscope-visualizer) (C/OpenGL) in Rust/Vulkan.

![current snapshot](./snapshot.png)

# Running

```bash
# It's rust....
cargo run -- -s shaders/high_low_dft.comp shaders/bloom_h.comp shaders/bloom_v.comp shaders/bloom_combine.comp
```

# Future development

* Audio input and processing
* Fourier transforms
* Visualizations
* Beat/bpm detection
* Mix and mash of different visualizations

# Current TODOs:

- [x] Sequence of multiple shaders
- [x] Compute norm of DFT once instead of inside shader
- [x] Different push-constants per shader
- [x] ~Separate descriptor sets per shader~
- [x] VK_KHR_push_descriptor
- [ ] ~Fix descriptor set allocation and binding~
- [x] Repopulate descriptors cache on shader rebuild, separate it as from `Descriptors`?
- [x] Bind images in different binding modes, e.g. sampler vs storage_image
- [x] Resize client images on resize. Static ~vs dynamic? Vulkan managed?~
- [x] Improve sequence of vulkan high-level operations: swapchain reinit, image reinit, etc...
- [ ] Exponentialize dft index on CPU side once?
- [ ] Run the app even without pipeline etc, when no shaders are working from the get-go.
- [ ] Better bloom? Using linear image samplers?
- [ ] use host_cached memory and flushes instead of _hoping_ that coherent writes work fine
