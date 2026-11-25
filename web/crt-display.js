// SPDX-License-Identifier: MIT
// Original presentation shader. Three horizontal taps approximate different
// luma and chroma bandwidths, with a source-line beam and output-pixel mask.
const vertexSource = `#version 300 es
out vec2 uv;
void main() {
  vec2 corner = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  gl_Position = vec4(2.0 * corner - 1.0, 0.0, 1.0);
  uv = vec2(corner.x, 1.0 - corner.y);
}`;

const fragmentSource = `#version 300 es
precision highp float;
uniform sampler2D source;
uniform vec4 preset;
uniform vec2 outputSize;
in vec2 uv;
out vec4 color;

void main() {
  vec2 size = vec2(textureSize(source, 0));
  vec2 scale = outputSize / size;
  float line = uv.y * size.y;
  vec2 point = vec2(uv.x, (floor(line) + 0.5) / size.y);
  // High horizontal resolution represents twice as many dots per scanline.
  vec2 tap = vec2((size.x >= 640.0 ? 2.0 : 1.0) / size.x, 0.0);
  vec3 center = texture(source, point).rgb;
  vec3 adjacent = 0.5 * (texture(source, point - tap).rgb + texture(source, point + tap).rgb);
  vec3 delta = adjacent - center;
  float luma = dot(delta, vec3(0.299, 0.587, 0.114));
  vec3 signal = clamp(center + preset.y * delta + (preset.x - preset.y) * luma, 0.0, 1.0);

  // Double-density interlace and 480-line modes already have closely spaced
  // rows. Suppress the beam modulation there and avoid any field-parity phase.
  float beam = size.y < 448.0 ? preset.z * smoothstep(1.75, 3.5, scale.y) : 0.0;
  signal *= 1.0 - beam * abs(2.0 * fract(line) - 1.0);
  if (scale.x > 3.0) {
    float strength = preset.w * smoothstep(3.0, 5.0, scale.x);
    float stripe = mod(floor(gl_FragCoord.x), 3.0);
    vec3 other = min(abs(vec3(0.0, 1.0, 2.0) - stripe), 1.0);
    signal *= 1.0 - strength * other;
  }
  color = vec4(signal, 1.0);
}`;

// Luma blend, chroma blend, maximum beam attenuation, maximum mask attenuation.
const presets = {
  composite: [0.44, 0.72, 0.13, 0.028],
  "s-video": [0.19, 0.30, 0.12, 0.030],
  rgb: [0.04, 0.04, 0.11, 0.044],
};

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("Shader allocation failed");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (gl.getShaderParameter(shader, gl.COMPILE_STATUS)) return shader;
  gl.deleteShader(shader);
  throw new Error("Shader compilation failed");
}

function createProgram(gl) {
  const program = gl.createProgram();
  if (!program) throw new Error("Program allocation failed");
  let vertex, fragment;
  try {
    vertex = compile(gl, gl.VERTEX_SHADER, vertexSource);
    fragment = compile(gl, gl.FRAGMENT_SHADER, fragmentSource);
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error("Shader link failed");
    gl.detachShader(program, vertex);
    gl.detachShader(program, fragment);
    return program;
  } catch (error) {
    gl.deleteProgram(program);
    throw error;
  } finally {
    if (vertex) gl.deleteShader(vertex);
    if (fragment) gl.deleteShader(fragment);
  }
}

export class CrtDisplay {
  constructor(screen, canvas, selector, status) {
    this.screen = screen;
    this.canvas = canvas;
    this.status = status;
    this.preset = selector.value = "clean";
    selector.addEventListener("change", () => {
      this.preset = selector.value;
      this.redraw();
    });
    canvas.addEventListener("webglcontextlost", event => {
      event.preventDefault();
      this.resources = null;
      this.uploaded = null;
      this.lost = true;
      this.redraw();
    });
    canvas.addEventListener("webglcontextrestored", () => {
      this.lost = this.failed = false;
      this.redraw();
    });
  }

