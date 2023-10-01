import * as PIXI from "https://cdn.jsdelivr.net/npm/pixi.js@7.2.0-rc/+esm";

function connectToBackend(onmessage) {
  if (!("WebSocket" in window)) {
    alert("WebSocket is not supported by your Browser!");
  }

  const ws = new WebSocket("ws://localhost:9090");
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => {
    document.getElementById("overlay").classList.add("hide")
    document.getElementById("canvas").classList.remove("blur")
    ws.send("Hello, client here!");

  };
  ws.onmessage = onmessage;
  ws.onclose = () => {
    document.getElementById("canvas").classList.add("blur")
    document.getElementById("overlay").classList.remove("hide")
    setTimeout(() => connectToBackend(onmessage), 3000);
  };
}

function createCircleTexture(app) {
  // Create template shape.
  const templateShape = new PIXI.Graphics()
    .beginFill(0xffffff)
    .lineStyle({ width: 1, color: 0x333333, alignment: 0 })
    .drawCircle(0, 0, 5);

  // Create texture.
  const { width, height } = templateShape;
  const renderTexture = PIXI.RenderTexture.create({
    width,
    height,
    multisample: PIXI.MSAA_QUALITY.HIGH,
    resolution: window.devicePixelRatio
  });

  // Render to texture.
  // We need a translation matrix, otherwise we'll get a quarter of a circle.
  const transform = new PIXI.Matrix(1, 0, 0, 1, width / 2, height / 2);
  app.renderer.render(templateShape, { renderTexture, transform });

  // Required for MSAA, WebGL 2 only
  app.renderer.framebuffer.blit();

  // Drop graphics object.
  templateShape.destroy(true);

  return renderTexture;
}

class Avg {
  constructor(length) {
    this.len = length;
    this.data = new Array(length).fill(0);

    this.sum = 0;
    this.avg = 0;
    this.square_sum = 0;
    this.square_avg = 0;

    this.sd = 0;
  }

  sample(x) {
    const old_x = this.data.shift();
    this.data.push(x);

    this.sum += x - old_x;
    this.avg = this.sum / this.len;

    this.square_sum += Math.pow(x, 2) - Math.pow(old_x, 2);
    this.square_avg = this.square_sum / this.len;

    this.sd = (this.square_avg - Math.sqrt(Math.pow(this.avg, 2)));
  }
}


class BeatDetector {
  constructor() {
    this.short_avg = new Avg(60 / 5);
    this.medium_avg = new Avg(1 * 60);
    this.long_avg = new Avg(60 * 60);

    this.last_value = 0;

    this.noise_threshold_factor = 0.5;
    this.beat_threshold_factor = 1.1;

    this.last_beat_samples_ago = 0;
    this.last_beat_samples_threshold = 15;

    this.is_beat = false;
    this.was_high = false;

    this.spb = 60;
    this.spb_offset_avg = new Avg(15);
  }

  sample(x) {
    this.was_high = this.is_high;

    this.short_avg.sample(x);
    this.medium_avg.sample(x);
    this.long_avg.sample(x);
    this.last_value = x;

    this.last_beat_samples_ago++;
    this.is_beat = false;

    if (!this.was_high && this.is_high && this.last_beat_samples_ago > this.last_beat_samples_threshold) {
      this.update_bpm();
      this.last_beat_samples_ago = 0;
      this.is_beat = true;
    }
  }

  update_bpm() {
    const num_of_cycles = Math.round(this.last_beat_samples_ago / this.spb);
    const last_cycle_duration = this.last_beat_samples_ago - Math.max(0, (num_of_cycles - 1)) * this.spb;
    this.spb_offset_avg.sample(Math.abs(this.spb - last_cycle_duration));
    this.spb = 0.95 * this.spb + 0.05 * last_cycle_duration;
  }

  get bpm_confidence() {
    return 1 / Math.max(1, this.spb_offset_avg.avg);
  }

  get beat_confidence() {
    const num_of_cycles = Math.round(this.last_beat_samples_ago / this.spb);
    const distance_to_cycle = Math.abs(this.last_beat_samples_ago - num_of_cycles * this.spb);
    return this.bpm_confidence * (1 / Math.max(1, distance_to_cycle));
  }

