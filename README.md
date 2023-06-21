# About

Rewrite of [oscilloscope-visualizer](https://github.com/alexd2580/oscilloscope-visualizer) (C/OpenGL) in Rust/Vulkan.

![current snapshot](./snapshot.png)

# Running

```bash
# It's rust.... not the 'x', it's an alias.
# WARNING: This messes with your pulseaudio setup (`-p true`).
# If you don't want that then run the app manually.
# See `.cargo/config.toml` and `cargo run -- --help` for reference.
cargo runx
```

# Linting

```bash
# Is pedantic about stuff, but also disables some obnoxious lints.
cargo lint
```

# Pulseaudio setup

The app has two modes: "passthrough" (default) and "listen". In "listen" mode
you control the audio input yourself, e.g. In `pavucontrol` you have to set up
the app to record from the monitor of your audio device. The problem is that
the visualization is delayed oh so slightly. The delayed beat detection is
especially noticeable. To improve this you can go "passthrough" mode, which:

- Works only with pulseaudio (or pipewire), `pactl` is required
- Creates a virtual sink
- Sets the virtual sink as the default sink
- Sets the source of the visualizer to the monitor of the virtual sink
- Sets the sink of the visualizer to the previous default sink
- Resets the default sink to the previous default on app exit

# Future development

* Visualizations
* Mix and match of different visualizations
* Various "Improvements"
* Cleanup of codebase
* Web based stats page instead of logs

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
- [x] Smoothing of input data, less stutter
- [x] Beat detection
- [x] BPM analysis
- [x] Notice the importance of running a compositor when under X -_-
- [x] Audio passthrough with delay
- [x] Passthrough selector
- [x] Automatic pulse null-sink setup and audio routing and restore on exit
- [x] ~Check that glslc is present.... use native impl? does it exist?~ shaderc-rs! ~?~
- [ ] Check that pactl and pulsecmd are present before allowing "passthrough" mode
- [ ] Better delay control
- [ ] Revise BPM and beat detection.
- [ ] Better beat-effects. Check last_beat and next_beat
- [ ] Exponentialize dft index on CPU side once?
- [ ] use host_cached memory and flushes instead of _hoping_ that coherent writes work fine
- [ ] Run the app even without pipeline etc, when no shaders are working from the get-go.
- [ ] Better bloom? Using linear image samplers?
- [ ] mix and match
- [ ] Document the installation process and requirements
