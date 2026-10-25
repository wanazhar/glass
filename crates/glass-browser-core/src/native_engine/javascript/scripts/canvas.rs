pub(in crate::native_engine::javascript) const NATIVE_CANVAS_SCRIPT: &str = r###"
  const nativeCanvasByteLimit = 4 * 1024 * 1024;
  const nativeCanvasPixelLimit = 1024 * 1024;
  const nativeCanvasDimensionLimit = 4096;
  const nativeCanvasPixelArray = (pixels) => Array.isArray(pixels)
    ? pixels.slice()
    : ArrayBuffer.isView(pixels) ? Array.from(pixels) : [];
  const nativeCanvasSurfaces = globalThis.__glassCanvasSurfaces instanceof Map
    ? globalThis.__glassCanvasSurfaces
    : new Map();
  globalThis.__glassCanvasSurfaces = nativeCanvasSurfaces;
  const nativeCanvasRevision = Number(state.revision) || 0;
  if (globalThis.__glassCanvasRevision !== nativeCanvasRevision) {
    nativeCanvasSurfaces.clear();
    globalThis.__glassCanvasRevision = nativeCanvasRevision;
  }
  const liveElementSnapshot = Array.isArray(state.elements) ? state.elements : [];
  if (cachedCanvasLiveIndexSource !== liveElementSnapshot || !(cachedCanvasLiveIndexes instanceof Set)) {
    cachedCanvasLiveIndexSource = liveElementSnapshot;
    cachedCanvasLiveIndexes = new Set(
      liveElementSnapshot
        .filter((entry) => entry && String(entry.tagName || "").toLowerCase() === "canvas")
        .map((entry) => Number(entry.nodeIndex)),
    );
  }
  const nativeCanvasLiveIndexes = cachedCanvasLiveIndexes;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("canvas_phase_live_indexes");
  for (const nodeIndex of Array.from(nativeCanvasSurfaces.keys())) {
    if (!nativeCanvasLiveIndexes.has(Number(nodeIndex))) nativeCanvasSurfaces.delete(nodeIndex);
  }
  const canvasResourceSnapshot = Array.isArray(state.canvasResources) ? state.canvasResources : [];
  if (!(cachedCanvasResources instanceof Map)) cachedCanvasResources = new Map();
  if (cachedCanvasResourceSource !== canvasResourceSnapshot) {
    cachedCanvasResourceSource = canvasResourceSnapshot;
    cachedCanvasResources.clear();
    for (const resource of canvasResourceSnapshot) {
      if (!resource || !Number.isSafeInteger(Number(resource.node_index))) continue;
      const width = Number(resource.width);
      const height = Number(resource.height);
      if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height)
          || width < 1 || height < 1 || width > nativeCanvasDimensionLimit
          || height > nativeCanvasDimensionLimit || width * height > nativeCanvasPixelLimit) continue;
      try {
        const pixels = decodeBase64(String(resource.pixels_base64 || ""), nativeCanvasByteLimit, true);
        if (pixels.length === width * height * 4) {
          cachedCanvasResources.set(Number(resource.node_index), {
            width,
            height,
            pixels,
            originClean: resource.origin_clean !== false,
          });
        }
      } catch (_error) {}
    }
  }
  const nativeCanvasResources = cachedCanvasResources;
  const publicCanvasResources = globalThis.__glassCanvasResources instanceof Map
    ? globalThis.__glassCanvasResources
    : new Map();
  publicCanvasResources.clear();
  for (const [nodeIndex, resource] of nativeCanvasResources) {
    publicCanvasResources.set(nodeIndex, {
      get width() { return resource.width; },
      get height() { return resource.height; },
      get pixels() { return nativeCanvasPixelArray(resource.pixels); },
      get originClean() { return resource.originClean; },
    });
  }
  globalThis.__glassCanvasResources = publicCanvasResources;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("canvas_phase_canvas_resources");
  const imageResourceSnapshot = Array.isArray(state.imageResources) ? state.imageResources : [];
  if (!(cachedCanvasImageResources instanceof Map)) cachedCanvasImageResources = new Map();
  if (cachedCanvasImageResourceSource !== imageResourceSnapshot) {
    cachedCanvasImageResourceSource = imageResourceSnapshot;
    cachedCanvasImageResources.clear();
    for (const resource of imageResourceSnapshot) {
      if (!resource || !Number.isSafeInteger(Number(resource.nodeIndex))) continue;
      const width = Number(resource.width);
      const height = Number(resource.height);
      if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height)
          || width < 1 || height < 1 || width > nativeCanvasDimensionLimit
          || height > nativeCanvasDimensionLimit || width * height > nativeCanvasPixelLimit) continue;
      try {
        const expectedBytes = width * height * 4;
        const decodePixels = (encoded) => {
          const pixels = decodeBase64(encoded, nativeCanvasByteLimit, true);
          return pixels.length === expectedBytes ? pixels : new Uint8Array(0);
        };
        const source = String(resource.pixelsBase64 || "");
        const frames = [];
        for (const frame of Array.isArray(resource.frames) ? resource.frames : []) {
          const delayMs = Number(frame && frame.delayMs);
          if (!Number.isSafeInteger(delayMs) || delayMs < 1 || delayMs > 60000) continue;
          const frameSource = String(frame.pixelsBase64 || "");
          frames.push({
            delayMs,
            pixels: null,
            getPixels() {
              if (this.pixels === null) {
                try { this.pixels = decodePixels(frameSource); }
                catch (_error) { this.pixels = new Uint8Array(0); }
              }
              return this.pixels;
            },
          });
        }
        const animationElapsedMs = Number(resource.animationElapsedMs);
        cachedCanvasImageResources.set(Number(resource.nodeIndex), {
          width,
          height,
          pixels: null,
          getPixels() {
            if (this.pixels === null) {
              try { this.pixels = decodePixels(source); }
              catch (_error) { this.pixels = new Uint8Array(0); }
            }
            return this.pixels;
          },
          source: String(resource.source || ""),
          frames,
          loopCount: resource.loopCount !== null && resource.loopCount !== undefined
            && Number.isSafeInteger(Number(resource.loopCount)) && Number(resource.loopCount) >= 0
            ? Number(resource.loopCount)
            : null,
          animationOriginNowMs: Number.isSafeInteger(animationElapsedMs)
            ? Number(host.now_ms) - animationElapsedMs
            : null,
        });
      } catch (_error) {}
    }
  }
  const nativeCanvasImageResources = cachedCanvasImageResources;
  const publicCanvasImageResources = globalThis.__glassCanvasImageResources instanceof Map
    ? globalThis.__glassCanvasImageResources
    : new Map();
  publicCanvasImageResources.clear();
  for (const [nodeIndex, resource] of nativeCanvasImageResources) {
    publicCanvasImageResources.set(nodeIndex, {
      get width() { return resource.width; },
      get height() { return resource.height; },
      get pixels() { return nativeCanvasPixelArray(resource.getPixels()); },
      get source() { return resource.source; },
      get frames() {
        return resource.frames.map((frame) => ({ delayMs: frame.delayMs, pixels: nativeCanvasPixelArray(frame.getPixels()) }));
      },
      get loopCount() { return resource.loopCount; },
      get animationOriginNowMs() { return resource.animationOriginNowMs; },
    });
  }
  globalThis.__glassCanvasImageResources = publicCanvasImageResources;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("canvas_phase_image_resources");
  const nativeCanvasDimension = (entry, name, fallback) => {
    const value = Number.parseInt(String(entry && entry.attributes && entry.attributes[name] || ""), 10);
    return Number.isSafeInteger(value) && value > 0 && value <= nativeCanvasDimensionLimit
      ? value
      : fallback;
  };
  const nativeCanvasDimensions = (entry) => {
    let width = nativeCanvasDimension(entry, "width", 300);
    let height = nativeCanvasDimension(entry, "height", 150);
    if (width * height > nativeCanvasPixelLimit) {
      height = Math.max(1, Math.floor(nativeCanvasPixelLimit / width));
    }
    return { width, height };
  };
  const nativeCanvasBlankPixels = (width, height) => new Array(width * height * 4).fill(0);
  const nativeCanvasSurfaceForEntry = (entry) => {
    const nodeIndex = Number(entry.nodeIndex);
    const dimensions = nativeCanvasDimensions(entry);
    const existing = nativeCanvasSurfaces.get(nodeIndex);
    if (existing && existing.width === dimensions.width && existing.height === dimensions.height) {
      return existing;
    }
    const resource = nativeCanvasResources.get(nodeIndex);
    const pixels = resource && resource.width === dimensions.width && resource.height === dimensions.height
      ? nativeCanvasPixelArray(resource.pixels)
      : nativeCanvasBlankPixels(dimensions.width, dimensions.height);
    const surface = {
      width: dimensions.width,
      height: dimensions.height,
      pixels,
      originClean: resource ? resource.originClean !== false : true,
    };
    nativeCanvasSurfaces.set(nodeIndex, surface);
    return surface;
  };
  const nativeCanvasClamp = (value, minimum, maximum) => Math.max(minimum, Math.min(maximum, value));
  const nativeCanvasFinite = (value, fallback = 0) => {
    const numeric = Number(value);
    return Number.isFinite(numeric) ? numeric : fallback;
  };
  const nativeCanvasMatrixIdentity = () => [1, 0, 0, 1, 0, 0];
  const nativeCanvasMatrixMultiply = (left, right) => [
    left[0] * right[0] + left[2] * right[1],
    left[1] * right[0] + left[3] * right[1],
    left[0] * right[2] + left[2] * right[3],
    left[1] * right[2] + left[3] * right[3],
    left[0] * right[4] + left[2] * right[5] + left[4],
    left[1] * right[4] + left[3] * right[5] + left[5],
  ];
  const nativeCanvasMatrixPoint = (matrix, x, y) => ({
    x: matrix[0] * x + matrix[2] * y + matrix[4],
    y: matrix[1] * x + matrix[3] * y + matrix[5],
  });
  const nativeCanvasMatrixInverse = (matrix) => {
    const determinant = matrix[0] * matrix[3] - matrix[1] * matrix[2];
    if (!Number.isFinite(determinant) || Math.abs(determinant) < 1e-12) return null;
    const inverse = 1 / determinant;
    return [
      matrix[3] * inverse,
      -matrix[1] * inverse,
      -matrix[2] * inverse,
      matrix[0] * inverse,
      (matrix[2] * matrix[5] - matrix[3] * matrix[4]) * inverse,
      (matrix[1] * matrix[4] - matrix[0] * matrix[5]) * inverse,
    ];
  };
  const nativeDomMatrixValues = (value) => {
    if (value && Array.isArray(value.__glassMatrix)) return value.__glassMatrix.slice();
    if (value && typeof value !== "string" && Number(value.length) === 6) {
      const values = Array.from(value).map(Number);
      if (values.every(Number.isFinite)) return values;
    }
    if (value && typeof value === "object") {
      const values = [value.a, value.b, value.c, value.d, value.e, value.f].map(Number);
      if (values.every(Number.isFinite)) return values;
    }
    throw new TypeError("DOMMatrix requires six finite 2D values");
  };
  const nativeDomMatrixJson = (matrix) => ({
    a: matrix[0], b: matrix[1], c: matrix[2], d: matrix[3], e: matrix[4], f: matrix[5],
    m11: matrix[0], m12: matrix[1], m21: matrix[2], m22: matrix[3], m41: matrix[4], m42: matrix[5],
    is2D: true,
    isIdentity: matrix[0] === 1 && matrix[1] === 0 && matrix[2] === 0
      && matrix[3] === 1 && matrix[4] === 0 && matrix[5] === 0,
  });
  const DOMPointReadOnlyNative = globalThis.__glassDOMPointReadOnlyConstructor || function DOMPointReadOnly(x = 0, y = 0, z = 0, w = 1) {
    if (!new.target) throw new TypeError("DOMPointReadOnly requires new");
    const values = [x, y, z, w].map(Number);
    if (!values.every(Number.isFinite)) throw new TypeError("DOMPoint values must be finite");
    Object.defineProperties(this, {
      x: { configurable: false, enumerable: true, value: values[0] },
      y: { configurable: false, enumerable: true, value: values[1] },
      z: { configurable: false, enumerable: true, value: values[2] },
      w: { configurable: false, enumerable: true, value: values[3] },
    });
  };
  const DOMPointNative = globalThis.__glassDOMPointConstructor || function DOMPoint(x = 0, y = 0, z = 0, w = 1) {
    if (!new.target) throw new TypeError("DOMPoint requires new");
    const values = [x, y, z, w].map(Number);
    if (!values.every(Number.isFinite)) throw new TypeError("DOMPoint values must be finite");
    Object.defineProperties(this, {
      x: { configurable: true, enumerable: true, writable: true, value: values[0] },
      y: { configurable: true, enumerable: true, writable: true, value: values[1] },
      z: { configurable: true, enumerable: true, writable: true, value: values[2] },
      w: { configurable: true, enumerable: true, writable: true, value: values[3] },
    });
  };
  if (Object.getPrototypeOf(DOMPointNative.prototype) !== DOMPointReadOnlyNative.prototype) {
    try { Object.setPrototypeOf(DOMPointNative.prototype, DOMPointReadOnlyNative.prototype); } catch (_error) {}
  }
  const nativeDomPointTransform = (point, matrix) => new DOMPointNative(
    matrix[0] * point.x + matrix[2] * point.y + matrix[4],
    matrix[1] * point.x + matrix[3] * point.y + matrix[5],
    point.z,
    point.w,
  );
  DOMPointReadOnlyNative.prototype.matrixTransform = function(matrix) {
    return nativeDomPointTransform(this, nativeDomMatrixValues(matrix));
  };
  DOMPointNative.prototype.matrixTransform = DOMPointReadOnlyNative.prototype.matrixTransform;
  globalThis.__glassDOMPointReadOnlyConstructor = DOMPointReadOnlyNative;
  globalThis.__glassDOMPointConstructor = DOMPointNative;
  globalThis.DOMPointReadOnly = DOMPointReadOnlyNative;
  globalThis.DOMPoint = DOMPointNative;
  const DOMMatrixReadOnlyNative = globalThis.__glassDOMMatrixReadOnlyConstructor || function DOMMatrixReadOnly(init) {
    if (!new.target) throw new TypeError("DOMMatrixReadOnly requires new");
    Object.defineProperty(this, "__glassMatrix", {
      configurable: false,
      enumerable: false,
      value: init === undefined ? nativeCanvasMatrixIdentity() : nativeDomMatrixValues(init),
    });
  };
  const DOMMatrixNative = globalThis.__glassDOMMatrixConstructor || function DOMMatrix(init) {
    if (!new.target) throw new TypeError("DOMMatrix requires new");
    Object.defineProperty(this, "__glassMatrix", {
      configurable: false,
      enumerable: false,
      writable: true,
      value: init === undefined ? nativeCanvasMatrixIdentity() : nativeDomMatrixValues(init),
    });
  };
  if (Object.getPrototypeOf(DOMMatrixNative.prototype) !== DOMMatrixReadOnlyNative.prototype) {
    try { Object.setPrototypeOf(DOMMatrixNative.prototype, DOMMatrixReadOnlyNative.prototype); } catch (_error) {}
  }
  const nativeDomMatrixProperty = (name, index) => ({
    configurable: true,
    enumerable: true,
    get() { return this.__glassMatrix[index]; },
  });
  Object.defineProperties(DOMMatrixReadOnlyNative.prototype, {
    a: nativeDomMatrixProperty("a", 0), b: nativeDomMatrixProperty("b", 1),
    c: nativeDomMatrixProperty("c", 2), d: nativeDomMatrixProperty("d", 3),
    e: nativeDomMatrixProperty("e", 4), f: nativeDomMatrixProperty("f", 5),
    m11: nativeDomMatrixProperty("m11", 0), m12: nativeDomMatrixProperty("m12", 1),
    m21: nativeDomMatrixProperty("m21", 2), m22: nativeDomMatrixProperty("m22", 3),
    m41: nativeDomMatrixProperty("m41", 4), m42: nativeDomMatrixProperty("m42", 5),
    is2D: { configurable: true, enumerable: true, get() { return true; } },
    isIdentity: { configurable: true, enumerable: true, get() { return nativeDomMatrixJson(this.__glassMatrix).isIdentity; } },
    toJSON: { configurable: true, value() { return nativeDomMatrixJson(this.__glassMatrix); } },
    toString: { configurable: true, value() { return "matrix(" + this.a + ", " + this.b + ", " + this.c + ", " + this.d + ", " + this.e + ", " + this.f + ")"; } },
    toFloat32Array: { configurable: true, value() { return new Float32Array(this.__glassMatrix); } },
    toFloat64Array: { configurable: true, value() { return new Float64Array(this.__glassMatrix); } },
    multiply: { configurable: true, value(other) { return new DOMMatrixReadOnlyNative(nativeCanvasMatrixMultiply(this.__glassMatrix, nativeDomMatrixValues(other))); } },
    translate: { configurable: true, value(x = 0, y = 0) { return new DOMMatrixReadOnlyNative(nativeCanvasMatrixMultiply(this.__glassMatrix, [1, 0, 0, 1, Number(x), Number(y)])); } },
    scale: { configurable: true, value(x, y = x, originX = 0, originY = 0) { const first = [1, 0, 0, 1, Number(originX), Number(originY)]; const scaling = [Number(x), 0, 0, Number(y), 0, 0]; const last = [1, 0, 0, 1, -Number(originX), -Number(originY)]; return new DOMMatrixReadOnlyNative(nativeCanvasMatrixMultiply(nativeCanvasMatrixMultiply(nativeCanvasMatrixMultiply(this.__glassMatrix, first), scaling), last)); } },
    rotate: { configurable: true, value(angle = 0, originX = 0, originY = 0) { const radians = Number(angle) * Math.PI / 180; const first = [1, 0, 0, 1, Number(originX), Number(originY)]; const rotation = [Math.cos(radians), Math.sin(radians), -Math.sin(radians), Math.cos(radians), 0, 0]; const last = [1, 0, 0, 1, -Number(originX), -Number(originY)]; return new DOMMatrixReadOnlyNative(nativeCanvasMatrixMultiply(nativeCanvasMatrixMultiply(nativeCanvasMatrixMultiply(this.__glassMatrix, first), rotation), last)); } },
    skewX: { configurable: true, value(angle = 0) { return new DOMMatrixReadOnlyNative(nativeCanvasMatrixMultiply(this.__glassMatrix, [1, 0, Math.tan(Number(angle) * Math.PI / 180), 1, 0, 0])); } },
    skewY: { configurable: true, value(angle = 0) { return new DOMMatrixReadOnlyNative(nativeCanvasMatrixMultiply(this.__glassMatrix, [1, Math.tan(Number(angle) * Math.PI / 180), 0, 1, 0, 0])); } },
    inverse: { configurable: true, value() { const inverse = nativeCanvasMatrixInverse(this.__glassMatrix); if (!inverse) throw nativeCanvasError("matrix is not invertible", "InvalidStateError"); return new DOMMatrixReadOnlyNative(inverse); } },
    transformPoint: { configurable: true, value(point = {}) { return nativeDomPointTransform(new DOMPointReadOnlyNative(point.x, point.y, point.z, point.w), this.__glassMatrix); } },
  });
  Object.defineProperties(DOMMatrixNative.prototype, {
    multiplySelf: { configurable: true, value(other) { this.__glassMatrix = nativeCanvasMatrixMultiply(this.__glassMatrix, nativeDomMatrixValues(other)); return this; } },
    preMultiplySelf: { configurable: true, value(other) { this.__glassMatrix = nativeCanvasMatrixMultiply(nativeDomMatrixValues(other), this.__glassMatrix); return this; } },
    translateSelf: { configurable: true, value(x = 0, y = 0) { this.__glassMatrix = nativeCanvasMatrixMultiply(this.__glassMatrix, [1, 0, 0, 1, Number(x), Number(y)]); return this; } },
    scaleSelf: { configurable: true, value(x, y = x, originX = 0, originY = 0) { this.__glassMatrix = this.scale(x, y, originX, originY).__glassMatrix.slice(); return this; } },
    rotateSelf: { configurable: true, value(angle = 0, originX = 0, originY = 0) { this.__glassMatrix = this.rotate(angle, originX, originY).__glassMatrix.slice(); return this; } },
    skewXSelf: { configurable: true, value(angle = 0) { this.__glassMatrix = this.skewX(angle).__glassMatrix.slice(); return this; } },
    skewYSelf: { configurable: true, value(angle = 0) { this.__glassMatrix = this.skewY(angle).__glassMatrix.slice(); return this; } },
    invertSelf: { configurable: true, value() { const inverse = nativeCanvasMatrixInverse(this.__glassMatrix); if (!inverse) throw nativeCanvasError("matrix is not invertible", "InvalidStateError"); this.__glassMatrix = inverse; return this; } },
  });
  globalThis.__glassDOMMatrixReadOnlyConstructor = DOMMatrixReadOnlyNative;
  globalThis.__glassDOMMatrixConstructor = DOMMatrixNative;
  globalThis.DOMMatrixReadOnly = DOMMatrixReadOnlyNative;
  globalThis.DOMMatrix = DOMMatrixNative;
  const nativeCanvasTransformObject = (matrix) => new DOMMatrixReadOnlyNative(matrix);
  const nativeCanvasNamedColors = {
    transparent: [0, 0, 0, 0], black: [0, 0, 0, 255], white: [255, 255, 255, 255],
    red: [255, 0, 0, 255], green: [0, 128, 0, 255], blue: [0, 0, 255, 255],
    yellow: [255, 255, 0, 255], cyan: [0, 255, 255, 255], aqua: [0, 255, 255, 255],
    magenta: [255, 0, 255, 255], fuchsia: [255, 0, 255, 255], gray: [128, 128, 128, 255],
    grey: [128, 128, 128, 255], orange: [255, 165, 0, 255], purple: [128, 0, 128, 255],
    pink: [255, 192, 203, 255], brown: [165, 42, 42, 255], lime: [0, 255, 0, 255],
    navy: [0, 0, 128, 255], teal: [0, 128, 128, 255], maroon: [128, 0, 0, 255],
  };
  const nativeCanvasColorChannel = (value) => {
    const text = String(value).trim();
    if (text.endsWith("%")) return nativeCanvasClamp(Number(text.slice(0, -1)) * 2.55, 0, 255);
    const numeric = Number(text);
    return Number.isFinite(numeric) ? nativeCanvasClamp(numeric, 0, 255) : NaN;
  };
  const nativeCanvasAlphaChannel = (value) => {
    const text = String(value).trim();
    const numeric = text.endsWith("%") ? Number(text.slice(0, -1)) / 100 : Number(text);
    return Math.round(nativeCanvasClamp(numeric, 0, 1) * 255);
  };
  const nativeCanvasParseColor = (value) => {
    if (value && value.__glassCanvasGradient === true) return value;
    if (typeof value !== "string") return null;
    const text = value.trim().toLowerCase();
    if (Object.prototype.hasOwnProperty.call(nativeCanvasNamedColors, text)) {
      return nativeCanvasNamedColors[text].slice();
    }
    if (/^#[0-9a-f]{3,8}$/i.test(text)) {
      const digits = text.slice(1);
      if (digits.length === 3 || digits.length === 4) {
        return digits.split("").map((digit) => parseInt(digit + digit, 16)).concat(digits.length === 3 ? 255 : []).slice(0, 4);
      }
      return [
        parseInt(digits.slice(0, 2), 16), parseInt(digits.slice(2, 4), 16),
        parseInt(digits.slice(4, 6), 16), digits.length === 8 ? parseInt(digits.slice(6, 8), 16) : 255,
      ];
    }
    const match = text.match(/^rgba?\((.*)\)$/);
    if (!match) return null;
    const parts = match[1].trim().replace(/\s*\/\s*/, ",").split(/[\s,]+/).filter(Boolean);
    if (parts.length < 3 || parts.length > 4) return null;
    const channels = [nativeCanvasColorChannel(parts[0]), nativeCanvasColorChannel(parts[1]), nativeCanvasColorChannel(parts[2])];
    channels.push(parts.length === 4 ? nativeCanvasAlphaChannel(parts[3]) : 255);
    if (channels.some((channel) => !Number.isFinite(channel))) return null;
    return channels;
  };
  const nativeCanvasGradientColor = (gradient, x, y) => {
    if (!gradient.stops.length) return [0, 0, 0, 255];
    let offset = 0;
    if (gradient.kind === "linear") {
      const dx = gradient.x1 - gradient.x0;
      const dy = gradient.y1 - gradient.y0;
      const denominator = dx * dx + dy * dy;
      offset = denominator === 0 ? 0 : ((x - gradient.x0) * dx + (y - gradient.y0) * dy) / denominator;
    } else {
      const dx = x - gradient.x0;
      const dy = y - gradient.y0;
      const distance = Math.sqrt(dx * dx + dy * dy);
      offset = (distance - gradient.r0) / (gradient.r1 - gradient.r0 || 1);
    }
    offset = nativeCanvasClamp(offset, 0, 1);
    let before = gradient.stops[0];
    let after = gradient.stops[gradient.stops.length - 1];
    for (let index = 1; index < gradient.stops.length; index += 1) {
      if (gradient.stops[index].offset >= offset) {
        after = gradient.stops[index];
        before = gradient.stops[index - 1];
        break;
      }
    }
    const span = after.offset - before.offset;
    const amount = span <= 0 ? 0 : (offset - before.offset) / span;
    return before.color.map((value, index) => Math.round(value + (after.color[index] - value) * amount));
  };
  const nativeCanvasResolvedColor = (style, x, y) => style && style.__glassCanvasGradient === true
    ? nativeCanvasGradientColor(style, x, y)
    : nativeCanvasParseColor(style) || [0, 0, 0, 255];
  const nativeCanvasBlendPixel = (surface, x, y, color, alpha, operation, clipRegions = []) => {
    if (x < 0 || y < 0 || x >= surface.width || y >= surface.height) return;
    if (!nativeCanvasClipAllows(clipRegions, x + 0.5, y + 0.5)) return;
    const index = (y * surface.width + x) * 4;
    if (operation === "clear") {
      surface.pixels[index] = 0;
      surface.pixels[index + 1] = 0;
      surface.pixels[index + 2] = 0;
      surface.pixels[index + 3] = 0;
      return;
    }
    const sourceAlpha = nativeCanvasClamp((color[3] / 255) * alpha, 0, 1);
    if (sourceAlpha <= 0) return;
    const destinationAlpha = surface.pixels[index + 3] / 255;
    if (operation === "copy") {
      surface.pixels[index] = Math.round(color[0]);
      surface.pixels[index + 1] = Math.round(color[1]);
      surface.pixels[index + 2] = Math.round(color[2]);
      surface.pixels[index + 3] = Math.round(sourceAlpha * 255);
      return;
    }
    if (operation === "destination-over" && destinationAlpha > 0) {
      const destinationFactor = 1 - destinationAlpha;
      surface.pixels[index] = Math.round(surface.pixels[index] + color[0] * sourceAlpha * destinationFactor);
      surface.pixels[index + 1] = Math.round(surface.pixels[index + 1] + color[1] * sourceAlpha * destinationFactor);
      surface.pixels[index + 2] = Math.round(surface.pixels[index + 2] + color[2] * sourceAlpha * destinationFactor);
      surface.pixels[index + 3] = Math.round((destinationAlpha + sourceAlpha * destinationFactor) * 255);
      return;
    }
    if (operation === "lighter") {
      surface.pixels[index] = nativeCanvasClamp(Math.round(surface.pixels[index] + color[0] * sourceAlpha), 0, 255);
      surface.pixels[index + 1] = nativeCanvasClamp(Math.round(surface.pixels[index + 1] + color[1] * sourceAlpha), 0, 255);
      surface.pixels[index + 2] = nativeCanvasClamp(Math.round(surface.pixels[index + 2] + color[2] * sourceAlpha), 0, 255);
      surface.pixels[index + 3] = nativeCanvasClamp(Math.round((destinationAlpha + sourceAlpha) * 255), 0, 255);
      return;
    }
    const outputAlpha = sourceAlpha + destinationAlpha * (1 - sourceAlpha);
    if (outputAlpha <= 0) return;
    surface.pixels[index] = Math.round((color[0] * sourceAlpha + surface.pixels[index] * destinationAlpha * (1 - sourceAlpha)) / outputAlpha);
    surface.pixels[index + 1] = Math.round((color[1] * sourceAlpha + surface.pixels[index + 1] * destinationAlpha * (1 - sourceAlpha)) / outputAlpha);
    surface.pixels[index + 2] = Math.round((color[2] * sourceAlpha + surface.pixels[index + 2] * destinationAlpha * (1 - sourceAlpha)) / outputAlpha);
    surface.pixels[index + 3] = Math.round(outputAlpha * 255);
  };
  const nativeCanvasPointInPolygon = (point, points) => {
    let inside = false;
    for (let left = 0, right = points.length - 1; left < points.length; right = left++) {
      const first = points[left];
      const second = points[right];
      if ((first.y > point.y) !== (second.y > point.y)
          && point.x < (second.x - first.x) * (point.y - first.y) / (second.y - first.y) + first.x) inside = !inside;
    }
    return inside;
  };
  const nativeCanvasPointInPaths = (point, paths, fillRule = "nonzero") => {
    const usable = paths.filter((path) => Array.isArray(path.points) && path.points.length >= 3);
    if (fillRule === "evenodd") {
      return usable.reduce((inside, path) => inside !== nativeCanvasPointInPolygon(point, path.points), false);
    }
    return usable.some((path) => nativeCanvasPointInPolygon(point, path.points));
  };
  const nativeCanvasClipAllows = (clipRegions, x, y) => {
    if (!Array.isArray(clipRegions) || clipRegions.length === 0) return true;
    const point = { x, y };
    return clipRegions.every((region) => nativeCanvasPointInPaths(point, region.paths, region.fillRule));
  };
  const nativeCanvasFillPolygon = (surface, points, style, alpha, operation, clipRegions = []) => {
    if (points.length < 3) return;
    const left = Math.max(0, Math.floor(Math.min(...points.map((point) => point.x))));
    const right = Math.min(surface.width - 1, Math.ceil(Math.max(...points.map((point) => point.x))));
    const top = Math.max(0, Math.floor(Math.min(...points.map((point) => point.y))));
    const bottom = Math.min(surface.height - 1, Math.ceil(Math.max(...points.map((point) => point.y))));
    for (let y = top; y <= bottom; y += 1) for (let x = left; x <= right; x += 1) {
      if (nativeCanvasPointInPolygon({ x: x + 0.5, y: y + 0.5 }, points)) {
        nativeCanvasBlendPixel(surface, x, y, nativeCanvasResolvedColor(style, x + 0.5, y + 0.5), alpha, operation, clipRegions);
      }
    }
  };
  const nativeCanvasFillPaths = (surface, paths, style, alpha, operation, clipRegions = [], fillRule = "nonzero") => {
    const points = paths.flatMap((path) => path.points || []);
    if (points.length < 3) return;
    const left = Math.max(0, Math.floor(Math.min(...points.map((point) => point.x))));
    const right = Math.min(surface.width - 1, Math.ceil(Math.max(...points.map((point) => point.x))));
    const top = Math.max(0, Math.floor(Math.min(...points.map((point) => point.y))));
    const bottom = Math.min(surface.height - 1, Math.ceil(Math.max(...points.map((point) => point.y))));
    for (let y = top; y <= bottom; y += 1) for (let x = left; x <= right; x += 1) {
      if (nativeCanvasPointInPaths({ x: x + 0.5, y: y + 0.5 }, paths, fillRule)) {
        nativeCanvasBlendPixel(surface, x, y, nativeCanvasResolvedColor(style, x + 0.5, y + 0.5), alpha, operation, clipRegions);
      }
    }
  };
  const nativeCanvasDistanceToSegment = (point, first, second) => {
    const dx = second.x - first.x;
    const dy = second.y - first.y;
    const length = dx * dx + dy * dy;
    const amount = length === 0 ? 0 : nativeCanvasClamp(((point.x - first.x) * dx + (point.y - first.y) * dy) / length, 0, 1);
    const x = first.x + amount * dx;
    const y = first.y + amount * dy;
    return Math.hypot(point.x - x, point.y - y);
  };
  const nativeCanvasStrokePath = (surface, paths, style, alpha, operation, lineWidth, clipRegions = []) => {
    const radius = Math.max(0.5, lineWidth / 2);
    for (const path of paths) {
      for (let index = 1; index < path.points.length; index += 1) {
        const first = path.points[index - 1];
        const second = path.points[index];
        const left = Math.max(0, Math.floor(Math.min(first.x, second.x) - radius - 1));
        const right = Math.min(surface.width - 1, Math.ceil(Math.max(first.x, second.x) + radius + 1));
        const top = Math.max(0, Math.floor(Math.min(first.y, second.y) - radius - 1));
        const bottom = Math.min(surface.height - 1, Math.ceil(Math.max(first.y, second.y) + radius + 1));
        for (let y = top; y <= bottom; y += 1) for (let x = left; x <= right; x += 1) {
          if (nativeCanvasDistanceToSegment({ x: x + 0.5, y: y + 0.5 }, first, second) <= radius) {
            nativeCanvasBlendPixel(surface, x, y, nativeCanvasResolvedColor(style, x + 0.5, y + 0.5), alpha, operation, clipRegions);
          }
        }
      }
    }
  };
  const nativeCanvasCommit = (canvas, surface) => {
    const nodeIndex = canvas && Number.isSafeInteger(canvas.nodeIndex) ? canvas.nodeIndex : null;
    if (nodeIndex === null) return;
    const encoded = encodeBase64(surface.pixels, nativeCanvasByteLimit);
    const commandsTarget = activeCommands();
    for (let index = commandsTarget.length - 1; index >= 0; index -= 1) {
      const command = commandsTarget[index];
      if (command && command.kind === "canvasCommit" && Number(command.node_index) === nodeIndex) {
        command.width = surface.width;
        command.height = surface.height;
        command.pixels_base64 = encoded;
        command.origin_clean = surface.originClean !== false;
        return;
      }
    }
    pushCommand({ kind: "canvasCommit", node_index: nodeIndex, width: surface.width, height: surface.height, pixels_base64: encoded, origin_clean: surface.originClean !== false });
  };
  const nativeCanvasError = (message, name = "IndexSizeError") => {
    const Constructor = globalThis.DOMException;
    if (typeof Constructor === "function") return new Constructor(message, name);
    const error = new Error(message);
    error.name = name;
    return error;
  };
  const nativeCanvasRequireOriginClean = (surface) => {
    if (surface.originClean === false) throw nativeCanvasError("canvas is not origin-clean", "SecurityError");
  };
  const nativeCanvasNumber = (value, fallback = 0) => {
    const numeric = Number(value);
    return Number.isFinite(numeric) ? numeric : fallback;
  };
  const nativeCanvasDimensionSetter = (value) => {
    const numeric = Number(value);
    if (!Number.isFinite(numeric) || Math.trunc(numeric) !== numeric || numeric < 0 || numeric > nativeCanvasDimensionLimit) {
      throw nativeCanvasError("canvas dimension is outside the native limit", "IndexSizeError");
    }
    return numeric === 0 ? 300 : numeric;
  };
  const nativeCanvasState = () => ({
    fillStyle: "#000000", strokeStyle: "#000000", globalAlpha: 1,
    globalCompositeOperation: "source-over", lineWidth: 1, lineCap: "butt", lineJoin: "miter",
    miterLimit: 10, font: "10px sans-serif", textAlign: "start", textBaseline: "alphabetic",
    direction: "inherit", transform: nativeCanvasMatrixIdentity(), lineDash: [], lineDashOffset: 0,
    path: [], clipRegions: [], stack: [],
  });
  const nativeCanvasPathPoint = (context, x, y) => nativeCanvasMatrixPoint(context.__glassCanvasState.transform, x, y);
  const nativeCanvasEnsurePath = (context) => {
    if (!context.__glassCanvasState.path) context.__glassCanvasState.path = [];
    return context.__glassCanvasState.path;
  };
  const nativeCanvasCurrentPath = (context) => {
    const path = nativeCanvasEnsurePath(context);
    if (!path.length) path.push({ points: [], closed: false });
    return path[path.length - 1];
  };
  const nativeCanvasAddPathPoint = (context, point) => {
    const path = nativeCanvasCurrentPath(context);
    if (path.points.length === 0 || path.points[path.points.length - 1].x !== point.x || path.points[path.points.length - 1].y !== point.y) path.points.push(point);
  };
  const nativeCanvasPathCoordinate = (value) => {
    const numeric = Number(value);
    if (!Number.isFinite(numeric)) throw new TypeError("path coordinates must be finite");
    return numeric;
  };
  const nativeCanvasPathClone = (paths) => (Array.isArray(paths) ? paths : []).map((path) => ({
    points: Array.isArray(path.points) ? path.points.map((point) => ({ x: point.x, y: point.y })) : [],
    closed: path.closed === true,
  }));
  const nativeCanvasPathTransform = (paths, matrix) => (Array.isArray(paths) ? paths : []).map((path) => ({
    points: Array.isArray(path.points)
      ? path.points.map((point) => nativeCanvasMatrixPoint(matrix, point.x, point.y))
      : [],
    closed: path.closed === true,
  }));
  const nativeCanvasPathEnsureSubpath = (paths) => {
    if (!paths.length || paths[paths.length - 1].closed) paths.push({ points: [], closed: false });
    return paths[paths.length - 1];
  };
  const nativeCanvasPathPushPoint = (subpath, point) => {
    const last = subpath.points[subpath.points.length - 1];
    if (!last || last.x !== point.x || last.y !== point.y) subpath.points.push(point);
  };
  const nativeCanvasPathMove = (paths, x, y) => {
    paths.push({ points: [{ x: nativeCanvasPathCoordinate(x), y: nativeCanvasPathCoordinate(y) }], closed: false });
  };
  const nativeCanvasPathLine = (paths, x, y) => {
    const subpath = nativeCanvasPathEnsureSubpath(paths);
    if (subpath.points.length === 0) subpath.points.push({ x: 0, y: 0 });
    nativeCanvasPathPushPoint(subpath, { x: nativeCanvasPathCoordinate(x), y: nativeCanvasPathCoordinate(y) });
  };
  const nativeCanvasPathClose = (paths) => {
    const subpath = paths[paths.length - 1];
    if (!subpath || subpath.points.length < 2) return;
    subpath.closed = true;
    nativeCanvasPathPushPoint(subpath, { ...subpath.points[0] });
  };
  const nativeCanvasPathRect = (paths, x, y, width, height) => {
    const left = nativeCanvasPathCoordinate(x);
    const top = nativeCanvasPathCoordinate(y);
    const right = left + nativeCanvasPathCoordinate(width);
    const bottom = top + nativeCanvasPathCoordinate(height);
    if (![right, bottom].every(Number.isFinite)) throw new TypeError("path rectangle is invalid");
    paths.push({
      points: [{ x: left, y: top }, { x: right, y: top }, { x: right, y: bottom }, { x: left, y: bottom }, { x: left, y: top }],
      closed: true,
    });
  };
  const nativeCanvasPathArc = (paths, x, y, radius, startAngle, endAngle, anticlockwise = false) => {
    const centerX = nativeCanvasPathCoordinate(x);
    const centerY = nativeCanvasPathCoordinate(y);
    const numericRadius = nativeCanvasPathCoordinate(radius);
    if (numericRadius < 0) throw nativeCanvasError("arc radius is invalid");
    let start = nativeCanvasPathCoordinate(startAngle);
    let end = nativeCanvasPathCoordinate(endAngle);
    const full = Math.PI * 2;
    if (!anticlockwise && end - start >= full) end = start + full;
    else if (anticlockwise && start - end >= full) end = start - full;
    else if (!anticlockwise && end < start) end = start;
    else if (anticlockwise && end > start) end = start;
    const segments = Math.max(4, Math.min(128, Math.ceil(Math.abs(end - start) * 16)));
    const points = [];
    for (let index = 0; index <= segments; index += 1) {
      const amount = index / segments;
      const angle = start + (end - start) * amount;
      points.push({ x: centerX + Math.cos(angle) * numericRadius, y: centerY + Math.sin(angle) * numericRadius });
    }
    const subpath = paths[paths.length - 1];
    if (subpath && subpath.points.length > 0 && !subpath.closed) {
      nativeCanvasPathPushPoint(subpath, points[0]);
      subpath.points.push(...points.slice(1));
    } else {
      paths.push({ points, closed: false });
    }
  };
  const nativeCanvasPathEllipse = (paths, x, y, radiusX, radiusY, rotation = 0, startAngle = 0, endAngle = Math.PI * 2, anticlockwise = false) => {
    const centerX = nativeCanvasPathCoordinate(x);
    const centerY = nativeCanvasPathCoordinate(y);
    const rx = nativeCanvasPathCoordinate(radiusX);
    const ry = nativeCanvasPathCoordinate(radiusY);
    if (rx < 0 || ry < 0) throw nativeCanvasError("ellipse radius is invalid");
    const rotationRadians = nativeCanvasPathCoordinate(rotation);
    const start = nativeCanvasPathCoordinate(startAngle);
    const end = nativeCanvasPathCoordinate(endAngle);
    const segments = Math.max(8, Math.min(128, Math.ceil(Math.abs(end - start) * 16)));
    const cos = Math.cos(rotationRadians);
    const sin = Math.sin(rotationRadians);
    const points = [];
    for (let index = 0; index <= segments; index += 1) {
      const amount = index / segments;
      const angle = start + (end - start) * (anticlockwise ? -amount : amount);
      const px = Math.cos(angle) * rx;
      const py = Math.sin(angle) * ry;
      points.push({ x: centerX + px * cos - py * sin, y: centerY + px * sin + py * cos });
    }
    const subpath = paths[paths.length - 1];
    if (subpath && subpath.points.length > 0 && !subpath.closed) {
      nativeCanvasPathPushPoint(subpath, points[0]);
      subpath.points.push(...points.slice(1));
    } else {
      paths.push({ points, closed: false });
    }
  };
  const nativeCanvasPathQuadratic = (paths, cpx, cpy, x, y) => {
    const subpath = nativeCanvasPathEnsureSubpath(paths);
    if (subpath.points.length === 0) subpath.points.push({ x: 0, y: 0 });
    const start = subpath.points[subpath.points.length - 1];
    const controlX = nativeCanvasPathCoordinate(cpx);
    const controlY = nativeCanvasPathCoordinate(cpy);
    const endX = nativeCanvasPathCoordinate(x);
    const endY = nativeCanvasPathCoordinate(y);
    for (let index = 1; index <= 16; index += 1) {
      const amount = index / 16;
      const inverse = 1 - amount;
      nativeCanvasPathPushPoint(subpath, {
        x: inverse * inverse * start.x + 2 * inverse * amount * controlX + amount * amount * endX,
        y: inverse * inverse * start.y + 2 * inverse * amount * controlY + amount * amount * endY,
      });
    }
  };
  const nativeCanvasPathBezier = (paths, cp1x, cp1y, cp2x, cp2y, x, y) => {
    const subpath = nativeCanvasPathEnsureSubpath(paths);
    if (subpath.points.length === 0) subpath.points.push({ x: 0, y: 0 });
    const start = subpath.points[subpath.points.length - 1];
    const controls = [cp1x, cp1y, cp2x, cp2y, x, y].map(nativeCanvasPathCoordinate);
    for (let index = 1; index <= 24; index += 1) {
      const amount = index / 24;
      const inverse = 1 - amount;
      nativeCanvasPathPushPoint(subpath, {
        x: inverse ** 3 * start.x + 3 * inverse ** 2 * amount * controls[0] + 3 * inverse * amount ** 2 * controls[2] + amount ** 3 * controls[4],
        y: inverse ** 3 * start.y + 3 * inverse ** 2 * amount * controls[1] + 3 * inverse * amount ** 2 * controls[3] + amount ** 3 * controls[5],
      });
    }
  };
  const nativeCanvasPathMatrix = (value) => {
    if (value === undefined || value === null) return nativeCanvasMatrixIdentity();
    return nativeDomMatrixValues(value);
  };
  const nativeCanvasPathData = (value) => {
    const text = String(value);
    const tokenPattern = /[a-zA-Z]|[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?/g;
    const tokens = text.match(tokenPattern) || [];
    if (text.replace(tokenPattern, "").replace(/[\s,]/g, "") !== "") throw new SyntaxError("unsupported Path2D data");
    const paths = [];
    let index = 0;
    let command = null;
    let current = { x: 0, y: 0 };
    let subpathStart = { ...current };
    const isCommand = (token) => /^[a-zA-Z]$/.test(token);
    const readNumbers = (count) => {
      if (index + count > tokens.length || tokens.slice(index, index + count).some(isCommand)) throw new SyntaxError("invalid Path2D data");
      const values = tokens.slice(index, index + count).map(Number);
      if (!values.every(Number.isFinite)) throw new SyntaxError("invalid Path2D data");
      index += count;
      return values;
    };
    while (index < tokens.length) {
      if (isCommand(tokens[index])) command = tokens[index++];
      if (!command) throw new SyntaxError("invalid Path2D data");
      const relative = command === command.toLowerCase();
      const normalized = command.toUpperCase();
      if (normalized === "Z") {
        nativeCanvasPathClose(paths);
        current = { ...subpathStart };
        command = null;
        continue;
      }
      const values = readNumbers(normalized === "M" || normalized === "L" ? 2 : normalized === "H" || normalized === "V" ? 1 : normalized === "Q" ? 4 : normalized === "C" ? 6 : 0);
      if (values.length === 0) throw new SyntaxError("unsupported Path2D command");
      if (normalized === "M") {
        const x = values[0] + (relative ? current.x : 0);
        const y = values[1] + (relative ? current.y : 0);
        nativeCanvasPathMove(paths, x, y);
        current = { x, y };
        subpathStart = { ...current };
        command = relative ? "l" : "L";
      } else if (normalized === "L") {
        const x = values[0] + (relative ? current.x : 0);
        const y = values[1] + (relative ? current.y : 0);
        nativeCanvasPathLine(paths, x, y);
        current = { x, y };
      } else if (normalized === "H") {
        const x = values[0] + (relative ? current.x : 0);
        nativeCanvasPathLine(paths, x, current.y);
        current.x = x;
      } else if (normalized === "V") {
        const y = values[0] + (relative ? current.y : 0);
        nativeCanvasPathLine(paths, current.x, y);
        current.y = y;
      } else if (normalized === "Q") {
        const controlX = values[0] + (relative ? current.x : 0);
        const controlY = values[1] + (relative ? current.y : 0);
        const x = values[2] + (relative ? current.x : 0);
        const y = values[3] + (relative ? current.y : 0);
        nativeCanvasPathQuadratic(paths, controlX, controlY, x, y);
        current = { x, y };
      } else if (normalized === "C") {
        const offsetX = relative ? current.x : 0;
        const offsetY = relative ? current.y : 0;
        const valuesAbsolute = values.map((entry, valueIndex) => entry + (valueIndex % 2 === 0 ? offsetX : offsetY));
        nativeCanvasPathBezier(paths, ...valuesAbsolute);
        current = { x: valuesAbsolute[4], y: valuesAbsolute[5] };
      } else {
        throw new SyntaxError("unsupported Path2D command");
      }
    }
    return paths;
  };
  const Path2DNative = globalThis.__glassPath2DConstructor || function Path2D(path) {
    if (!new.target) throw new TypeError("Path2D must be constructed with new");
    let paths = [];
    if (path !== undefined) {
      if (path && Array.isArray(path.__glassPath)) paths = nativeCanvasPathClone(path.__glassPath);
      else if (typeof path === "string") paths = nativeCanvasPathData(path);
      else throw new TypeError("Path2D source must be a Path2D or path string");
    }
    Object.defineProperty(this, "__glassPath", { configurable: false, enumerable: false, writable: true, value: paths });
  };
  globalThis.__glassPath2DConstructor = Path2DNative;
  globalThis.Path2D = Path2DNative;
  Object.defineProperties(Path2DNative.prototype, {
    addPath: { configurable: true, value(path, transform) { if (!path || !Array.isArray(path.__glassPath)) throw new TypeError("addPath requires a Path2D"); this.__glassPath.push(...nativeCanvasPathTransform(path.__glassPath, nativeCanvasPathMatrix(transform))); } },
    closePath: { configurable: true, value() { nativeCanvasPathClose(this.__glassPath); } },
    moveTo: { configurable: true, value(x, y) { nativeCanvasPathMove(this.__glassPath, x, y); } },
    lineTo: { configurable: true, value(x, y) { nativeCanvasPathLine(this.__glassPath, x, y); } },
    rect: { configurable: true, value(x, y, width, height) { nativeCanvasPathRect(this.__glassPath, x, y, width, height); } },
    arc: { configurable: true, value(x, y, radius, startAngle, endAngle, anticlockwise = false) { nativeCanvasPathArc(this.__glassPath, x, y, radius, startAngle, endAngle, anticlockwise); } },
    ellipse: { configurable: true, value(x, y, radiusX, radiusY, rotation = 0, startAngle = 0, endAngle = Math.PI * 2, anticlockwise = false) { nativeCanvasPathEllipse(this.__glassPath, x, y, radiusX, radiusY, rotation, startAngle, endAngle, anticlockwise); } },
    quadraticCurveTo: { configurable: true, value(cpx, cpy, x, y) { nativeCanvasPathQuadratic(this.__glassPath, cpx, cpy, x, y); } },
    bezierCurveTo: { configurable: true, value(cp1x, cp1y, cp2x, cp2y, x, y) { nativeCanvasPathBezier(this.__glassPath, cp1x, cp1y, cp2x, cp2y, x, y); } },
  });
  const nativeCanvasPathArgument = (context, value) => {
    if (value === undefined || value === null) return nativeCanvasEnsurePath(context);
    if (!value || !Array.isArray(value.__glassPath)) throw new TypeError("path must be a Path2D");
    return nativeCanvasPathTransform(value.__glassPath, context.__glassCanvasState.transform);
  };
  const nativeCanvasResetSurface = (canvas) => {
    const entry = canvas.__glassCanvasEntry();
    const dimensions = nativeCanvasDimensions(entry);
    const surface = {
      width: dimensions.width,
      height: dimensions.height,
      pixels: nativeCanvasBlankPixels(dimensions.width, dimensions.height),
      originClean: true,
    };
    nativeCanvasSurfaces.set(Number(canvas.nodeIndex), surface);
    if (canvas.__glassCanvasContext) canvas.__glassCanvasContext.__glassCanvasState = nativeCanvasState();
    nativeCanvasCommit(canvas, surface);
  };
  const CanvasGradientNative = globalThis.__glassCanvasGradientConstructor || function CanvasGradient() {
    throw new TypeError("CanvasGradient cannot be constructed directly");
  };
  globalThis.__glassCanvasGradientConstructor = CanvasGradientNative;
  globalThis.CanvasGradient = CanvasGradientNative;
  const nativeCanvasGradient = (kind, values) => {
    const gradient = Object.create(CanvasGradientNative.prototype);
    Object.defineProperties(gradient, {
      __glassCanvasGradient: { configurable: false, enumerable: false, value: true },
      kind: { configurable: false, enumerable: false, value: kind },
      x0: { configurable: false, enumerable: false, writable: true, value: values[0] },
      y0: { configurable: false, enumerable: false, writable: true, value: values[1] },
      x1: { configurable: false, enumerable: false, writable: true, value: values[2] },
      y1: { configurable: false, enumerable: false, writable: true, value: values[3] },
      r0: { configurable: false, enumerable: false, writable: true, value: values[2] || 0 },
      r1: { configurable: false, enumerable: false, writable: true, value: values[3] || 1 },
      stops: { configurable: false, enumerable: false, writable: true, value: [] },
    });
    return gradient;
  };
  Object.defineProperty(CanvasGradientNative.prototype, "addColorStop", {
    configurable: true,
    value(offset, color) {
      const numeric = Number(offset);
      if (!Number.isFinite(numeric) || numeric < 0 || numeric > 1) throw nativeCanvasError("gradient offset is outside 0..1", "IndexSizeError");
      const parsed = nativeCanvasParseColor(String(color));
      if (!parsed) throw nativeCanvasError("gradient color is invalid", "SyntaxError");
      this.stops.push({ offset: numeric, color: parsed });
      this.stops.sort((left, right) => left.offset - right.offset);
    },
  });
  const ImageDataNative = globalThis.__glassImageDataConstructor || function ImageData(dataOrWidth, widthOrHeight, height) {
    if (!(this instanceof ImageDataNative)) throw new TypeError("ImageData requires new");
    let data;
    let width;
    let actualHeight;
    if (typeof dataOrWidth === "number") {
      width = Number(dataOrWidth);
      actualHeight = widthOrHeight === undefined ? width : Number(widthOrHeight);
      if (!Number.isSafeInteger(width) || !Number.isSafeInteger(actualHeight) || width < 1 || actualHeight < 1) throw nativeCanvasError("ImageData dimensions are invalid");
      data = new Uint8ClampedArray(width * actualHeight * 4);
    } else {
      if (!dataOrWidth || typeof dataOrWidth.length !== "number") throw new TypeError("ImageData data must be an array-like value");
      data = new Uint8ClampedArray(Array.from(dataOrWidth));
      width = Number(widthOrHeight);
      actualHeight = Number(height);
      if (!Number.isSafeInteger(width) || !Number.isSafeInteger(actualHeight) || width < 1 || actualHeight < 1 || data.length !== width * actualHeight * 4) throw nativeCanvasError("ImageData data dimensions are invalid");
    }
    if (width * actualHeight > nativeCanvasPixelLimit) throw nativeCanvasError("ImageData exceeds the native pixel limit", "QuotaExceededError");
    Object.defineProperties(this, {
      data: { configurable: false, enumerable: true, value: data },
      width: { configurable: false, enumerable: true, value: width },
      height: { configurable: false, enumerable: true, value: actualHeight },
      colorSpace: { configurable: false, enumerable: true, value: "srgb" },
    });
  };
  globalThis.__glassImageDataConstructor = ImageDataNative;
  globalThis.ImageData = ImageDataNative;
  const ImageBitmapNative = globalThis.__glassImageBitmapConstructor || function ImageBitmap() {
    throw new TypeError("ImageBitmap cannot be constructed directly");
  };
  globalThis.__glassImageBitmapConstructor = ImageBitmapNative;
  globalThis.ImageBitmap = ImageBitmapNative;
  const nativeCanvasImageBitmap = (surface) => {
    const bitmap = Object.create(ImageBitmapNative.prototype);
    Object.defineProperty(bitmap, "__glassImageBitmapSurface", {
      configurable: false,
      enumerable: false,
      writable: true,
      value: surface,
    });
    return bitmap;
  };
  const nativeCanvasImageBitmapSurface = (value) => {
    try {
      return value instanceof ImageBitmapNative
        ? value.__glassImageBitmapSurface || null
        : null;
    } catch (_error) {
      return null;
    }
  };
  globalThis.__glassNativeImageBitmapIsImageBitmap = (value) => {
    try { return value instanceof ImageBitmapNative; }
    catch (_error) { return false; }
  };
  globalThis.__glassNativeImageBitmapTransferDescriptor = (value) => {
    const surface = nativeCanvasImageBitmapSurface(value);
    if (!surface) throw new TypeError("ImageBitmap is closed");
    return {
      type: "imagebitmap",
      width: surface.width,
      height: surface.height,
      pixels: Array.isArray(surface.pixels) ? surface.pixels.slice() : [],
      originClean: surface.originClean !== false,
    };
  };
  globalThis.__glassNativeImageBitmapDetach = (value) => {
    const surface = nativeCanvasImageBitmapSurface(value);
    if (!surface) return false;
    value.__glassImageBitmapSurface = null;
    return true;
  };
  globalThis.__glassNativeImageBitmapFromTransfer = (descriptor) => {
    if (!descriptor || typeof descriptor !== "object")
      throw new TypeError("ImageBitmap transfer descriptor is invalid");
    return nativeCanvasImageBitmap({
      width: Number(descriptor.width),
      height: Number(descriptor.height),
      pixels: Array.isArray(descriptor.pixels) ? descriptor.pixels.slice() : [],
      originClean: descriptor.originClean !== false,
    });
  };
  globalThis.__glassNativeImageBitmapFromSurface = (surface) =>
    nativeCanvasImageBitmap({
      width: Number(surface && surface.width),
      height: Number(surface && surface.height),
      pixels: Array.isArray(surface && surface.pixels) ? surface.pixels.slice() : [],
      originClean: !surface || surface.originClean !== false,
    });
  Object.defineProperties(ImageBitmapNative.prototype, {
    width: {
      configurable: true,
      enumerable: true,
      get() { return this.__glassImageBitmapSurface ? this.__glassImageBitmapSurface.width : 0; },
    },
    height: {
      configurable: true,
      enumerable: true,
      get() { return this.__glassImageBitmapSurface ? this.__glassImageBitmapSurface.height : 0; },
    },
    close: {
      configurable: true,
      value() { this.__glassImageBitmapSurface = null; },
    },
  });
  const CanvasRenderingContext2DNative = globalThis.__glassCanvasRenderingContext2DConstructor || function CanvasRenderingContext2D() {
    throw new TypeError("CanvasRenderingContext2D cannot be constructed directly");
  };
  globalThis.__glassCanvasRenderingContext2DConstructor = CanvasRenderingContext2DNative;
  globalThis.CanvasRenderingContext2D = CanvasRenderingContext2DNative;
  const nativeCanvasContextSurface = (context) => nativeCanvasContextSurfaceFor(context.__glassCanvas);
  const nativeCanvasImagePixels = (resource) => {
    if (!resource || !Array.isArray(resource.frames) || resource.frames.length === 0
        || !Number.isFinite(Number(resource.animationOriginNowMs))) return resource.getPixels();
    const elapsed = Math.max(0, Math.floor(Number(host.now_ms) - Number(resource.animationOriginNowMs)));
    const cycleMs = resource.frames.reduce((total, frame) => total + frame.delayMs, 0);
    if (cycleMs <= 0) return resource.frames[0].getPixels();
    if (resource.loopCount !== null) {
      const totalMs = cycleMs * (resource.loopCount + 1);
      if (elapsed >= totalMs) return resource.frames[resource.frames.length - 1].getPixels();
    }
    const position = elapsed % cycleMs;
    let consumed = 0;
    for (const frame of resource.frames) {
      consumed += frame.delayMs;
      if (position < consumed) return frame.getPixels();
    }
    return resource.frames[resource.frames.length - 1].getPixels();
  };
  const nativeCanvasImageOriginClean = (source) => {
    const current = String(source && (source.currentSrc || source.src) || "");
    if (!current || /^(?:data|fixture):/i.test(current)) return true;
    const URLConstructor = globalThis.URL;
    if (typeof URLConstructor !== "function") return false;
    try {
      return new URLConstructor(current, host.url).origin === String(host.origin || "null");
    } catch (_error) {
      return false;
    }
  };
  const nativeCanvasSource = (source) => {
    if (source && Object.prototype.hasOwnProperty.call(source, "__glassImageBitmapSurface")) {
      const surface = source.__glassImageBitmapSurface;
      if (!surface) throw nativeCanvasError("ImageBitmap is closed", "InvalidStateError");
      return { surface, originClean: surface.originClean !== false };
    }
    if (source && source.__glassCanvasSurface) {
      const surface = source.__glassCanvasSurface;
      return { surface, originClean: surface.originClean !== false };
    }
    if (!source || String(source.localName || "").toLowerCase() !== "img") return null;
    const resource = nativeCanvasImageResources.get(Number(source.nodeIndex));
    if (!resource) return null;
    if (source.complete === false) throw nativeCanvasError("image is not ready", "InvalidStateError");
    return { surface: { ...resource, pixels: nativeCanvasImagePixels(resource) }, originClean: nativeCanvasImageOriginClean(source) };
  };
  const nativeCanvasCreateImageBitmap = (source, args) => {
    let sourceInfo;
    if (source instanceof ImageDataNative) {
      sourceInfo = {
        surface: {
          width: source.width,
          height: source.height,
          pixels: Array.from(source.data),
          originClean: true,
        },
        originClean: true,
      };
    } else {
      sourceInfo = nativeCanvasSource(source);
    }
    if (!sourceInfo) throw nativeCanvasError("ImageBitmap source is not ready", "InvalidStateError");
    let sx = 0;
    let sy = 0;
    let sw = sourceInfo.surface.width;
    let sh = sourceInfo.surface.height;
    let options = {};
    if (args.length === 1 && args[0] && typeof args[0] === "object") {
      options = args[0];
    } else if (args.length >= 4 && args.length <= 5) {
      [sx, sy, sw, sh] = args;
      options = args[4] && typeof args[4] === "object" ? args[4] : {};
    } else if (args.length !== 0) {
      throw new TypeError("createImageBitmap arguments are invalid");
    }
    sx = Number(sx);
    sy = Number(sy);
    sw = Number(sw);
    sh = Number(sh);
    if (![sx, sy, sw, sh].every(Number.isFinite) || sw === 0 || sh === 0) {
      throw nativeCanvasError("ImageBitmap crop dimensions are invalid", "RangeError");
    }
    if (sw < 0) {
      sx += sw;
      sw = -sw;
    }
    if (sh < 0) {
      sy += sh;
      sh = -sh;
    }
    let width = options.resizeWidth === undefined ? Math.max(1, Math.round(sw)) : Number(options.resizeWidth);
    let height = options.resizeHeight === undefined ? Math.max(1, Math.round(sh)) : Number(options.resizeHeight);
    if (options.resizeWidth !== undefined && options.resizeHeight === undefined) {
      height = Math.max(1, Math.round(width * sh / sw));
    } else if (options.resizeHeight !== undefined && options.resizeWidth === undefined) {
      width = Math.max(1, Math.round(height * sw / sh));
    }
    if (![width, height].every(Number.isSafeInteger) || width < 1 || height < 1
        || width > nativeCanvasDimensionLimit || height > nativeCanvasDimensionLimit
        || width * height > nativeCanvasPixelLimit) {
      throw nativeCanvasError("ImageBitmap dimensions exceed the native limit", "QuotaExceededError");
    }
    const pixels = nativeCanvasBlankPixels(width, height);
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const sourceX = Math.floor(sx + (x + 0.5) * sw / width);
        const sourceY = Math.floor(sy + (y + 0.5) * sh / height);
        if (sourceX < 0 || sourceY < 0 || sourceX >= sourceInfo.surface.width || sourceY >= sourceInfo.surface.height) continue;
        const sourceIndex = (sourceY * sourceInfo.surface.width + sourceX) * 4;
        const targetIndex = (y * width + x) * 4;
        for (let channel = 0; channel < 4; channel += 1) pixels[targetIndex + channel] = sourceInfo.surface.pixels[sourceIndex + channel];
      }
    }
    return nativeCanvasImageBitmap({ width, height, pixels, originClean: sourceInfo.originClean });
  };
  globalThis.createImageBitmap = (source, ...args) => new Promise((resolve, reject) => {
    try {
      resolve(nativeCanvasCreateImageBitmap(source, args));
    } catch (error) {
      reject(error);
    }
  });
  const nativeCanvasContextDrawPolygon = (context, points, style = context.__glassCanvasState.fillStyle, operation = context.__glassCanvasState.globalCompositeOperation) => {
    const surface = nativeCanvasContextSurface(context);
    nativeCanvasFillPolygon(surface, points, style, context.__glassCanvasState.globalAlpha, operation, context.__glassCanvasState.clipRegions);
    nativeCanvasCommit(context.__glassCanvas, surface);
  };
  const nativeCanvasContextStroke = (context, paths, style = context.__glassCanvasState.strokeStyle) => {
    const surface = nativeCanvasContextSurface(context);
    nativeCanvasStrokePath(surface, paths, style, context.__glassCanvasState.globalAlpha, context.__glassCanvasState.globalCompositeOperation, context.__glassCanvasState.lineWidth, context.__glassCanvasState.clipRegions);
    nativeCanvasCommit(context.__glassCanvas, surface);
  };
  const nativeCanvasTextSize = (context) => {
    const match = String(context.__glassCanvasState.font).match(/([0-9]+(?:\.[0-9]+)?)px/i);
    return Math.max(1, nativeCanvasNumber(match ? match[1] : 10, 10));
  };
  const nativeCanvasDrawText = (context, text, x, y, maxWidth, stroke) => {
    const content = String(text);
    const size = nativeCanvasTextSize(context);
    const advance = size * 0.62;
    const width = content.length * advance;
    const scale = maxWidth === undefined || !Number.isFinite(Number(maxWidth)) || width <= Number(maxWidth)
      ? 1
      : Math.max(0, Number(maxWidth) / width);
    const baseline = context.__glassCanvasState.textBaseline === "top" ? 0
      : context.__glassCanvasState.textBaseline === "middle" ? size * 0.35
      : context.__glassCanvasState.textBaseline === "bottom" ? size
      : size * 0.8;
    const align = context.__glassCanvasState.textAlign === "center" ? width * scale / 2
      : context.__glassCanvasState.textAlign === "right" || context.__glassCanvasState.textAlign === "end" ? width * scale
      : 0;
    const surface = nativeCanvasContextSurface(context);
    for (let index = 0; index < content.length; index += 1) {
      if (content[index] === " ") continue;
      const left = Number(x) - align + index * advance * scale;
      const top = Number(y) - baseline;
      const points = [
        nativeCanvasPathPoint(context, left, top),
        nativeCanvasPathPoint(context, left + Math.max(1, advance * 0.82) * scale, top),
        nativeCanvasPathPoint(context, left + Math.max(1, advance * 0.82) * scale, top + size),
        nativeCanvasPathPoint(context, left, top + size),
      ];
      nativeCanvasFillPolygon(surface, points, stroke ? context.__glassCanvasState.strokeStyle : context.__glassCanvasState.fillStyle, context.__glassCanvasState.globalAlpha, context.__glassCanvasState.globalCompositeOperation, context.__glassCanvasState.clipRegions);
    }
    nativeCanvasCommit(context.__glassCanvas, surface);
  };
  const nativeCanvasInstallContextMethods = () => {
    const prototype = CanvasRenderingContext2DNative.prototype;
    Object.defineProperties(prototype, {
      canvas: { configurable: true, enumerable: true, get() { return this.__glassCanvas; } },
      fillStyle: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.fillStyle; }, set(value) { if (nativeCanvasParseColor(value)) this.__glassCanvasState.fillStyle = value; } },
      strokeStyle: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.strokeStyle; }, set(value) { if (nativeCanvasParseColor(value)) this.__glassCanvasState.strokeStyle = value; } },
      globalAlpha: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.globalAlpha; }, set(value) { const numeric = Number(value); if (Number.isFinite(numeric) && numeric >= 0 && numeric <= 1) this.__glassCanvasState.globalAlpha = numeric; } },
      globalCompositeOperation: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.globalCompositeOperation; }, set(value) { const operation = String(value); if (["source-over", "copy", "clear", "destination-over", "lighter"].includes(operation)) this.__glassCanvasState.globalCompositeOperation = operation; } },
      lineWidth: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.lineWidth; }, set(value) { const numeric = Number(value); if (Number.isFinite(numeric) && numeric > 0) this.__glassCanvasState.lineWidth = numeric; } },
      lineCap: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.lineCap; }, set(value) { if (["butt", "round", "square"].includes(String(value))) this.__glassCanvasState.lineCap = String(value); } },
      lineJoin: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.lineJoin; }, set(value) { if (["miter", "round", "bevel"].includes(String(value))) this.__glassCanvasState.lineJoin = String(value); } },
      miterLimit: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.miterLimit; }, set(value) { const numeric = Number(value); if (Number.isFinite(numeric) && numeric > 0) this.__glassCanvasState.miterLimit = numeric; } },
      font: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.font; }, set(value) { this.__glassCanvasState.font = String(value); } },
      textAlign: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.textAlign; }, set(value) { if (["left", "right", "center", "start", "end"].includes(String(value))) this.__glassCanvasState.textAlign = String(value); } },
      textBaseline: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.textBaseline; }, set(value) { if (["top", "hanging", "middle", "alphabetic", "ideographic", "bottom"].includes(String(value))) this.__glassCanvasState.textBaseline = String(value); } },
      direction: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.direction; }, set(value) { if (["ltr", "rtl", "inherit"].includes(String(value))) this.__glassCanvasState.direction = String(value); } },
      lineDashOffset: { configurable: true, enumerable: true, get() { return this.__glassCanvasState.lineDashOffset; }, set(value) { const numeric = Number(value); if (Number.isFinite(numeric)) this.__glassCanvasState.lineDashOffset = numeric; } },
      fillRect: { configurable: true, value(x, y, width, height) { const left = Number(x); const top = Number(y); const right = left + Number(width); const bottom = top + Number(height); if (![left, top, right, bottom].every(Number.isFinite)) throw new TypeError("fillRect arguments must be finite"); nativeCanvasContextDrawPolygon(this, [nativeCanvasPathPoint(this, left, top), nativeCanvasPathPoint(this, right, top), nativeCanvasPathPoint(this, right, bottom), nativeCanvasPathPoint(this, left, bottom)]); } },
      clearRect: { configurable: true, value(x, y, width, height) { const left = Number(x); const top = Number(y); const right = left + Number(width); const bottom = top + Number(height); if (![left, top, right, bottom].every(Number.isFinite)) throw new TypeError("clearRect arguments must be finite"); nativeCanvasContextDrawPolygon(this, [nativeCanvasPathPoint(this, left, top), nativeCanvasPathPoint(this, right, top), nativeCanvasPathPoint(this, right, bottom), nativeCanvasPathPoint(this, left, bottom)], [0, 0, 0, 0], "clear"); } },
      strokeRect: { configurable: true, value(x, y, width, height) { const left = Number(x); const top = Number(y); const right = left + Number(width); const bottom = top + Number(height); nativeCanvasContextStroke(this, [{ points: [nativeCanvasPathPoint(this, left, top), nativeCanvasPathPoint(this, right, top), nativeCanvasPathPoint(this, right, bottom), nativeCanvasPathPoint(this, left, bottom), nativeCanvasPathPoint(this, left, top)], closed: true }]); } },
      beginPath: { configurable: true, value() { this.__glassCanvasState.path = []; } },
      closePath: { configurable: true, value() { const path = nativeCanvasCurrentPath(this); if (path.points.length > 1) { path.closed = true; path.points.push(path.points[0]); } } },
      moveTo: { configurable: true, value(x, y) { const path = nativeCanvasEnsurePath(this); path.push({ points: [nativeCanvasPathPoint(this, Number(x), Number(y))], closed: false }); } },
      lineTo: { configurable: true, value(x, y) { nativeCanvasAddPathPoint(this, nativeCanvasPathPoint(this, Number(x), Number(y))); } },
      rect: { configurable: true, value(x, y, width, height) { const path = nativeCanvasEnsurePath(this); const left = Number(x); const top = Number(y); const right = left + Number(width); const bottom = top + Number(height); path.push({ points: [nativeCanvasPathPoint(this, left, top), nativeCanvasPathPoint(this, right, top), nativeCanvasPathPoint(this, right, bottom), nativeCanvasPathPoint(this, left, bottom), nativeCanvasPathPoint(this, left, top)], closed: true }); } },
      arc: { configurable: true, value(x, y, radius, startAngle, endAngle, anticlockwise = false) { const r = Number(radius); if (!Number.isFinite(r) || r < 0) throw nativeCanvasError("arc radius is invalid"); let start = Number(startAngle); let end = Number(endAngle); const full = Math.PI * 2; if (!anticlockwise && end - start >= full) end = start + full; else if (anticlockwise && start - end >= full) end = start - full; else if (!anticlockwise && end < start) end = start; else if (anticlockwise && end > start) end = start; const segments = Math.max(4, Math.min(128, Math.ceil(Math.abs(end - start) * 16))); const path = nativeCanvasEnsurePath(this); const points = []; for (let index = 0; index <= segments; index += 1) { const amount = index / segments; const angle = start + (end - start) * amount; points.push(nativeCanvasPathPoint(this, Number(x) + Math.cos(angle) * r, Number(y) + Math.sin(angle) * r)); } path.push({ points, closed: false }); } },
      ellipse: { configurable: true, value(x, y, radiusX, radiusY, rotation = 0, startAngle = 0, endAngle = Math.PI * 2, anticlockwise = false) { const rx = Number(radiusX); const ry = Number(radiusY); if (rx < 0 || ry < 0) throw nativeCanvasError("ellipse radius is invalid"); const segments = Math.max(8, Math.min(128, Math.ceil(Math.abs(endAngle - startAngle) * 16))); const points = []; const cos = Math.cos(rotation); const sin = Math.sin(rotation); for (let index = 0; index <= segments; index += 1) { const amount = index / segments; const angle = startAngle + (endAngle - startAngle) * (anticlockwise ? -amount : amount); const px = Math.cos(angle) * rx; const py = Math.sin(angle) * ry; points.push(nativeCanvasPathPoint(this, Number(x) + px * cos - py * sin, Number(y) + px * sin + py * cos)); } nativeCanvasEnsurePath(this).push({ points, closed: false }); } },
      quadraticCurveTo: { configurable: true, value(cpx, cpy, x, y) { const path = nativeCanvasCurrentPath(this); const start = path.points[path.points.length - 1] || nativeCanvasPathPoint(this, 0, 0); const points = []; for (let index = 1; index <= 16; index += 1) { const amount = index / 16; const inverse = 1 - amount; points.push(nativeCanvasPathPoint(this, inverse * inverse * start.x + 2 * inverse * amount * Number(cpx) + amount * amount * Number(x), inverse * inverse * start.y + 2 * inverse * amount * Number(cpy) + amount * amount * Number(y))); } path.points.push(...points); } },
      bezierCurveTo: { configurable: true, value(cp1x, cp1y, cp2x, cp2y, x, y) { const path = nativeCanvasCurrentPath(this); const start = path.points[path.points.length - 1] || nativeCanvasPathPoint(this, 0, 0); const points = []; for (let index = 1; index <= 24; index += 1) { const amount = index / 24; const inverse = 1 - amount; points.push(nativeCanvasPathPoint(this, inverse ** 3 * start.x + 3 * inverse ** 2 * amount * Number(cp1x) + 3 * inverse * amount ** 2 * Number(cp2x) + amount ** 3 * Number(x), inverse ** 3 * start.y + 3 * inverse ** 2 * amount * Number(cp1y) + 3 * inverse * amount ** 2 * Number(cp2y) + amount ** 3 * Number(y))); } path.points.push(...points); } },
      fill: { configurable: true, value(pathOrRule, maybeRule) { const path = pathOrRule && pathOrRule.__glassPath ? pathOrRule : undefined; const fillRule = path ? maybeRule === undefined ? "nonzero" : maybeRule : pathOrRule === undefined ? "nonzero" : pathOrRule; if (!["nonzero", "evenodd"].includes(String(fillRule))) throw new TypeError("fill rule is invalid"); const operation = this.__glassCanvasState.globalCompositeOperation; const surface = nativeCanvasContextSurface(this); nativeCanvasFillPaths(surface, nativeCanvasPathArgument(this, path), this.__glassCanvasState.fillStyle, this.__glassCanvasState.globalAlpha, operation, this.__glassCanvasState.clipRegions, String(fillRule)); nativeCanvasCommit(this.__glassCanvas, surface); } },
      stroke: { configurable: true, value(path) { nativeCanvasContextStroke(this, nativeCanvasPathArgument(this, path)); } },
      save: { configurable: true, value() { const current = this.__glassCanvasState; this.__glassCanvasState.stack.push({ ...current, transform: current.transform.slice(), lineDash: current.lineDash.slice(), clipRegions: current.clipRegions.map((region) => ({ fillRule: region.fillRule, paths: nativeCanvasPathClone(region.paths) })), path: undefined, stack: undefined }); } },
      restore: { configurable: true, value() { const current = this.__glassCanvasState; const saved = current.stack.pop(); if (saved) { saved.stack = current.stack; this.__glassCanvasState = saved; } } },
      reset: { configurable: true, value() { this.__glassCanvasState = nativeCanvasState(); } },
      translate: { configurable: true, value(x, y) { this.__glassCanvasState.transform = nativeCanvasMatrixMultiply(this.__glassCanvasState.transform, [1, 0, 0, 1, Number(x), Number(y)]); } },
      scale: { configurable: true, value(x, y = x) { this.__glassCanvasState.transform = nativeCanvasMatrixMultiply(this.__glassCanvasState.transform, [Number(x), 0, 0, Number(y), 0, 0]); } },
      rotate: { configurable: true, value(angle) { const radians = Number(angle); this.__glassCanvasState.transform = nativeCanvasMatrixMultiply(this.__glassCanvasState.transform, [Math.cos(radians), Math.sin(radians), -Math.sin(radians), Math.cos(radians), 0, 0]); } },
      transform: { configurable: true, value(a, b, c, d, e, f) { const matrix = a && typeof a === "object" ? nativeDomMatrixValues(a) : [Number(a), Number(b), Number(c), Number(d), Number(e), Number(f)]; if (!matrix.every(Number.isFinite)) throw new TypeError("transform values must be finite"); this.__glassCanvasState.transform = nativeCanvasMatrixMultiply(this.__glassCanvasState.transform, matrix); } },
      setTransform: { configurable: true, value(a, b, c, d, e, f) { if (a && typeof a === "object") this.__glassCanvasState.transform = nativeDomMatrixValues(a); else { const matrix = [Number(a === undefined ? 1 : a), Number(b || 0), Number(c || 0), Number(d === undefined ? 1 : d), Number(e || 0), Number(f || 0)]; if (!matrix.every(Number.isFinite)) throw new TypeError("transform values must be finite"); this.__glassCanvasState.transform = matrix; } } },
      resetTransform: { configurable: true, value() { this.__glassCanvasState.transform = nativeCanvasMatrixIdentity(); } },
      getTransform: { configurable: true, value() { return nativeCanvasTransformObject(this.__glassCanvasState.transform.slice()); } },
      getContextAttributes: { configurable: true, value() { return { alpha: true, desynchronized: false, willReadFrequently: false }; } },
      getLineDash: { configurable: true, value() { return this.__glassCanvasState.lineDash.slice(); } },
      setLineDash: { configurable: true, value(value) { if (!Array.isArray(value) || value.some((entry) => !Number.isFinite(Number(entry)) || Number(entry) < 0)) throw new TypeError("line dash must be a non-negative array"); this.__glassCanvasState.lineDash = value.map(Number); } },
      clip: { configurable: true, value(pathOrRule, maybeRule) { const path = pathOrRule && pathOrRule.__glassPath ? pathOrRule : undefined; const fillRule = path ? maybeRule === undefined ? "nonzero" : maybeRule : pathOrRule === undefined ? "nonzero" : pathOrRule; if (!["nonzero", "evenodd"].includes(String(fillRule))) throw new TypeError("clip rule is invalid"); this.__glassCanvasState.clipRegions.push({ fillRule: String(fillRule), paths: nativeCanvasPathClone(nativeCanvasPathArgument(this, path)) }); } },
      isPointInPath: { configurable: true, value(...args) { const path = args[0] && args[0].__glassPath ? args[0] : undefined; const offset = path ? 1 : 0; const x = Number(args[offset]); const y = Number(args[offset + 1]); const fillRule = args[offset + 2] === undefined ? "nonzero" : String(args[offset + 2]); if (!["nonzero", "evenodd"].includes(fillRule)) throw new TypeError("fill rule is invalid"); if (![x, y].every(Number.isFinite)) return false; const point = nativeCanvasPathPoint(this, x, y); return nativeCanvasPointInPaths(point, nativeCanvasPathArgument(this, path), fillRule); } },
      isPointInStroke: { configurable: true, value(...args) { const path = args[0] && args[0].__glassPath ? args[0] : undefined; const offset = path ? 1 : 0; const x = Number(args[offset]); const y = Number(args[offset + 1]); if (![x, y].every(Number.isFinite)) return false; const point = nativeCanvasPathPoint(this, x, y); const radius = Math.max(0.5, this.__glassCanvasState.lineWidth / 2); return nativeCanvasPathArgument(this, path).some((subpath) => subpath.points.slice(1).some((end, index) => nativeCanvasDistanceToSegment(point, subpath.points[index], end) <= radius)); } },
      createLinearGradient: { configurable: true, value(x0, y0, x1, y1) { return nativeCanvasGradient("linear", [Number(x0), Number(y0), Number(x1), Number(y1)]); } },
      createRadialGradient: { configurable: true, value(x0, y0, r0, x1, y1, r1) { const gradient = nativeCanvasGradient("radial", [Number(x0), Number(y0), Number(x1), Number(y1)]); gradient.x0 = Number(x0); gradient.y0 = Number(y0); gradient.r0 = Number(r0); gradient.x1 = Number(x1); gradient.y1 = Number(y1); gradient.r1 = Number(r1); return gradient; } },
      createImageData: { configurable: true, value(dataOrWidth, height) { return dataOrWidth instanceof ImageDataNative ? new ImageDataNative(dataOrWidth.width, dataOrWidth.height) : new ImageDataNative(dataOrWidth, height); } },
      getImageData: { configurable: true, value(sx, sy, sw, sh) { const x = Math.trunc(Number(sx)); const y = Math.trunc(Number(sy)); const width = Math.trunc(Number(sw)); const height = Math.trunc(Number(sh)); if (width <= 0 || height <= 0) throw nativeCanvasError("getImageData dimensions must be positive"); if (width * height > nativeCanvasPixelLimit) throw nativeCanvasError("getImageData exceeds the native pixel limit", "QuotaExceededError"); const surface = nativeCanvasContextSurface(this); const bytes = new Uint8ClampedArray(width * height * 4); for (let row = 0; row < height; row += 1) for (let column = 0; column < width; column += 1) { const sourceX = x + column; const sourceY = y + row; if (sourceX < 0 || sourceY < 0 || sourceX >= surface.width || sourceY >= surface.height) continue; const sourceIndex = (sourceY * surface.width + sourceX) * 4; const targetIndex = (row * width + column) * 4; for (let channel = 0; channel < 4; channel += 1) bytes[targetIndex + channel] = surface.pixels[sourceIndex + channel]; } return new ImageDataNative(bytes, width, height); } },
      putImageData: { configurable: true, value(image, dx, dy) { if (!image || !image.data) throw new TypeError("putImageData requires ImageData"); const width = Number(image.width); const height = Number(image.height); if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1 || width * height > nativeCanvasPixelLimit) throw nativeCanvasError("putImageData exceeds the native pixel limit", "QuotaExceededError"); const surface = nativeCanvasContextSurface(this); const offsetX = Math.trunc(Number(dx)); const offsetY = Math.trunc(Number(dy)); for (let row = 0; row < height; row += 1) for (let column = 0; column < width; column += 1) { const targetX = offsetX + column; const targetY = offsetY + row; if (targetX < 0 || targetY < 0 || targetX >= surface.width || targetY >= surface.height) continue; const sourceIndex = (row * width + column) * 4; const targetIndex = (targetY * surface.width + targetX) * 4; for (let channel = 0; channel < 4; channel += 1) surface.pixels[targetIndex + channel] = Number(image.data[sourceIndex + channel]) || 0; } nativeCanvasCommit(this.__glassCanvas, surface); } },
      drawImage: { configurable: true, value(source, ...args) { const sourceSurface = source && source.__glassCanvasSurface ? source.__glassCanvasSurface : null; if (!sourceSurface) throw new TypeError("native drawImage currently requires a canvas source"); let sx = 0; let sy = 0; let sw = sourceSurface.width; let sh = sourceSurface.height; let dx; let dy; let dw; let dh; if (args.length === 2) { [dx, dy] = args; dw = sw; dh = sh; } else if (args.length === 4) { [dx, dy, dw, dh] = args; } else if (args.length === 8) { [sx, sy, sw, sh, dx, dy, dw, dh] = args; } else throw new TypeError("drawImage arguments are invalid"); sx = Number(sx); sy = Number(sy); sw = Number(sw); sh = Number(sh); dx = Number(dx); dy = Number(dy); dw = Number(dw); dh = Number(dh); if (sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0) throw nativeCanvasError("drawImage dimensions are invalid"); const surface = nativeCanvasContextSurface(this); const inverse = nativeCanvasMatrixInverse(this.__glassCanvasState.transform); if (!inverse) return; for (let y = 0; y < surface.height; y += 1) for (let x = 0; x < surface.width; x += 1) { const user = nativeCanvasMatrixPoint(inverse, x + 0.5, y + 0.5); if (user.x < dx || user.x >= dx + dw || user.y < dy || user.y >= dy + dh) continue; const sourceX = Math.floor(sx + (user.x - dx) * sw / dw); const sourceY = Math.floor(sy + (user.y - dy) * sh / dh); if (sourceX < 0 || sourceY < 0 || sourceX >= sourceSurface.width || sourceY >= sourceSurface.height) continue; const sourceIndex = (sourceY * sourceSurface.width + sourceX) * 4; nativeCanvasBlendPixel(surface, x, y, sourceSurface.pixels.slice(sourceIndex, sourceIndex + 4), this.__glassCanvasState.globalAlpha, this.__glassCanvasState.globalCompositeOperation, this.__glassCanvasState.clipRegions); } nativeCanvasCommit(this.__glassCanvas, surface); } },
      measureText: { configurable: true, value(text) { const width = String(text).length * nativeCanvasTextSize(this) * 0.62; return { width, actualBoundingBoxLeft: 0, actualBoundingBoxRight: width, actualBoundingBoxAscent: nativeCanvasTextSize(this) * 0.8, actualBoundingBoxDescent: nativeCanvasTextSize(this) * 0.2 }; } },
      fillText: { configurable: true, value(text, x, y, maxWidth) { nativeCanvasDrawText(this, text, x, y, maxWidth, false); } },
      strokeText: { configurable: true, value(text, x, y, maxWidth) { nativeCanvasDrawText(this, text, x, y, maxWidth, true); } },
    });
  };
  nativeCanvasInstallContextMethods();
  Object.defineProperty(CanvasRenderingContext2DNative.prototype, "drawImage", {
    configurable: true,
    writable: true,
    value(source, ...args) {
      const sourceInfo = nativeCanvasSource(source);
      if (!sourceInfo) throw new TypeError("drawImage source is not a ready canvas or image");
      const sourceSurface = sourceInfo.surface;
      let sx = 0;
      let sy = 0;
      let sw = sourceSurface.width;
      let sh = sourceSurface.height;
      let dx;
      let dy;
      let dw;
      let dh;
      if (args.length === 2) {
        [dx, dy] = args;
        dw = sw;
        dh = sh;
      } else if (args.length === 4) {
        [dx, dy, dw, dh] = args;
      } else if (args.length === 8) {
        [sx, sy, sw, sh, dx, dy, dw, dh] = args;
      } else {
        throw new TypeError("drawImage arguments are invalid");
      }
      sx = Number(sx);
      sy = Number(sy);
      sw = Number(sw);
      sh = Number(sh);
      dx = Number(dx);
      dy = Number(dy);
      dw = Number(dw);
      dh = Number(dh);
      if (![sx, sy, sw, sh, dx, dy, dw, dh].every(Number.isFinite)
          || sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0) {
        throw nativeCanvasError("drawImage dimensions are invalid");
      }
      const surface = nativeCanvasContextSurface(this);
      surface.originClean = surface.originClean && sourceInfo.originClean;
      const inverse = nativeCanvasMatrixInverse(this.__glassCanvasState.transform);
      if (!inverse) return;
      for (let y = 0; y < surface.height; y += 1) {
        for (let x = 0; x < surface.width; x += 1) {
          const user = nativeCanvasMatrixPoint(inverse, x + 0.5, y + 0.5);
          if (user.x < dx || user.x >= dx + dw || user.y < dy || user.y >= dy + dh) continue;
          const sourceX = Math.floor(sx + (user.x - dx) * sw / dw);
          const sourceY = Math.floor(sy + (user.y - dy) * sh / dh);
          if (sourceX < 0 || sourceY < 0 || sourceX >= sourceSurface.width || sourceY >= sourceSurface.height) continue;
          const sourceIndex = (sourceY * sourceSurface.width + sourceX) * 4;
          nativeCanvasBlendPixel(
            surface,
            x,
            y,
            sourceSurface.pixels.slice(sourceIndex, sourceIndex + 4),
            this.__glassCanvasState.globalAlpha,
            this.__glassCanvasState.globalCompositeOperation,
            this.__glassCanvasState.clipRegions,
          );
        }
      }
      nativeCanvasCommit(this.__glassCanvas, surface);
    },
  });
  const nativeCanvasGetImageData = CanvasRenderingContext2DNative.prototype.getImageData;
  Object.defineProperty(CanvasRenderingContext2DNative.prototype, "getImageData", {
    configurable: true,
    writable: true,
    value(...args) {
      nativeCanvasRequireOriginClean(nativeCanvasContextSurface(this));
      return nativeCanvasGetImageData.call(this, ...args);
    },
  });
  const nativeCanvasOffscreenSurface = (canvas) => {
    if (!canvas || canvas.__glassCanvasDetached === true)
      throw nativeCanvasError("OffscreenCanvas has been transferred", "InvalidStateError");
    if (canvas.__glassCanvasSurface) return canvas.__glassCanvasSurface;
    if (typeof canvas.__glassCanvasEntry === "function")
      return nativeCanvasSurfaceForEntry(canvas.__glassCanvasEntry());
    throw nativeCanvasError("canvas context has no backing surface", "InvalidStateError");
  };
  const nativeCanvasContextSurfaceFor = (canvas) => {
    if (canvas && canvas.__glassCanvasDetached === true)
      throw nativeCanvasError("OffscreenCanvas has been transferred", "InvalidStateError");
    if (canvas && canvas.__glassCanvasSurface) return canvas.__glassCanvasSurface;
    if (canvas && typeof canvas.__glassCanvasEntry === "function") {
      return nativeCanvasSurfaceForEntry(canvas.__glassCanvasEntry());
    }
    throw nativeCanvasError("canvas context has no backing surface", "InvalidStateError");
  };
  const nativeCanvasResizeSurface = (surface, width, height) => {
    surface.width = width;
    surface.height = height;
    surface.pixels = nativeCanvasBlankPixels(width, height);
    surface.originClean = true;
  };
  const nativeCanvasSyncOffscreenDimension = (canvas, name, value) => {
    if (!canvas || !Number.isSafeInteger(canvas.nodeIndex) || typeof canvas.__glassCanvasEntry !== "function") return;
    const entry = canvas.__glassCanvasEntry();
    if (!entry || !entry.attributes) return;
    const stringValue = String(value);
    entry.attributes[name] = stringValue;
    pushCommand({ kind: "setAttribute", node_index: canvas.nodeIndex, name, value: stringValue, namespace_uri: "" });
  };
  const nativeCanvasMakeContext = (canvas) => {
    const context = Object.create(CanvasRenderingContext2DNative.prototype);
    Object.defineProperties(context, {
      __glassCanvas: { configurable: false, enumerable: false, value: canvas },
      __glassCanvasState: { configurable: false, enumerable: false, writable: true, value: nativeCanvasState() },
    });
    return context;
  };
  const nativeCanvasOffscreenDimension = (value) => {
    const numeric = Number(value);
    if (!Number.isFinite(numeric) || Math.trunc(numeric) !== numeric || numeric < 0 || numeric > nativeCanvasDimensionLimit) {
      throw nativeCanvasError("OffscreenCanvas dimension is outside the native limit", "IndexSizeError");
    }
    return numeric;
  };
  const OffscreenCanvasNative = globalThis.__glassOffscreenCanvasConstructor || function OffscreenCanvas(width, height) {
    if (!new.target) throw new TypeError("OffscreenCanvas must be constructed with new");
    const normalizedWidth = nativeCanvasOffscreenDimension(width);
    const normalizedHeight = nativeCanvasOffscreenDimension(height);
    if (normalizedWidth * normalizedHeight > nativeCanvasPixelLimit) {
      throw nativeCanvasError("OffscreenCanvas exceeds the native pixel limit", "QuotaExceededError");
    }
    Object.defineProperty(this, "__glassCanvasSurface", {
      configurable: false,
      enumerable: false,
      writable: true,
      value: {
        width: normalizedWidth,
        height: normalizedHeight,
        pixels: nativeCanvasBlankPixels(normalizedWidth, normalizedHeight),
        originClean: true,
      },
    });
    Object.defineProperty(this, "__glassCanvasDetached", {
      configurable: false,
      enumerable: false,
      writable: true,
      value: false,
    });
    Object.defineProperty(this, "nodeIndex", {
      configurable: false,
      enumerable: false,
      value: null,
    });
  };
  globalThis.__glassOffscreenCanvasConstructor = OffscreenCanvasNative;
  globalThis.OffscreenCanvas = OffscreenCanvasNative;
  Object.defineProperties(OffscreenCanvasNative.prototype, {
    width: {
      configurable: true,
      enumerable: true,
      get() { return nativeCanvasOffscreenSurface(this).width; },
      set(value) {
        const width = nativeCanvasOffscreenDimension(value);
        const surface = nativeCanvasOffscreenSurface(this);
        if (width * surface.height > nativeCanvasPixelLimit) throw nativeCanvasError("OffscreenCanvas exceeds the native pixel limit", "QuotaExceededError");
        nativeCanvasResizeSurface(surface, width, surface.height);
        if (this.__glassCanvasContext) this.__glassCanvasContext.__glassCanvasState = nativeCanvasState();
        nativeCanvasSyncOffscreenDimension(this, "width", width);
        nativeCanvasCommit(this, surface);
      },
    },
    height: {
      configurable: true,
      enumerable: true,
      get() { return nativeCanvasOffscreenSurface(this).height; },
      set(value) {
        const height = nativeCanvasOffscreenDimension(value);
        const surface = nativeCanvasOffscreenSurface(this);
        if (surface.width * height > nativeCanvasPixelLimit) throw nativeCanvasError("OffscreenCanvas exceeds the native pixel limit", "QuotaExceededError");
        nativeCanvasResizeSurface(surface, surface.width, height);
        if (this.__glassCanvasContext) this.__glassCanvasContext.__glassCanvasState = nativeCanvasState();
        nativeCanvasSyncOffscreenDimension(this, "height", height);
        nativeCanvasCommit(this, surface);
      },
    },
    getContext: {
      configurable: true,
      enumerable: true,
      value(type) {
        nativeCanvasOffscreenSurface(this);
        if (String(type).toLowerCase() !== "2d") return null;
        if (!this.__glassCanvasContext) {
          Object.defineProperty(this, "__glassCanvasContext", {
            configurable: false,
            enumerable: false,
            value: nativeCanvasMakeContext(this),
          });
        }
        return this.__glassCanvasContext;
      },
    },
    convertToBlob: {
      configurable: true,
      enumerable: true,
      value(options = {}) {
        return new Promise((resolve, reject) => {
          try {
            const type = String(options && options.type || "image/png").toLowerCase();
            if (type !== "image/png") throw nativeCanvasError("only image/png is supported", "NotSupportedError");
            const bytes = nativeCanvasPngBytes(nativeCanvasOffscreenSurface(this));
            resolve(new BlobNative([new Uint8Array(bytes)], { type }));
          } catch (error) {
            reject(error);
          }
        });
      },
    },
    transferToImageBitmap: {
      configurable: true,
      enumerable: true,
      value() {
        const surface = nativeCanvasOffscreenSurface(this);
        return nativeCanvasImageBitmap({ ...surface, pixels: surface.pixels.slice() });
      },
    },
  });
  const nativeCanvasOffscreenCanvasSurface = (value) => {
    try {
      return value instanceof OffscreenCanvasNative
        ? nativeCanvasOffscreenSurface(value)
        : null;
    } catch (_error) {
      return null;
    }
  };
  globalThis.__glassNativeOffscreenCanvasIsOffscreenCanvas = (value) => {
    try { return value instanceof OffscreenCanvasNative; }
    catch (_error) { return false; }
  };
  globalThis.__glassNativeOffscreenCanvasTransferDescriptor = (value) => {
    const surface = nativeCanvasOffscreenCanvasSurface(value);
    if (!surface) throw new TypeError("OffscreenCanvas is detached");
    return {
      type: "offscreencanvas",
      width: surface.width,
      height: surface.height,
      pixels: Array.isArray(surface.pixels) ? surface.pixels.slice() : [],
      originClean: surface.originClean !== false,
    };
  };
  globalThis.__glassNativeOffscreenCanvasDetach = (value) => {
    const surface = nativeCanvasOffscreenCanvasSurface(value);
    if (!surface) return false;
    value.__glassCanvasDetached = true;
    return true;
  };
  globalThis.__glassNativeOffscreenCanvasFromTransfer = (descriptor) => {
    if (!descriptor || typeof descriptor !== "object")
      throw new TypeError("OffscreenCanvas transfer descriptor is invalid");
    const canvas = Object.create(OffscreenCanvasNative.prototype);
    Object.defineProperty(canvas, "__glassCanvasSurface", {
      configurable: false,
      enumerable: false,
      writable: true,
      value: {
        width: Number(descriptor.width),
        height: Number(descriptor.height),
        pixels: Array.isArray(descriptor.pixels) ? descriptor.pixels.slice() : [],
        originClean: descriptor.originClean !== false,
      },
    });
    Object.defineProperty(canvas, "__glassCanvasDetached", {
      configurable: false,
      enumerable: false,
      writable: true,
      value: false,
    });
    Object.defineProperty(canvas, "nodeIndex", {
      configurable: false,
      enumerable: false,
      value: null,
    });
    return canvas;
  };
  const nativeCanvasInstallElement = (element, entry) => {
    if (element.localName !== "canvas") return null;
    let context = null;
    let transferred = null;
    Object.defineProperty(element, "__glassCanvasEntry", { configurable: false, enumerable: false, value: () => entry });
    Object.defineProperty(element, "__glassCanvasSurface", { configurable: false, enumerable: false, get() { return nativeCanvasSurfaceForEntry(entry); } });
    const reset = () => nativeCanvasResetSurface(element);
    const originalSetAttribute = element.setAttribute.bind(element);
    const originalRemoveAttribute = element.removeAttribute.bind(element);
    element.setAttribute = (name, value) => { originalSetAttribute(name, value); if (["width", "height"].includes(String(name).toLowerCase())) reset(); };
    element.removeAttribute = (name) => { originalRemoveAttribute(name); if (["width", "height"].includes(String(name).toLowerCase())) reset(); };
    Object.defineProperties(element, {
      width: { configurable: false, enumerable: true, get() { return nativeCanvasSurfaceForEntry(entry).width; }, set(value) { const numeric = nativeCanvasDimensionSetter(value); if (numeric * nativeCanvasDimension(entry, "height", 150) > nativeCanvasPixelLimit) throw nativeCanvasError("canvas exceeds the native pixel limit", "QuotaExceededError"); element.setAttribute("width", String(numeric)); } },
      height: { configurable: false, enumerable: true, get() { return nativeCanvasSurfaceForEntry(entry).height; }, set(value) { const numeric = nativeCanvasDimensionSetter(value); if (nativeCanvasDimension(entry, "width", 300) * numeric > nativeCanvasPixelLimit) throw nativeCanvasError("canvas exceeds the native pixel limit", "QuotaExceededError"); element.setAttribute("height", String(numeric)); } },
      getContext: { configurable: false, enumerable: true, value(type) { if (transferred) throw nativeCanvasError("canvas control has been transferred", "InvalidStateError"); if (String(type).toLowerCase() !== "2d") return null; if (!context) context = nativeCanvasMakeContext(element); element.__glassCanvasContext = context; return context; } },
      toDataURL: { configurable: false, enumerable: true, value(type = "image/png") { if (transferred) throw nativeCanvasError("canvas control has been transferred", "InvalidStateError"); const normalized = String(type).toLowerCase(); if (normalized !== "image/png") return "data:,"; const surface = nativeCanvasSurfaceForEntry(entry); const bytes = nativeCanvasPngBytes(surface); return "data:image/png;base64," + encodeBase64(bytes, nativeCanvasByteLimit); } },
      toBlob: { configurable: false, enumerable: true, value(callback, type = "image/png") { if (transferred) throw nativeCanvasError("canvas control has been transferred", "InvalidStateError"); if (typeof callback !== "function") throw new TypeError("toBlob callback must be callable"); const normalized = String(type).toLowerCase(); const result = normalized === "image/png" ? new BlobNative([new Uint8Array(nativeCanvasPngBytes(nativeCanvasSurfaceForEntry(entry)))], { type: normalized }) : null; setTimeoutNative(() => callback(result), 0); } },
      transferControlToOffscreen: { configurable: false, enumerable: true, value() { if (transferred || context) throw nativeCanvasError("canvas control is already in use", "InvalidStateError"); const offscreen = Object.create(OffscreenCanvasNative.prototype); Object.defineProperties(offscreen, { __glassCanvasEntry: { configurable: false, enumerable: false, value: () => entry }, __glassCanvasSurface: { configurable: false, enumerable: false, get() { return nativeCanvasSurfaceForEntry(entry); } }, __glassCanvasDetached: { configurable: false, enumerable: false, writable: true, value: false }, nodeIndex: { configurable: false, enumerable: false, value: Number(element.nodeIndex) } }); transferred = offscreen; return offscreen; } },
      getContextAttributes: { configurable: false, enumerable: true, value() { return { alpha: true, desynchronized: false, willReadFrequently: false }; } },
    });
    return reset;
  };
  const nativeCanvasPngCrcTable = globalThis.__glassCanvasPngCrcTable || (() => { const table = []; for (let value = 0; value < 256; value += 1) { let crc = value; for (let bit = 0; bit < 8; bit += 1) crc = (crc & 1) ? 0xedb88320 ^ (crc >>> 1) : crc >>> 1; table.push(crc >>> 0); } return table; })();
  globalThis.__glassCanvasPngCrcTable = nativeCanvasPngCrcTable;
  const nativeCanvasPngCrc = (bytes) => { let crc = 0xffffffff; for (const value of bytes) crc = nativeCanvasPngCrcTable[(crc ^ value) & 255] ^ (crc >>> 8); return (crc ^ 0xffffffff) >>> 0; };
  const nativeCanvasPngChunk = (type, bytes) => { const result = []; const length = bytes.length; result.push((length >>> 24) & 255, (length >>> 16) & 255, (length >>> 8) & 255, length & 255); const typeBytes = Array.from(type).map((value) => value.charCodeAt(0)); result.push(...typeBytes, ...bytes); const crc = nativeCanvasPngCrc(typeBytes.concat(bytes)); result.push((crc >>> 24) & 255, (crc >>> 16) & 255, (crc >>> 8) & 255, crc & 255); return result; };
  const nativeCanvasPngBytes = (surface) => {
    nativeCanvasRequireOriginClean(surface);
    const raw = [];
    for (let row = 0; row < surface.height; row += 1) {
      raw.push(0, ...surface.pixels.slice(row * surface.width * 4, (row + 1) * surface.width * 4));
    }
    const compressed = [0x78, 0x01];
    for (let offset = 0; offset < raw.length; offset += 65535) {
      const length = Math.min(65535, raw.length - offset);
      const final = offset + length >= raw.length;
      compressed.push(final ? 1 : 0, length & 255, (length >>> 8) & 255, (~length) & 255, ((~length) >>> 8) & 255, ...raw.slice(offset, offset + length));
    }
    let a = 1;
    let b = 0;
    for (const value of raw) {
      a = (a + value) % 65521;
      b = (b + a) % 65521;
    }
    compressed.push((b >>> 8) & 255, b & 255, (a >>> 8) & 255, a & 255);
    const header = [137, 80, 78, 71, 13, 10, 26, 10];
    const ihdr = [(surface.width >>> 24) & 255, (surface.width >>> 16) & 255, (surface.width >>> 8) & 255, surface.width & 255, (surface.height >>> 24) & 255, (surface.height >>> 16) & 255, (surface.height >>> 8) & 255, surface.height & 255, 8, 6, 0, 0, 0];
    return header.concat(nativeCanvasPngChunk("IHDR", ihdr), nativeCanvasPngChunk("IDAT", compressed), nativeCanvasPngChunk("IEND", []));
  };
"###;
