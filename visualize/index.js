import * as PIXI from "https://cdn.jsdelivr.net/npm/pixi.js@7.2.0-rc/+esm";

function connectToBackend(onmessage) {
  if (!("WebSocket" in window)) {
    alert("WebSocket is not supported by your Browser!");
  }

  const ws = new WebSocket("ws://localhost:9090");
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => ws.send("Hello, client here!");
  ws.onmessage = onmessage;
  ws.onclose = () => {
    // alert("Connection is closed...");
    setTimeout(() => connectToBackend(onmessage), 3000);
  }
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
    this.long_avg = new Avg(5 * 60);

    this.noise_threshold_factor = 1;
    this.beat_sigma_threshold_factor = 2.5;
  }

  sample(x) {
    this.short_avg.sample(x);
    this.long_avg.sample(x);
  }

  get is_not_noise() {
    const noise_threshold = this.long_avg.avg * this.noise_threshold_factor;
    return this.short_avg.avg > noise_threshold;
  }

  get is_outlier() {
    const beat_margin = this.beat_sigma_threshold_factor * this.short_avg.sd;
    const beat_threshold = this.short_avg.avg + beat_margin;
    return this.short_avg.data[this.short_avg.data.length - 1] > beat_threshold;
  }
}

async function initializeGraphics() {
  const app = new PIXI.Application({ background: '#1099bb', resizeTo: window });

  document.body.appendChild(app.view);

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
        graphics.lineStyle(1, 0xffffff);
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
      0xff00ff,
      0x0000ff,
      0x00ffff
    ];

    let scale = [1 / 1000, 1, 1];

    for (let i = 0; i < count; i++) {
      const v = floats[i];
      const d = detectors[i];
      d.sample(v);

      let s = (d.is_not_noise && d.is_outlier) ? 2.0 : 0.5;

      let base = 1 - i * slice_size;
      addPoint(base - slice_size * scale[i] * v, s, colors[i]);
      addPoint(base - slice_size * scale[i] * d.short_avg.avg, 0.5, colors[i + 1]);
    }
  });
}

initializeGraphics();