  present(pixels, width, height) {
    this.pixels = pixels;
    this.width = width;
    this.height = height;
    if (this.preset !== "clean" && !this.failed && !this.lost) this.redraw();
  }

  clear() {
    this.pixels = this.uploaded = null;
    this.canvas.hidden = true;
  }

  initialize() {
    const gl = this.gl ??= this.canvas.getContext("webgl2", {
      alpha: false, antialias: false, depth: false, stencil: false,
    });
    if (!gl) throw new Error("WebGL 2 unavailable");
    const program = createProgram(gl);
    const texture = gl.createTexture();
    if (!texture) {
      gl.deleteProgram(program);
      throw new Error("Texture allocation failed");
    }
    gl.useProgram(program);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    // Sampler source defaults to the only bound texture unit (0).
    this.resources = {
      preset: gl.getUniformLocation(program, "preset"),
      outputSize: gl.getUniformLocation(program, "outputSize"),
      limit: gl.getParameter(gl.MAX_VIEWPORT_DIMS),
      width: 0, height: 0,
    };
    if (!this.observer) {
      this.observer = new ResizeObserver(() => this.resize());
      this.observer.observe(this.screen);
      this.watchPixelRatio();
    }
  }

  watchPixelRatio() {
    matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener("change", () => {
      this.watchPixelRatio();
      this.resize();
    }, { once: true });
  }

  resize() {
    if (!this.resources) return;
    this.measure();
    this.redraw();
  }

  measure() {
    const { width, height } = this.screen.getBoundingClientRect();
    const [maxWidth, maxHeight] = this.resources.limit;
    // Use physical pixels within the GPU's viewport limit and a 4K pixel budget.
    const ratio = Math.min(devicePixelRatio, maxWidth / width, maxHeight / height,
      Math.sqrt(3840 * 2160 / (width * height)));
    this.outputWidth = Math.max(1, Math.floor(width * ratio));
    this.outputHeight = Math.max(1, Math.floor(height * ratio));
  }

  redraw() {
    if (this.preset === "clean") {
      this.canvas.hidden = true;
      this.uploaded = null;
      this.status.textContent = "";
      return;
    }
    if (!this.failed && !this.lost) {
      try {
        if (!this.resources) this.initialize();
        if (this.pixels) {
          if (this.resources.width !== this.width || this.resources.height !== this.height) this.measure();
          this.draw();
        }
        if (this.canvas.hidden === Boolean(this.pixels)) this.canvas.hidden = !this.pixels;
        if (this.status.textContent) this.status.textContent = "";
        return;
      } catch {
        this.failed = true;
      }
    }
    this.canvas.hidden = true;
    this.status.textContent = this.lost
      ? "CRT display interrupted. Showing Clean while WebGL recovers."
      : "CRT display unavailable. Showing Clean.";
  }

  draw() {
    const { gl, canvas, resources, width, height, pixels, outputWidth, outputHeight } = this;
    if (resources.outputWidth !== outputWidth || resources.outputHeight !== outputHeight) {
      canvas.width = outputWidth;
      canvas.height = outputHeight;
      if (gl.drawingBufferWidth !== outputWidth || gl.drawingBufferHeight !== outputHeight) {
        throw new Error("Drawing buffer allocation failed");
      }
      gl.viewport(0, 0, outputWidth, outputHeight);
      gl.uniform2f(resources.outputSize, outputWidth, outputHeight);
      resources.outputWidth = outputWidth;
      resources.outputHeight = outputHeight;
    }
    if (resources.width !== width || resources.height !== height) {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      if (gl.getError() !== gl.NO_ERROR) throw new Error("Frame texture allocation failed");
      resources.width = width;
      resources.height = height;
    } else if (this.uploaded !== pixels) {
      gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    }
    this.uploaded = pixels;
    if (resources.activePreset !== this.preset) {
      gl.uniform4fv(resources.preset, presets[this.preset]);
      resources.activePreset = this.preset;
    }
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }
}
