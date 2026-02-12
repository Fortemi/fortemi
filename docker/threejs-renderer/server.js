/**
 * Lightweight 3D Model Renderer using Three.js
 *
 * Push-only design: Receives model data via multipart POST, renders multi-view images,
 * and returns all rendered images directly in a multipart response.
 * No download endpoints - all data returned in single request/response cycle.
 */

const express = require('express');
const multer = require('multer');
const path = require('path');
const fs = require('fs');
const os = require('os');

// Three.js imports
const THREE = require('three');

// Try to load headless-gl for server-side rendering
let gl;
try {
  gl = require('gl');
  console.log('Using headless-gl for server-side rendering');
} catch (e) {
  console.warn('headless-gl not available, will use software renderer');
  gl = null;
}

const app = express();
const upload = multer({ storage: multer.memoryStorage() });

// Import GLTFLoader - need to handle ES modules in CommonJS
let GLTFLoader;
let OBJLoader;
let STLLoader;

// Dynamic import for Three.js loaders (they're ES modules)
async function initLoaders() {
  const { GLTFLoader: GLTF } = await import('three/examples/jsm/loaders/GLTFLoader.js');
  GLTFLoader = GLTF;

  try {
    const { OBJLoader: OBJ } = await import('three/examples/jsm/loaders/OBJLoader.js');
    OBJLoader = OBJ;
  } catch (e) {
    console.warn('OBJLoader not available');
  }

  try {
    const { STLLoader: STL } = await import('three/examples/jsm/loaders/STLLoader.js');
    STLLoader = STL;
  } catch (e) {
    console.warn('STLLoader not available');
  }
}

/**
 * Create a WebGL context for headless rendering
 */
function createGLContext(width, height) {
  if (gl) {
    return gl(width, height, { preserveDrawingBuffer: true });
  }
  return null;
}

/**
 * Software renderer fallback using canvas
 */
function createSoftwareRenderer(width, height) {
  // Create a mock canvas for Three.js
  const { createCanvas } = require('canvas');
  const canvas = createCanvas(width, height);

  // Use software WebGL renderer
  const renderer = new THREE.WebGLRenderer({
    canvas,
    antialias: true,
    preserveDrawingBuffer: true,
    alpha: true
  });
  renderer.setSize(width, height);
  renderer.setClearColor(0x000000, 0);

  return { renderer, canvas };
}

/**
 * Create WebGL renderer with headless-gl
 */
function createHeadlessRenderer(width, height) {
  const context = createGLContext(width, height);
  if (!context) {
    throw new Error('Failed to create GL context');
  }

  // Create mock canvas
  const canvas = {
    width,
    height,
    style: {},
    addEventListener: () => {},
    removeEventListener: () => {},
    getContext: () => context
  };

  const renderer = new THREE.WebGLRenderer({
    canvas,
    context,
    antialias: true,
    preserveDrawingBuffer: true,
    alpha: true
  });
  renderer.setSize(width, height);
  renderer.setClearColor(0x000000, 0);

  return { renderer, context };
}

/**
 * Load 3D model from buffer based on file extension
 */
async function loadModel(buffer, filename) {
  const ext = path.extname(filename).toLowerCase();

  return new Promise((resolve, reject) => {
    if (ext === '.glb' || ext === '.gltf') {
      const loader = new GLTFLoader();
      const arrayBuffer = buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength);

      loader.parse(arrayBuffer, '', (gltf) => {
        resolve(gltf.scene);
      }, reject);
    } else if (ext === '.obj' && OBJLoader) {
      const loader = new OBJLoader();
      const text = buffer.toString('utf-8');
      try {
        const obj = loader.parse(text);
        resolve(obj);
      } catch (e) {
        reject(e);
      }
    } else if (ext === '.stl' && STLLoader) {
      const loader = new STLLoader();
      const arrayBuffer = buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength);
      try {
        const geometry = loader.parse(arrayBuffer);
        const material = new THREE.MeshStandardMaterial({ color: 0x888888 });
        const mesh = new THREE.Mesh(geometry, material);
        resolve(mesh);
      } catch (e) {
        reject(e);
      }
    } else {
      // Default to GLB
      const loader = new GLTFLoader();
      const arrayBuffer = buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength);

      loader.parse(arrayBuffer, '', (gltf) => {
        resolve(gltf.scene);
      }, reject);
    }
  });
}

/**
 * Calculate bounding box and optimal camera position
 */
function calculateCameraPosition(object) {
  const box = new THREE.Box3().setFromObject(object);
  const center = box.getCenter(new THREE.Vector3());
  const size = box.getSize(new THREE.Vector3());

  const maxDim = Math.max(size.x, size.y, size.z);
  const distance = maxDim * 2.5;

  return { center, size, distance, maxDim };
}

/**
 * Render scene from a specific camera angle
 */
function renderView(scene, camera, renderer, width, height) {
  renderer.render(scene, camera);

  // Get pixel data
  const gl = renderer.getContext();
  const pixels = new Uint8Array(width * height * 4);
  gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);

  // Flip vertically (WebGL has origin at bottom-left)
  const flipped = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const srcIdx = ((height - y - 1) * width + x) * 4;
      const dstIdx = (y * width + x) * 4;
      flipped[dstIdx] = pixels[srcIdx];
      flipped[dstIdx + 1] = pixels[srcIdx + 1];
      flipped[dstIdx + 2] = pixels[srcIdx + 2];
      flipped[dstIdx + 3] = pixels[srcIdx + 3];
    }
  }

  return flipped;
}

/**
 * Convert raw RGBA pixels to PNG buffer
 */
