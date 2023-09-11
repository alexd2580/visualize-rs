import * as PIXI from "https://cdn.jsdelivr.net/npm/pixi.js@7.2.0-rc/+esm";


function createCircleTexture(app) {
  // Create template shape.
  const templateShape = new PIXI.Graphics()
    .beginFill(0xffffff)
    .lineStyle({ width: 1, color: 0x333333, alignment: 0 })
    .drawCircle(0, 0, 20);

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

    if (elapsed_frames % 60 === 0) {
      console.log(dataContainer.children.length);
    }

    while (dataContainer.children.length > 0 && dataContainer.children[0].position.x < dataOffset - app.screen.width) {
      console.log("before", dataContainer.children.length);
      dataContainer.removeChildAt(0);
      console.log("after", dataContainer.children.length);
    }

    const shape = new PIXI.Sprite(circleTexture);
    shape.anchor.set(0.5);
    shape.position.x = dataOffset;
    shape.position.y = app.screen.height * Math.random();
    dataContainer.addChild(shape);

    dataOffset += delta * 500;
    dataContainer.position.x = app.screen.width - dataOffset;

    elapsed_frames++;
    elapsed += delta;
  });
}

initializeGraphics();

function connectToBackend() {
  if (!("WebSocket" in window)) {
    alert("WebSocket is not supported by your Browser!");
  }

  const ws = new WebSocket("ws://localhost:9090");
  ws.onopen = () => {
    ws.send("Hello, client here!");
  };
  ws.onmessage = (evt) => {
    const { data } = evt;
    console.log(data);
  };
  ws.onclose = () => {
    alert("Connection is closed...");
  };
}

function oldThreeJs() {
  // import * as THREE from "https://cdn.jsdelivr.net/npm/three@0.156.1/+esm";
  const THREE = {};

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 1000);
  camera.position.z = 4;

  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setClearColor("#000000");
  renderer.setSize(window.innerWidth, window.innerHeight);

  document.body.appendChild(renderer.domElement);

  const geometry = new THREE.BoxGeometry(1, 1, 1);
  const material = new THREE.MeshBasicMaterial({ color: "#433F81" });
  const cube = new THREE.Mesh(geometry, material);

  scene.add(cube);

  function render() {
    requestAnimationFrame(render);

    cube.rotation.x += 0.01;
    cube.rotation.y += 0.01;

    renderer.render(scene, camera);
  };

  render();
}
