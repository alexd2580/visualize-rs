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
    .lineStyle({ width: 0, color: 0x333333, alignment: 0 })
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

async function initializeGraphics(floatsToSeries, plotSeries) {
  const app = new PIXI.Application({ background: '#1099bb', resizeTo: window });
  document.body.appendChild(app.view).id = "canvas";

  const circleTexture = createCircleTexture(app);

  const dataContainer = new PIXI.Container();
  let dataOffset = 0;
  dataContainer.position.x = app.screen.width - dataOffset;
  app.stage.addChild(dataContainer);

  const gridContainer = new PIXI.Container();
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

  let numGraphs = 1;

  connectToBackend((message) => {
    dataOffset += 0.5;
    dataContainer.position.x = app.screen.width - dataOffset;

    const floats = new Float32Array(message.data);
    const sets = floatsToSeries(floats);
    const count = sets.length
    const sliceSize = 1 / count;

    if (count != numGraphs) {
      // Delete all.
      while (gridContainer.children.length > 0) {
        let child = gridContainer.children[0];
        gridContainer.removeChild(child);
        child.destroy();
      }

      // Create new sub plots.
      for (let i = 0; i < count; i++) {
        let graphics = new PIXI.Graphics();
        graphics.position.set(0, i * sliceSize * app.screen.height);
        graphics.lineStyle(5, 0x000000);
        graphics.lineTo(app.screen.width, 0);
        gridContainer.addChild(graphics);
      }
    }

    const add = (plotIndex) => (v, scale, tint) => {
      let base = 1 - plotIndex * sliceSize;
      let value = base - sliceSize * v;

      const shape = new PIXI.Sprite(circleTexture);
      shape.anchor.set(0.5);
      shape.scale.set(scale);
      shape.position.x = dataOffset;
      shape.position.y = app.screen.height * value;
      shape.tint = tint;
      dataContainer.addChild(shape);
    };
    sets.forEach((set, index) => plotSeries(set, add(index)));
  });
}

const colors = [
  0xff0000,
  0x00ff00,
  0x0000ff,
  0xffff00,
  0xff00ff,
  0x00ffff,
  0x000000,
  0xffffff,
  0x799999
];

function plotSimple(value, add) {
  add(value, 1.0, colors[0]);
}

// initializeGraphics(floats => floats, plotSimple);

function floatsToEnergyStats(floats) {
  const results = [];
  for (let i = 0; i < floats.length; i += 6) {
    results.push({
      energy: floats[i + 0],
      short: floats[i + 1],
      long: floats[i + 2],
      is_beat: floats[i + 3] > 0.5,
      confidence: floats[i + 4],
      phase_error: floats[i + 5]
    });
  }
  return results;
}

const nmin = (a, b) => a < b ? a : b;
const nmax = (a, b) => a > b ? a : b;

function plotStats(stats, add) {
  add(0.5, 0.2, 0x000000);

  add(stats.energy, stats.is_beat ? 3.0 : 0.8, colors[0]);
  add(stats.short, 0.5, colors[1]);
  add(stats.long, 0.5, colors[2]);

  const min_long = nmin(stats.long, 0.4);
  const max_long = nmax(stats.long, 0.4);

  // add(min_long, 0.2, colors[3]);
  // add(max_long, 0.7, colors[3]);

  add(stats.confidence, stats.confidence, colors[3]);
  add(stats.phase_error, 0.5, colors[4]);
}

initializeGraphics(floatsToEnergyStats, plotStats);
