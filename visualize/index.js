import * as PIXI from "https://cdn.jsdelivr.net/npm/pixi.js@7.2.0-rc/+esm";

function connectToBackend(onmessage) {
  if (!("WebSocket" in window)) {
    alert("WebSocket is not supported by your Browser!");
  }

  const ws = new WebSocket("ws://localhost:9090");
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => ws.send("Hello, client here!");
  ws.onmessage = onmessage;
  ws.onclose = () => alert("Connection is closed...");
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




async function initializeGraphics() {
  const app = new PIXI.Application({ background: '#1099bb', resizeTo: window });

  document.body.appendChild(app.view);

  const circleTexture = createCircleTexture(app);

  const dataContainer = new PIXI.Container();
  let dataOffset = 0;
  dataContainer.position.x = app.screen.width - dataOffset;

  app.stage.addChild(dataContainer);

  let elapsed_frames = 0;
  let elapsed = 0;

  app.ticker.speed = 0.001 / PIXI.Ticker.targetFPMS;
  app.ticker.add((delta) => {
    const maxChildren = 100;

    // if (elapsed_frames % 60 === 0) {
    //   console.log(dataContainer.children.length);
    // }

    while (dataContainer.children.length > 0 && dataContainer.children[0].position.x < dataOffset - app.screen.width) {
      dataContainer.removeChildAt(0);
    }

    // const shape = new PIXI.Sprite(circleTexture);
    // shape.anchor.set(0.5);
    // shape.position.x = dataOffset;
    // shape.position.y = app.screen.height * (Math.sin(elapsed) + 1) / 2;
    // dataContainer.addChild(shape);

    elapsed_frames++;
    elapsed += delta;
  });

  connectToBackend((message) => {
    dataOffset += 5;
    dataContainer.position.x = app.screen.width - dataOffset;

    const floats = new Float32Array(message.data);

    const addPoint = (value, scale, tint) => {
      const shape = new PIXI.Sprite(circleTexture);
      shape.anchor.set(0.5);
      shape.scale.set(scale);
      shape.position.x = dataOffset;
      shape.position.y = app.screen.height * value;
      shape.tint = tint;
      dataContainer.addChild(shape);
    };

    addPoint(0.2 - 0.2 * 0.002 * floats[0], 1, "#FF0000");
    addPoint(0.4 - 0.2 * 0.002 * floats[1], 1, "#00FF00");
    addPoint(0.6 - 0.2 * 0.002 * floats[2], 1, "#0000FF");
    addPoint(0.8 - 0.2 * 0.002 * floats[3], 1, "#FFFFFF");
    const sum = floats[1] + floats[2];
    addPoint(1 - 0.2 * 0.002 * sum / 2, 1, "#000000");
  });
}

initializeGraphics();