async function pixelsToPng(pixels, width, height) {
  const { PNG } = require('pngjs');

  const png = new PNG({ width, height });
  png.data = Buffer.from(pixels);

  return new Promise((resolve, reject) => {
    const chunks = [];
    png.pack()
      .on('data', chunk => chunks.push(chunk))
      .on('end', () => resolve(Buffer.concat(chunks)))
      .on('error', reject);
  });
}

/**
 * Health check endpoint
 */
app.get('/health', (req, res) => {
  res.json({
    status: 'healthy',
    renderer: gl ? 'headless-gl' : 'software',
    formats: ['glb', 'gltf', OBJLoader ? 'obj' : null, STLLoader ? 'stl' : null].filter(Boolean),
    version: require('./package.json').version
  });
});

/**
 * Render endpoint - push-only design
 *
 * Request: multipart/form-data with:
 *   model: binary model file
 *   filename: original filename (for extension detection)
 *   num_views: number of views to render (default: 6, max: 15)
 *
 * Response: multipart/mixed with all rendered PNG images
 */
app.post('/render', upload.single('model'), async (req, res) => {
  const startTime = Date.now();

  try {
    if (!req.file) {
      return res.status(400).json({ error: 'model file is required' });
    }

    const modelBuffer = req.file.buffer;
    if (!modelBuffer || modelBuffer.length === 0) {
      return res.status(400).json({ error: 'model file is empty' });
    }

    const filename = req.body.filename || req.file.originalname || 'model.glb';
    const numViews = Math.min(parseInt(req.body.num_views || '6', 10), 15);
    const width = 512;
    const height = 512;

    console.log(`Rendering ${filename} with ${numViews} views`);

    // Load the model
    const model = await loadModel(modelBuffer, filename);

    // Create scene
    const scene = new THREE.Scene();
    scene.add(model);

    // Add lighting
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.5);
    scene.add(ambientLight);

    const directionalLight = new THREE.DirectionalLight(0xffffff, 1.0);
    directionalLight.position.set(5, 10, 7.5);
    scene.add(directionalLight);

    const fillLight = new THREE.DirectionalLight(0xffffff, 0.3);
    fillLight.position.set(-5, 0, -5);
    scene.add(fillLight);

    // Calculate camera position
    const { center, distance } = calculateCameraPosition(model);

    // Create camera
    const camera = new THREE.PerspectiveCamera(50, width / height, 0.1, 1000);

    // Create renderer
    let renderer, context;
    try {
      const result = createHeadlessRenderer(width, height);
      renderer = result.renderer;
      context = result.context;
    } catch (e) {
      console.warn('Headless renderer failed, trying software:', e.message);
      // Software fallback would go here
      throw new Error('Rendering not available - headless-gl required');
    }

    // Render from multiple angles
    const views = [];

    for (let i = 0; i < numViews; i++) {
      const angle = (2 * Math.PI * i) / numViews;
      const elevation = (i % 2 === 0) ? Math.PI / 6 : Math.PI / 3; // 30° or 60°

      const x = center.x + distance * Math.cos(angle) * Math.cos(elevation);
      const y = center.y + distance * Math.sin(elevation);
      const z = center.z + distance * Math.sin(angle) * Math.cos(elevation);

      camera.position.set(x, y, z);
      camera.lookAt(center);

      const pixels = renderView(scene, camera, renderer, width, height);
      const pngBuffer = await pixelsToPng(pixels, width, height);

      views.push({
        index: i,
        angle_degrees: (360.0 / numViews) * i,
        elevation: i % 2 === 0 ? 'low_30deg' : 'high_60deg',
        data: pngBuffer
      });
    }

    // Cleanup
    renderer.dispose();
    if (context && context.getExtension) {
      const loseContext = context.getExtension('WEBGL_lose_context');
      if (loseContext) loseContext.loseContext();
    }

    // Build multipart response
    const boundary = '----ThreeJsRenderBoundary';
    const parts = [];

    for (const view of views) {
      const part = Buffer.concat([
        Buffer.from(
          `--${boundary}\r\n` +
          `Content-Type: image/png\r\n` +
          `Content-Disposition: attachment; ` +
          `filename="view_${String(view.index).padStart(3, '0')}.png"; ` +
          `index="${view.index}"; ` +
          `angle_degrees="${view.angle_degrees}"; ` +
          `elevation="${view.elevation}"\r\n` +
          `Content-Length: ${view.data.length}\r\n` +
          `\r\n`
        ),
        view.data,
        Buffer.from('\r\n')
      ]);
      parts.push(part);
    }

    // Final boundary
    parts.push(Buffer.from(`--${boundary}--\r\n`));

    const responseBody = Buffer.concat(parts);
    const duration = Date.now() - startTime;

    console.log(`Rendered ${numViews} views in ${duration}ms`);

    res.set({
      'Content-Type': `multipart/mixed; boundary=${boundary}`,
      'X-Render-Views': String(views.length),
      'X-Render-Success': 'true',
      'X-Render-Duration-Ms': String(duration)
    });

    res.send(responseBody);

  } catch (error) {
    console.error('Render error:', error);
    res.status(500).json({
      error: 'Rendering failed',
      message: error.message
    });
  }
});

// Initialize and start server
const PORT = parseInt(process.env.PORT || '8080', 10);

initLoaders().then(() => {
  app.listen(PORT, '0.0.0.0', () => {
    console.log(`Three.js renderer listening on port ${PORT}`);
    console.log(`Renderer: ${gl ? 'headless-gl' : 'software'}`);
  });
}).catch(err => {
  console.error('Failed to initialize loaders:', err);
  process.exit(1);
});
