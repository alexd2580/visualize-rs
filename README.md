# About

Rewrite of [oscilloscope-visualizer](https://github.com/alexd2580/oscilloscope-visualizer) (C/OpenGL) in Rust/Vulkan.

![current snapshot](./snapshot.png)

# Running

```bash
# It's rust....
cargo run -- -s shaders/high_low_dft.comp -d 8192
```

# Future development

* Audio input and processing
* Fourier transforms
* Visualizations
* Beat/bpm detection
* Mix and mash of different visualizations