  get is_high() {
    return this.is_not_noise && this.is_eligible && this.is_outlier;
  }

  get is_not_noise() {
    return this.last_value > this.long_avg.avg * this.noise_threshold_factor;
  }

  get is_eligible() {
    return this.short_avg.avg > this.medium_avg.avg * this.beat_threshold_factor;
  }

  get is_outlier() {
    return this.last_value > this.short_avg.avg;
  }

  get expecting_beat() {
    return this.last_beat_samples_ago == Math.trunc(this.spb);
  }
}

async function initializeGraphics() {
  const app = new PIXI.Application({ background: '#1099bb', resizeTo: window });
  document.body.appendChild(app.view).id = "canvas";

  const circleTexture = createCircleTexture(app);

  const dataContainer = new PIXI.Container();
  let dataOffset = 0;
  dataContainer.position.x = app.screen.width - dataOffset;
  app.stage.addChild(dataContainer);

  const gridContainer = new PIXI.Container();
  let numGraphs = 4;
  app.stage.addChild(gridContainer);


  let elapsed_frames = 0;
  let elapsed = 0;

  // Tick app (remove off-screen children, update delta).
  app.ticker.speed = 0.001 / PIXI.Ticker.targetFPMS;
  app.ticker.add((delta) => {
    while (dataContainer.children.length > 0 && dataContainer.children[0].position.x < dataOffset - app.screen.width) {
      dataContainer.removeChildAt(0);
    }

    elapsed_frames++;
    elapsed += delta;
  });

  const detectors = [new BeatDetector(), new BeatDetector(), new BeatDetector()];

  connectToBackend((message) => {
    dataOffset += 7;
    dataContainer.position.x = app.screen.width - dataOffset;

    const floats = new Float32Array(message.data);

    let count = floats.length;
    let slice_size = 1 / count;

    if (count != numGraphs) {
      while (gridContainer.children.length > 0) {
        let child = gridContainer.children[0];
        gridContainer.removeChild(child);
        child.destroy();
      }

      for (let i = 0; i < count; i++) {
        let graphics = new PIXI.Graphics();
        graphics.position.set(0, i * slice_size * app.screen.height);
        graphics.lineStyle(5, 0x000000);
        graphics.lineTo(app.screen.width, 0);
        gridContainer.addChild(graphics);
      }
    }


    const addPoint = (value, scale, tint) => {
      const shape = new PIXI.Sprite(circleTexture);
      shape.anchor.set(0.5);
      shape.scale.set(scale);
      shape.position.x = dataOffset;
      shape.position.y = app.screen.height * value;
      shape.tint = tint;
      dataContainer.addChild(shape);
    };

    let colors = [
      0xff0000,
      0x00ff00,
      0x0000ff,
      0xffff00,
      0xff00ff,
      0x00ffff,
      0x000000,
      0xffffff
    ];

    let scale = [1 / 1000, 1, 1];

    for (let i = 0; i < count; i++) {
      const v = floats[i];
      const d = detectors[i];
      d.sample(v);

      let base = 1 - i * slice_size;
      if (d.is_beat) {
        addPoint(base - slice_size * scale[i] * v, 2.0, colors[i]);
      }
      addPoint(base - slice_size * scale[i] * d.short_avg.avg, 0.7, colors[i + 1]);
      addPoint(base - slice_size * scale[i] * d.beat_threshold_factor * d.medium_avg.avg, 0.7, colors[i + 2]);
      addPoint(base - slice_size * scale[i] * d.noise_threshold_factor * d.long_avg.avg, 0.7, colors[i + 3]);

      // addPoint(base - slice_size * (d.last_beat_samples_ago / d.spb % 1), 0.5, colors[i + 4]);
      addPoint(base - slice_size * d.bpm_confidence, 0.5, colors[i + 4]);
      addPoint(base - slice_size * d.beat_confidence, 0.5, colors[i + 5]);
    }
  });
}

initializeGraphics();
