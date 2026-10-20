pub(in crate::native_engine::javascript) const NATIVE_MESSAGE_CHANNEL_BOOTSTRAP: &str = r###"
  const glassMessagePortQueueLimit = __GLASS_MESSAGE_PORT_QUEUE_LIMIT__;
  const glassMessageChannelNameLimit = __GLASS_MESSAGE_CHANNEL_NAME_LIMIT__;
  if (typeof globalThis.__glassNativeImageBitmapIsImageBitmap !== "function") {
    const GlassImageBitmapNative = globalThis.__glassImageBitmapConstructor
      || function ImageBitmap() { throw new TypeError("ImageBitmap cannot be constructed directly"); };
    globalThis.__glassImageBitmapConstructor = GlassImageBitmapNative;
    globalThis.ImageBitmap = GlassImageBitmapNative;
    const makeImageBitmap = (surface) => {
      const bitmap = Object.create(GlassImageBitmapNative.prototype);
      Object.defineProperty(bitmap, "__glassImageBitmapSurface", {
        configurable: false,
        enumerable: false,
        writable: true,
        value: surface,
      });
      return bitmap;
    };
    Object.defineProperties(GlassImageBitmapNative.prototype, {
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
    globalThis.__glassNativeImageBitmapIsImageBitmap = (value) => {
      try { return value instanceof GlassImageBitmapNative; }
      catch (_error) { return false; }
    };
    globalThis.__glassNativeImageBitmapTransferDescriptor = (value) => {
      const surface = value && value.__glassImageBitmapSurface;
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
      if (!value || !value.__glassImageBitmapSurface) return false;
      value.__glassImageBitmapSurface = null;
      return true;
    };
    globalThis.__glassNativeImageBitmapFromTransfer = (descriptor) => makeImageBitmap({
      width: Number(descriptor.width),
      height: Number(descriptor.height),
      pixels: Array.isArray(descriptor.pixels) ? descriptor.pixels.slice() : [],
      originClean: descriptor.originClean !== false,
    });
    globalThis.__glassNativeImageBitmapFromSurface = (surface) => makeImageBitmap({
      width: Number(surface && surface.width),
      height: Number(surface && surface.height),
      pixels: Array.isArray(surface && surface.pixels) ? surface.pixels.slice() : [],
      originClean: !surface || surface.originClean !== false,
    });
  }
  if (typeof globalThis.__glassNativeOffscreenCanvasIsOffscreenCanvas !== "function") {
    const GlassOffscreenCanvasNative = globalThis.__glassOffscreenCanvasConstructor
      || function OffscreenCanvas(width, height) {
        if (!new.target) throw new TypeError("OffscreenCanvas must be constructed with new");
        const normalizedWidth = Number(width);
        const normalizedHeight = Number(height);
        if (!Number.isSafeInteger(normalizedWidth) || !Number.isSafeInteger(normalizedHeight)
            || normalizedWidth < 0 || normalizedHeight < 0
            || normalizedWidth > 4096 || normalizedHeight > 4096
            || normalizedWidth * normalizedHeight > 262144)
          throw new RangeError("OffscreenCanvas dimensions are outside the native limit");
        Object.defineProperty(this, "__glassCanvasSurface", {
          configurable: false,
          enumerable: false,
          writable: true,
          value: {
            width: normalizedWidth,
            height: normalizedHeight,
            pixels: new Array(normalizedWidth * normalizedHeight * 4).fill(0),
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
    const workerOffscreenSurface = (value) => {
      if (!(value instanceof GlassOffscreenCanvasNative) || value.__glassCanvasDetached === true)
        return null;
      return value.__glassCanvasSurface || null;
    };
    const workerCanvasSourceSurface = (value) => {
      if (value && value.__glassImageBitmapSurface) return value.__glassImageBitmapSurface;
      return workerOffscreenSurface(value);
    };
    const workerCanvasSurface = (context) => {
      const surface = workerOffscreenSurface(context.__glassCanvas);
      if (!surface) throw new DOMException("OffscreenCanvas is detached", "InvalidStateError");
      return surface;
    };
    const workerCanvasColor = (value) => {
      const text = String(value || "").trim().toLowerCase();
      const match = text.match(/^#([0-9a-f]{6})$/);
      if (!match) return [0, 0, 0, 255];
      return [
        Number.parseInt(match[1].slice(0, 2), 16),
        Number.parseInt(match[1].slice(2, 4), 16),
        Number.parseInt(match[1].slice(4, 6), 16),
        255,
      ];
    };
    const WorkerCanvasRenderingContext2DNative = function CanvasRenderingContext2D() {
      throw new TypeError("CanvasRenderingContext2D cannot be constructed directly");
    };
    const workerCanvasImageData = (width, height, data) => ({
      data: data || new Uint8ClampedArray(width * height * 4),
      width,
      height,
    });
    const workerCanvasContext = (canvas) => {
      const context = Object.create(WorkerCanvasRenderingContext2DNative.prototype);
      Object.defineProperty(context, "__glassCanvas", { configurable: false, enumerable: false, value: canvas });
      context.fillStyle = "#000000";
      context.clearRect = (x, y, width, height) => {
        const surface = workerCanvasSurface(context);
        const left = Math.max(0, Math.floor(Number(x)));
        const top = Math.max(0, Math.floor(Number(y)));
        const right = Math.min(surface.width, Math.ceil(Number(x) + Number(width)));
        const bottom = Math.min(surface.height, Math.ceil(Number(y) + Number(height)));
        for (let row = top; row < bottom; row += 1) {
          for (let column = left; column < right; column += 1) {
            const index = (row * surface.width + column) * 4;
            surface.pixels[index] = 0;
            surface.pixels[index + 1] = 0;
            surface.pixels[index + 2] = 0;
            surface.pixels[index + 3] = 0;
          }
        }
      };
      context.fillRect = (x, y, width, height) => {
        const surface = workerCanvasSurface(context);
        const color = workerCanvasColor(context.fillStyle);
        const left = Math.max(0, Math.floor(Number(x)));
        const top = Math.max(0, Math.floor(Number(y)));
        const right = Math.min(surface.width, Math.ceil(Number(x) + Number(width)));
        const bottom = Math.min(surface.height, Math.ceil(Number(y) + Number(height)));
        for (let row = top; row < bottom; row += 1) {
          for (let column = left; column < right; column += 1) {
            const index = (row * surface.width + column) * 4;
            surface.pixels[index] = color[0];
            surface.pixels[index + 1] = color[1];
            surface.pixels[index + 2] = color[2];
            surface.pixels[index + 3] = color[3];
          }
        }
      };
      context.getImageData = (x, y, width, height) => {
        const surface = workerCanvasSurface(context);
        const normalizedWidth = Number(width);
        const normalizedHeight = Number(height);
        if (!Number.isSafeInteger(normalizedWidth) || !Number.isSafeInteger(normalizedHeight)
            || normalizedWidth < 0 || normalizedHeight < 0)
          throw new RangeError("ImageData dimensions are invalid");
        const data = new Uint8ClampedArray(normalizedWidth * normalizedHeight * 4);
        for (let row = 0; row < normalizedHeight; row += 1) {
          for (let column = 0; column < normalizedWidth; column += 1) {
            const sourceX = Math.floor(Number(x)) + column;
            const sourceY = Math.floor(Number(y)) + row;
            const targetIndex = (row * normalizedWidth + column) * 4;
            if (sourceX < 0 || sourceY < 0 || sourceX >= surface.width || sourceY >= surface.height) continue;
            const sourceIndex = (sourceY * surface.width + sourceX) * 4;
            for (let channel = 0; channel < 4; channel += 1) data[targetIndex + channel] = surface.pixels[sourceIndex + channel];
          }
        }
        return workerCanvasImageData(normalizedWidth, normalizedHeight, data);
      };
      context.putImageData = (image, x, y) => {
        const surface = workerCanvasSurface(context);
        const width = Number(image && image.width);
        const height = Number(image && image.height);
        const data = image && image.data;
        if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || !data
            || data.length !== width * height * 4)
          throw new TypeError("ImageData is invalid");
        for (let row = 0; row < height; row += 1) {
          for (let column = 0; column < width; column += 1) {
            const targetX = Math.floor(Number(x)) + column;
            const targetY = Math.floor(Number(y)) + row;
            if (targetX < 0 || targetY < 0 || targetX >= surface.width || targetY >= surface.height) continue;
            const sourceIndex = (row * width + column) * 4;
            const targetIndex = (targetY * surface.width + targetX) * 4;
            for (let channel = 0; channel < 4; channel += 1) surface.pixels[targetIndex + channel] = data[sourceIndex + channel];
          }
        }
      };
      context.createImageData = (width, height) => workerCanvasImageData(Number(width), Number(height));
      context.drawImage = (source, x, y) => {
        const sourceSurface = workerCanvasSourceSurface(source);
        const target = workerCanvasSurface(context);
        if (!sourceSurface) throw new DOMException("image source is invalid", "InvalidStateError");
        const offsetX = Math.floor(Number(x));
        const offsetY = Math.floor(Number(y));
        for (let row = 0; row < sourceSurface.height; row += 1) {
          for (let column = 0; column < sourceSurface.width; column += 1) {
            const targetX = offsetX + column;
            const targetY = offsetY + row;
            if (targetX < 0 || targetY < 0 || targetX >= target.width || targetY >= target.height) continue;
            const sourceIndex = (row * sourceSurface.width + column) * 4;
            const targetIndex = (targetY * target.width + targetX) * 4;
            for (let channel = 0; channel < 4; channel += 1) target.pixels[targetIndex + channel] = sourceSurface.pixels[sourceIndex + channel];
          }
        }
      };
      return context;
    };
    globalThis.__glassOffscreenCanvasConstructor = GlassOffscreenCanvasNative;
    globalThis.OffscreenCanvas = GlassOffscreenCanvasNative;
    Object.defineProperties(GlassOffscreenCanvasNative.prototype, {
      width: { configurable: true, enumerable: true, get() { return workerOffscreenSurface(this).width; }, set(value) { const surface = workerOffscreenSurface(this); const width = Number(value); if (!Number.isSafeInteger(width) || width < 0 || width > 4096 || width * surface.height > 262144) throw new RangeError("OffscreenCanvas width is invalid"); surface.width = width; surface.pixels = new Array(width * surface.height * 4).fill(0); } },
      height: { configurable: true, enumerable: true, get() { return workerOffscreenSurface(this).height; }, set(value) { const surface = workerOffscreenSurface(this); const height = Number(value); if (!Number.isSafeInteger(height) || height < 0 || height > 4096 || surface.width * height > 262144) throw new RangeError("OffscreenCanvas height is invalid"); surface.height = height; surface.pixels = new Array(surface.width * height * 4).fill(0); } },
      getContext: { configurable: true, enumerable: true, value(type) { workerOffscreenSurface(this); if (String(type).toLowerCase() !== "2d") return null; if (!this.__glassCanvasContext) Object.defineProperty(this, "__glassCanvasContext", { configurable: false, enumerable: false, value: workerCanvasContext(this) }); return this.__glassCanvasContext; } },
      transferToImageBitmap: { configurable: true, enumerable: true, value() { const surface = workerOffscreenSurface(this); return globalThis.__glassNativeImageBitmapFromSurface(surface); } },
    });
    globalThis.__glassNativeOffscreenCanvasIsOffscreenCanvas = (value) => {
      try { return value instanceof GlassOffscreenCanvasNative; }
      catch (_error) { return false; }
    };
    globalThis.__glassNativeOffscreenCanvasTransferDescriptor = (value) => {
      const surface = workerOffscreenSurface(value);
      if (!surface) throw new TypeError("OffscreenCanvas is detached");
      return { type: "offscreencanvas", width: surface.width, height: surface.height, pixels: surface.pixels.slice(), originClean: surface.originClean !== false };
    };
    globalThis.__glassNativeOffscreenCanvasDetach = (value) => {
      if (!workerOffscreenSurface(value)) return false;
      value.__glassCanvasDetached = true;
      return true;
    };
    globalThis.__glassNativeOffscreenCanvasFromTransfer = (descriptor) => {
      const canvas = Object.create(GlassOffscreenCanvasNative.prototype);
      Object.defineProperty(canvas, "__glassCanvasSurface", { configurable: false, enumerable: false, writable: true, value: { width: Number(descriptor.width), height: Number(descriptor.height), pixels: descriptor.pixels.slice(), originClean: descriptor.originClean !== false } });
      Object.defineProperty(canvas, "__glassCanvasDetached", { configurable: false, enumerable: false, writable: true, value: false });
      Object.defineProperty(canvas, "nodeIndex", { configurable: false, enumerable: false, value: null });
      return canvas;
    };
  }
  const glassMessageImageBitmapIsImageBitmap = globalThis.__glassNativeImageBitmapIsImageBitmap;
  const glassMessageImageBitmapTransferDescriptor = globalThis.__glassNativeImageBitmapTransferDescriptor;
  const glassMessageImageBitmapDetach = globalThis.__glassNativeImageBitmapDetach;
  const glassMessageImageBitmapFromTransfer = globalThis.__glassNativeImageBitmapFromTransfer;
  const glassMessageOffscreenCanvasIsOffscreenCanvas = globalThis.__glassNativeOffscreenCanvasIsOffscreenCanvas;
  const glassMessageOffscreenCanvasTransferDescriptor = globalThis.__glassNativeOffscreenCanvasTransferDescriptor;
  const glassMessageOffscreenCanvasDetach = globalThis.__glassNativeOffscreenCanvasDetach;
  const glassMessageOffscreenCanvasFromTransfer = globalThis.__glassNativeOffscreenCanvasFromTransfer;
  const glassMessageArrayBufferIsAttached = globalThis.__glassNativeArrayBufferIsAttached;
  const glassMessageDetachArrayBuffer = globalThis.__glassNativeDetachArrayBuffer;
  const glassMessagePortRegistry = globalThis.__glassMessagePortRegistry instanceof Map
    ? globalThis.__glassMessagePortRegistry
    : new Map();
  const glassMessageBridgeRegistry = globalThis.__glassMessageBridgeRegistry instanceof Map
    ? globalThis.__glassMessageBridgeRegistry
    : new Map();
  const glassMessageRealmKey = String(globalThis.__glassMessageRealmKey || "native");
  const glassBroadcastChannelRegistry = globalThis.__glassBroadcastChannelRegistry instanceof Map
    ? globalThis.__glassBroadcastChannelRegistry
    : new Map();
  let glassNextMessagePortId = Number.isSafeInteger(globalThis.__glassNextMessagePortId)
    ? globalThis.__glassNextMessagePortId
    : 1;
  const glassMessageHidden = (target, name, value) => {
    Object.defineProperty(target, name, {
      configurable: false,
      enumerable: false,
      writable: true,
      value,
    });
  };
  const glassMessageException = (message, name) => {
    const Exception = typeof globalThis.DOMException === "function"
      ? globalThis.DOMException
      : Error;
    try { return new Exception(message, name); } catch (_error) { return new Error(message); }
  };
  const glassMessageCloneMarker = "glass-native-structured-clone-v1";
  const glassMessageCloneNodeLimit = __GLASS_MESSAGE_BYTES_LIMIT__;
  const glassMessageTypedArrayNames = new Set([
    "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array", "Uint16Array",
    "Int32Array", "Uint32Array", "Float32Array", "Float64Array",
    "BigInt64Array", "BigUint64Array",
  ]);
  const glassMessageOwn = (target, key, value) => {
    Object.defineProperty(target, key, {
      configurable: true,
      enumerable: true,
      writable: true,
      value,
    });
  };
  const glassMessageUtf8Bytes = (text) => {
    const bytes = [];
    for (let index = 0; index < text.length; index += 1) {
      let code = text.charCodeAt(index);
      if (code >= 0xd800 && code <= 0xdbff && index + 1 < text.length) {
        const low = text.charCodeAt(index + 1);
        if (low >= 0xdc00 && low <= 0xdfff) {
          code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
          index += 1;
        }
      } else if (code >= 0xd800 && code <= 0xdfff) {
        code = 0xfffd;
      }
      if (code <= 0x7f) bytes.push(code);
      else if (code <= 0x7ff) bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
      else if (code <= 0xffff) bytes.push(
        0xe0 | (code >> 12),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
      else bytes.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    }
    return bytes;
  };
  const glassMessagePortValue = (value) => {
    try {
      const id = Number(value && value.__glassMessagePortId);
      return Number.isSafeInteger(id) && glassMessagePortRegistry.get(id) === value;
    } catch (_error) {
      return false;
    }
  };
  const glassMessageImageBitmapValue = (value) => {
    try {
      return typeof glassMessageImageBitmapIsImageBitmap === "function"
        && glassMessageImageBitmapIsImageBitmap(value) === true;
    } catch (_error) {
      return false;
    }
  };
  const glassMessageOffscreenCanvasValue = (value) => {
    try {
      return typeof glassMessageOffscreenCanvasIsOffscreenCanvas === "function"
        && glassMessageOffscreenCanvasIsOffscreenCanvas(value) === true;
    } catch (_error) {
      return false;
    }
  };
  const glassMessageReadableStreamValue = (value) => {
    try {
      const hook = globalThis.__glassNativeReadableStreamIsReadableStream;
      return typeof hook === "function" && hook(value) === true;
    } catch (_error) {
      return false;
    }
  };
  const glassMessageReadableStreamByteMode = (value) => {
    try {
      const hook = globalThis.__glassNativeReadableStreamIsByteStream;
      return typeof hook === "function" && hook(value) === true;
    } catch (_error) {
      return false;
    }
  };
  const glassMessageNumberDescriptor = (value) => {
    if (Number.isNaN(value)) return { type: "number", value: "nan" };
    if (value === Infinity) return { type: "number", value: "infinity" };
    if (value === -Infinity) return { type: "number", value: "negativeInfinity" };
    if (Object.is(value, -0)) return { type: "number", value: "negativeZero" };
    return { type: "number", value };
  };
  const glassMessageNormalizeBytes = (input, state) => {
    if (!Array.isArray(input))
      throw glassMessageException("binary message data is invalid", "DataCloneError");
    if (input.length > glassMessageCloneNodeLimit - state.binaryBytes)
      throw new RangeError("native message data exceeds its limit");
    const bytes = input.slice();
    for (const byte of bytes) {
      if (!Number.isInteger(byte) || byte < 0 || byte > 255)
        throw glassMessageException("binary message data is invalid", "DataCloneError");
    }
    state.binaryBytes += bytes.length;
    return bytes;
  };
  const glassMessageBlobBytes = (value, state) => {
    if (Array.isArray(value && value._bytes))
      return glassMessageNormalizeBytes(value._bytes, state);
    if (typeof (value && value._text) === "string")
      return glassMessageNormalizeBytes(glassMessageUtf8Bytes(value._text), state);
    throw glassMessageException("Blob data could not be cloned", "DataCloneError");
  };
  const glassMessageArrayIndex = (key, length) => {
    if (typeof key !== "string" || key === "") return false;
    const index = Number(key);
    return Number.isInteger(index) && index >= 0 && index < length
      && String(index) === key;
  };
  const glassMessageEncodeGraph = (value, transfers, transferValues) => {
    const nodes = [];
    const seen = new Map();
    const state = { binaryBytes: 0 };
    const reserve = () => {
      if (nodes.length >= glassMessageCloneNodeLimit)
        throw new RangeError("native message data exceeds its limit");
      const id = nodes.length;
      nodes.push(null);
      return id;
    };
    const encodeProperties = (value, skip) => {
      const properties = [];
      for (const key of Object.keys(value)) {
        if (skip(key)) continue;
        properties.push([key, encode(value[key])]);
      }
      return properties;
    };
    const encode = (current) => {
      if (current === undefined) return { type: "undefined" };
      if (current === null) return { type: "null" };
      const kind = typeof current;
      if (kind === "boolean") return { type: "boolean", value: current };
      if (kind === "string") {
        if (current.length > glassMessageCloneNodeLimit)
          throw new RangeError("native message data exceeds its limit");
        return { type: "string", value: current };
      }
      if (kind === "number") return glassMessageNumberDescriptor(current);
      if (kind === "bigint") return { type: "bigint", value: String(current) };
      if (kind === "function" || kind === "symbol")
        throw glassMessageException("message could not be cloned", "DataCloneError");
      if (kind !== "object")
        throw glassMessageException("message could not be cloned", "DataCloneError");
      if (transfers && transfers.has(current)) {
        const transfer = transfers.get(current);
        if (typeof transfer === "number") return { type: "port", index: transfer };
        if (transfer && (transfer.kind === "imageBitmap"
              || transfer.kind === "offscreenCanvas"
              || transfer.kind === "readableStream")
            && Number.isSafeInteger(transfer.index))
          return { type: "transfer", index: transfer.index };
        throw glassMessageException("message transfer reference is invalid", "DataCloneError");
      }
      if (glassMessagePortValue(current))
        throw glassMessageException("MessagePort is not in the transfer list", "DataCloneError");
      if (glassMessageImageBitmapValue(current))
        throw glassMessageException("ImageBitmap is not in the transfer list", "DataCloneError");
      if (glassMessageOffscreenCanvasValue(current))
        throw glassMessageException("OffscreenCanvas is not in the transfer list", "DataCloneError");
      if (glassMessageReadableStreamValue(current))
        throw glassMessageException("ReadableStream is not in the transfer list", "DataCloneError");
      if (seen.has(current)) return { ref: seen.get(current) };

      const id = reserve();
      seen.set(current, id);
      let tag;
      try { tag = Object.prototype.toString.call(current); }
      catch (_error) { throw glassMessageException("message could not be cloned", "DataCloneError"); }

      if (tag === "[object SharedArrayBuffer]")
        throw glassMessageException("SharedArrayBuffer is not supported by the native message bridge", "DataCloneError");
      if (tag === "[object ArrayBuffer]") {
        if (typeof glassMessageArrayBufferIsAttached !== "function"
            || glassMessageArrayBufferIsAttached(current) !== true)
          throw glassMessageException("ArrayBuffer is detached", "DataCloneError");
        let bytes;
        try { bytes = Array.from(new Uint8Array(current)); }
        catch (_error) { throw glassMessageException("ArrayBuffer could not be cloned", "DataCloneError"); }
        nodes[id] = { type: "arraybuffer", bytes: glassMessageNormalizeBytes(bytes, state) };
        return { ref: id };
      }
      if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(current)) {
        const viewTag = tag.slice(8, -1);
        if (tag === "[object DataView]") {
          if (Object.prototype.toString.call(current.buffer) === "[object SharedArrayBuffer]")
            throw glassMessageException("SharedArrayBuffer is not supported by the native message bridge", "DataCloneError");
          nodes[id] = {
            type: "dataview",
            buffer: encode(current.buffer),
            byteOffset: current.byteOffset,
            byteLength: current.byteLength,
          };
          return { ref: id };
        }
        if (glassMessageTypedArrayNames.has(viewTag)) {
          if (Object.prototype.toString.call(current.buffer) === "[object SharedArrayBuffer]")
            throw glassMessageException("SharedArrayBuffer is not supported by the native message bridge", "DataCloneError");
          nodes[id] = {
            type: "typedarray",
            constructor: viewTag,
            buffer: encode(current.buffer),
            byteOffset: current.byteOffset,
            length: current.length,
          };
          return { ref: id };
        }
        throw glassMessageException("typed array could not be cloned", "DataCloneError");
      }
      if (tag === "[object Date]") {
        nodes[id] = { type: "date", value: encode(current.getTime()) };
        return { ref: id };
      }
      if (tag === "[object RegExp]") {
        nodes[id] = {
          type: "regexp",
          source: String(current.source),
          flags: String(current.flags),
          lastIndex: encode(current.lastIndex),
        };
        return { ref: id };
      }
      if (tag === "[object Map]") {
        const node = { type: "map", entries: [] };
        nodes[id] = node;
        try {
          current.forEach((mapValue, mapKey) => node.entries.push([encode(mapKey), encode(mapValue)]));
        } catch (_error) {
          throw glassMessageException("Map could not be cloned", "DataCloneError");
        }
        return { ref: id };
      }
      if (tag === "[object Set]") {
        const node = { type: "set", values: [] };
        nodes[id] = node;
        try { current.forEach(item => node.values.push(encode(item))); }
        catch (_error) { throw glassMessageException("Set could not be cloned", "DataCloneError"); }
        return { ref: id };
      }
      if (tag === "[object Error]" || tag === "[object DOMException]") {
        const node = {
          type: "error",
          name: String(current.name || "Error"),
          message: String(current.message || ""),
          stack: typeof current.stack === "string" ? current.stack : null,
        };
        nodes[id] = node;
        if (Object.prototype.hasOwnProperty.call(current, "cause")) node.cause = encode(current.cause);
        return { ref: id };
      }
      if (current.__glassNativeBlob === true || current.__glassWorkerBlob === true) {
        const node = {
          type: current.__glassNativeFile === true || current.__glassWorkerFile === true ? "file" : "blob",
          bytes: glassMessageBlobBytes(current, state),
          mime: String(current.type || ""),
        };
        if (node.type === "file") {
          node.name = String(current.name || "");
          node.lastModified = encode(Number(current.lastModified) || 0);
        }
        nodes[id] = node;
        return { ref: id };
      }
      if (tag === "[object Promise]" || tag === "[object WeakMap]" || tag === "[object WeakSet]")
        throw glassMessageException("message could not be cloned", "DataCloneError");
      if (tag === "[object Number]" || tag === "[object String]" || tag === "[object Boolean]") {
        nodes[id] = { type: "boxed", value: encode(current.valueOf()) };
        return { ref: id };
      }
      const node = { type: Array.isArray(current) ? "array" : "object" };
      nodes[id] = node;
      if (node.type === "array") {
        if (!Number.isSafeInteger(current.length) || current.length > glassMessageCloneNodeLimit)
          throw new RangeError("native message data exceeds its limit");
        node.length = current.length;
        node.items = [];
        for (let index = 0; index < current.length; index += 1) {
          node.items.push(Object.prototype.hasOwnProperty.call(current, index)
            ? encode(current[index])
            : { type: "hole" });
        }
        node.properties = encodeProperties(current, key => glassMessageArrayIndex(key, current.length));
      } else {
        node.properties = encodeProperties(current, () => false);
      }
      return { ref: id };
    };
    return {
      __glassMessageClone: glassMessageCloneMarker,
      root: encode(value),
      nodes,
      transfer_values: Array.isArray(transferValues) ? transferValues : [],
    };
  };
  const glassMessageEncodeMessage = (value, transfers, transferValues) => {
    const graph = glassMessageEncodeGraph(value, transfers || new Map(), transferValues);
    let encoded;
    try { encoded = JSON.stringify(graph); }
    catch (_error) { throw glassMessageException("message could not be cloned", "DataCloneError"); }
    if (encoded === undefined || encoded.length > __GLASS_MESSAGE_BYTES_LIMIT__)
      throw new RangeError("native message data exceeds its limit");
    return graph;
  };
  const glassMessageClone = (value) => glassMessageDecodeClone(
    glassMessageEncodeMessage(value, new Map()),
    [],
  );
  const glassMessageTransferList = (options) => {
    const transfer = options === undefined
      ? []
      : Array.isArray(options)
        ? options
        : options && typeof options === "object" && options.transfer !== undefined
          ? options.transfer
          : null;
    if (!Array.isArray(transfer))
      throw glassMessageException("message transfer list must be an array", "TypeError");
    if (transfer.length > glassMessagePortQueueLimit)
      throw new RangeError("native message transfer list exceeds its limit");
    return transfer;
  };
  const glassMessageValidateArrayBufferTransfer = (value) => {
    let tag;
    try { tag = Object.prototype.toString.call(value); }
    catch (_error) { tag = ""; }
    if (tag !== "[object ArrayBuffer]"
        || typeof glassMessageArrayBufferIsAttached !== "function"
        || glassMessageArrayBufferIsAttached(value) !== true)
      throw glassMessageException("ArrayBuffer is not transferable", "DataCloneError");
  };
  const glassMessageValidateImageBitmapDescriptor = (descriptor, expectedType = "imagebitmap", label = "ImageBitmap") => {
    if (!descriptor || typeof descriptor !== "object" || descriptor.type !== expectedType)
      throw glassMessageException(label + " transfer descriptor is invalid", "DataCloneError");
    const width = Number(descriptor.width);
    const height = Number(descriptor.height);
    if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height)
        || width < 1 || height < 1 || width > 4096 || height > 4096
        || width * height > glassMessageCloneNodeLimit)
      throw glassMessageException(label + " dimensions are invalid", "DataCloneError");
    if (!Array.isArray(descriptor.pixels)
        || descriptor.pixels.length !== width * height * 4
        || descriptor.pixels.length > glassMessageCloneNodeLimit)
      throw glassMessageException(label + " pixels are invalid", "DataCloneError");
    for (const byte of descriptor.pixels) {
      if (!Number.isInteger(byte) || byte < 0 || byte > 255)
        throw glassMessageException(label + " pixels are invalid", "DataCloneError");
    }
    return {
      type: expectedType,
      width,
      height,
      pixels: descriptor.pixels.slice(),
      originClean: descriptor.originClean !== false,
    };
  };
  const glassMessageImageBitmapTransfer = (value) => {
    if (!glassMessageImageBitmapValue(value)) return null;
    if (typeof glassMessageImageBitmapTransferDescriptor !== "function")
      throw glassMessageException("ImageBitmap transfer is unavailable", "DataCloneError");
    let descriptor;
    try { descriptor = glassMessageImageBitmapTransferDescriptor(value); }
    catch (_error) { throw glassMessageException("ImageBitmap is not transferable", "DataCloneError"); }
    return glassMessageValidateImageBitmapDescriptor(descriptor);
  };
  const glassMessageOffscreenCanvasTransfer = (value) => {
    if (!glassMessageOffscreenCanvasValue(value)) return null;
    if (typeof glassMessageOffscreenCanvasTransferDescriptor !== "function")
      throw glassMessageException("OffscreenCanvas transfer is unavailable", "DataCloneError");
    let descriptor;
    try { descriptor = glassMessageOffscreenCanvasTransferDescriptor(value); }
    catch (_error) { throw glassMessageException("OffscreenCanvas is not transferable", "DataCloneError"); }
    return glassMessageValidateImageBitmapDescriptor(descriptor, "offscreencanvas", "OffscreenCanvas");
  };
  const glassMessageCommitImageBitmapTransfers = (bitmaps) => {
    for (const bitmap of bitmaps) {
      let detached = false;
      try {
        detached = typeof glassMessageImageBitmapDetach === "function"
          && glassMessageImageBitmapDetach(bitmap) === true;
      } catch (_error) {}
      if (!detached)
        throw glassMessageException("ImageBitmap could not be detached", "DataCloneError");
    }
  };
  const glassMessageCommitOffscreenCanvasTransfers = (canvases) => {
    for (const canvas of canvases) {
      let detached = false;
      try {
        detached = typeof glassMessageOffscreenCanvasDetach === "function"
          && glassMessageOffscreenCanvasDetach(canvas) === true;
      } catch (_error) {}
      if (!detached)
        throw glassMessageException("OffscreenCanvas could not be detached", "DataCloneError");
    }
  };
  const glassMessageDecodeImageBitmapTransfer = (descriptor) => {
    const normalized = glassMessageValidateImageBitmapDescriptor(descriptor);
    if (typeof glassMessageImageBitmapFromTransfer !== "function")
      throw glassMessageException("ImageBitmap transfer is unavailable", "DataCloneError");
    let result;
    try { result = glassMessageImageBitmapFromTransfer(normalized); }
    catch (_error) { throw glassMessageException("ImageBitmap transfer is invalid", "DataCloneError"); }
    if (!glassMessageImageBitmapValue(result))
      throw glassMessageException("ImageBitmap transfer result is invalid", "DataCloneError");
    return result;
  };
  const glassMessageDecodeOffscreenCanvasTransfer = (descriptor) => {
    const normalized = glassMessageValidateImageBitmapDescriptor(
      descriptor,
      "offscreencanvas",
      "OffscreenCanvas",
    );
    if (typeof glassMessageOffscreenCanvasFromTransfer !== "function")
      throw glassMessageException("OffscreenCanvas transfer is unavailable", "DataCloneError");
    let result;
    try { result = glassMessageOffscreenCanvasFromTransfer(normalized); }
    catch (_error) { throw glassMessageException("OffscreenCanvas transfer is invalid", "DataCloneError"); }
    if (!glassMessageOffscreenCanvasValue(result))
      throw glassMessageException("OffscreenCanvas transfer result is invalid", "DataCloneError");
    return result;
  };
  const glassMessageDecodeReadableStreamTransfer = (descriptor, ports) => {
    if (!descriptor || typeof descriptor !== "object"
        || descriptor.type !== "readablestream"
        || !Number.isSafeInteger(descriptor.port_index)
        || descriptor.port_index < 0
        || typeof descriptor.byte_mode !== "boolean"
        || !ports || !ports[descriptor.port_index]
        || typeof ports[descriptor.port_index].postMessage !== "function")
      throw glassMessageException("ReadableStream transfer descriptor is invalid", "DataCloneError");
    const factory = globalThis.__glassNativeReadableStreamFromTransfer;
    if (typeof factory !== "function")
      throw glassMessageException("ReadableStream transfer is unavailable", "DataCloneError");
    let result;
    try { result = factory(descriptor, ports[descriptor.port_index]); }
    catch (_error) { throw glassMessageException("ReadableStream transfer is invalid", "DataCloneError"); }
    if (!glassMessageReadableStreamValue(result))
      throw glassMessageException("ReadableStream transfer result is invalid", "DataCloneError");
    return result;
  };
  const glassMessageDecodeTransferValue = (descriptor, ports) => {
    if (descriptor && descriptor.type === "offscreencanvas")
      return glassMessageDecodeOffscreenCanvasTransfer(descriptor);
    if (descriptor && descriptor.type === "readablestream")
      return glassMessageDecodeReadableStreamTransfer(descriptor, ports);
    return glassMessageDecodeImageBitmapTransfer(descriptor);
  };
  const glassMessageCommitArrayBufferTransfers = (buffers) => {
    for (const buffer of buffers) {
      let detached = false;
      try {
        detached = typeof glassMessageDetachArrayBuffer === "function"
          && glassMessageDetachArrayBuffer(buffer) === true;
      } catch (_error) {}
      if (!detached)
        throw glassMessageException("ArrayBuffer could not be detached", "DataCloneError");
    }
  };
  const glassMessageTransferDescriptor = (port) => {
    if (!port || typeof port !== "object")
      throw glassMessageException("transfer list member is not a MessagePort", "DataCloneError");
    const id = Number(port.__glassMessagePortId);
    if (!Number.isSafeInteger(id) || glassMessagePortRegistry.get(id) !== port
        || port.__glassMessageClosed || port.__glassMessagePortTransferred
        || port.__glassMessagePortBridgeKey) {
      throw glassMessageException("MessagePort is not transferable", "DataCloneError");
    }
    const peer = port.__glassMessagePortPeer;
    const peerId = peer && Number(peer.__glassMessagePortId);
    if (!peer || !Number.isSafeInteger(peerId) || peer.__glassMessageClosed
        || peer.__glassMessagePortTransferred || peer.__glassMessagePortBridgeKey) {
      throw glassMessageException("MessagePort is not entangled", "DataCloneError");
    }
    const bridgeKey = glassMessageRealmKey + ":port:" + String(id);
    if (bridgeKey.length > __GLASS_MESSAGE_CHANNEL_NAME_LIMIT__)
      throw new RangeError("native MessagePort bridge key exceeds its limit");
    if (glassMessageBridgeRegistry.has(bridgeKey))
      throw glassMessageException("MessagePort was already transferred", "DataCloneError");
    return {
      port,
      peer,
      id,
      bridgeKey,
      descriptor: { bridge_key: bridgeKey, port_id: id, peer_id: peerId },
    };
  };
  const glassMessageReadableStreamTransfer = (value, prepared) => {
    if (!glassMessageReadableStreamValue(value)) return null;
    if (value.locked === true)
      throw glassMessageException("ReadableStream is locked", "DataCloneError");
    if (typeof globalThis.MessageChannel !== "function")
      throw glassMessageException("ReadableStream transfer is unavailable", "DataCloneError");
    let channel;
    try { channel = new globalThis.MessageChannel(); }
    catch (_error) { throw glassMessageException("ReadableStream transfer is unavailable", "DataCloneError"); }
    let endpoint;
    try { endpoint = glassMessageTransferDescriptor(channel.port2); }
    catch (_error) { throw glassMessageException("ReadableStream transfer endpoint is invalid", "DataCloneError"); }
    endpoint.descriptor.hidden = true;
    const portIndex = prepared.length;
    prepared.push(endpoint);
    return {
      stream: value,
      port: channel.port1,
      descriptor: {
        type: "readablestream",
        port_index: portIndex,
        byte_mode: glassMessageReadableStreamByteMode(value),
      },
    };
  };
  const glassMessageCommitReadableStreamTransfers = (transfers) => {
    for (const transfer of transfers) {
      let reader;
      try { reader = transfer.stream.getReader(); }
      catch (_error) {
        throw glassMessageException("ReadableStream could not be transferred", "DataCloneError");
      }
      let reading = false;
      let closed = false;
      const send = (message) => {
        if (closed) return;
        try { transfer.port.postMessage(message); }
        catch (_error) { closed = true; }
      };
      transfer.port.onmessage = (event) => {
        if (closed || !event || !event.data || typeof event.data !== "object") return;
        const command = String(event.data.__glassReadableStreamControl || "");
        if (command === "pull") {
          if (reading) return;
          reading = true;
          Promise.resolve(reader.read()).then(
            result => {
              reading = false;
              if (closed) return;
              if (!result || result.done) {
                closed = true;
                try { transfer.port.postMessage({ __glassReadableStreamControl: "close" }); }
                catch (_error) {}
                return;
              }
              send({ __glassReadableStreamControl: "chunk", value: result.value });
            },
            error => {
              reading = false;
              if (closed) return;
              closed = true;
              try {
                transfer.port.postMessage({
                  __glassReadableStreamControl: "error",
                  message: error && error.message ? String(error.message) : String(error),
                });
              } catch (_error) {}
            },
          );
        } else if (command === "cancel") {
          closed = true;
          Promise.resolve(reader.cancel(event.data.reason)).catch(() => undefined);
        }
      };
      transfer.port.start();
    }
  };
  const glassMessageCommitTransfer = ({ port, peer, id, bridgeKey }) => {
    port.__glassMessagePortTransferred = true;
    port.__glassMessageClosed = true;
    port.__glassMessageQueue.length = 0;
    glassMessagePortRegistry.delete(id);
    peer.__glassMessagePortPeer = null;
    peer.__glassMessagePortBridgeKey = bridgeKey;
    glassMessageBridgeRegistry.set(bridgeKey, peer);
  };
  const glassMessageCloneWithTransfers = (value, transfer) => {
    const prepared = [];
    const buffers = [];
    const bitmaps = [];
    const offscreenCanvases = [];
    const readableStreams = [];
    const members = new Map();
    const transferValues = [];
    const transferMembers = new Set();
    for (const member of transfer) {
      if (transferMembers.has(member))
        throw glassMessageException("transfer list member appears more than once", "DataCloneError");
      transferMembers.add(member);
      let tag;
      try { tag = Object.prototype.toString.call(member); }
      catch (_error) { tag = ""; }
      if (tag === "[object ArrayBuffer]") {
        glassMessageValidateArrayBufferTransfer(member);
        buffers.push(member);
        continue;
      }
      const bitmap = glassMessageImageBitmapTransfer(member);
      if (bitmap) {
        const index = transferValues.length;
        transferValues.push(bitmap);
        bitmaps.push(member);
        members.set(member, { kind: "imageBitmap", index });
        continue;
      }
      const offscreenCanvas = glassMessageOffscreenCanvasTransfer(member);
      if (offscreenCanvas) {
        const index = transferValues.length;
        transferValues.push(offscreenCanvas);
        offscreenCanvases.push(member);
        members.set(member, { kind: "offscreenCanvas", index });
        continue;
      }
      const readableStream = glassMessageReadableStreamTransfer(member, prepared);
      if (readableStream) {
        const index = transferValues.length;
        transferValues.push(readableStream.descriptor);
        readableStreams.push(readableStream);
        members.set(member, { kind: "readableStream", index });
        continue;
      }
      if (members.has(member))
        throw glassMessageException("MessagePort appears more than once in the transfer list", "DataCloneError");
      const descriptor = glassMessageTransferDescriptor(member);
      if (members.has(descriptor.peer))
        throw glassMessageException("both endpoints of a MessageChannel cannot be transferred together", "DataCloneError");
      members.set(member, prepared.length);
      prepared.push(descriptor);
    }
    const data = glassMessageEncodeMessage(value, members, transferValues);
    glassMessageCommitArrayBufferTransfers(buffers);
    glassMessageCommitImageBitmapTransfers(bitmaps);
    glassMessageCommitOffscreenCanvasTransfers(offscreenCanvases);
    for (const descriptor of prepared) glassMessageCommitTransfer(descriptor);
    glassMessageCommitReadableStreamTransfers(readableStreams);
    return {
      data,
      transfer_ports: prepared.map(({ descriptor }) => descriptor),
    };
  };
  const glassMessageCloneWithArrayBufferTransfers = (value, transfer) => {
    const buffers = [];
    const seen = new Set();
    for (const member of transfer) {
      if (seen.has(member))
        throw glassMessageException("transfer list member appears more than once", "DataCloneError");
      seen.add(member);
      glassMessageValidateArrayBufferTransfer(member);
      buffers.push(member);
    }
    const data = glassMessageEncodeMessage(value, new Map());
    const result = glassMessageDecodeClone(data, []);
    glassMessageCommitArrayBufferTransfers(buffers);
    return result;
  };
  const glassMessageCommitLocalPortTransfer = ({ port, peer, id }, replacement) => {
    const queued = Array.isArray(port.__glassMessageQueue)
      ? port.__glassMessageQueue.splice(0)
      : [];
    const started = port.__glassMessageStarted === true;
    port.__glassMessagePortTransferred = true;
    port.__glassMessageClosed = true;
    port.__glassMessageQueue.length = 0;
    port.__glassMessagePortPeer = null;
    glassMessagePortRegistry.delete(id);
    peer.__glassMessagePortPeer = replacement;
    replacement.__glassMessagePortPeer = peer;
    replacement.__glassMessageQueue.push(...queued);
    replacement.__glassMessageStarted = started;
    if (started) glassMessageSchedule(replacement);
  };
  const glassMessageCloneWithLocalTransfers = (value, transfer) => {
    const prepared = [];
    const buffers = [];
    const bitmaps = [];
    const offscreenCanvases = [];
    const readableStreams = [];
    const members = new Map();
    const transferValues = [];
    const transferMembers = new Set();
    for (const member of transfer) {
      if (transferMembers.has(member))
        throw glassMessageException("transfer list member appears more than once", "DataCloneError");
      transferMembers.add(member);
      let tag;
      try { tag = Object.prototype.toString.call(member); }
      catch (_error) { tag = ""; }
      if (tag === "[object ArrayBuffer]") {
        glassMessageValidateArrayBufferTransfer(member);
        buffers.push(member);
        continue;
      }
      const bitmap = glassMessageImageBitmapTransfer(member);
      if (bitmap) {
        const index = transferValues.length;
        transferValues.push(bitmap);
        bitmaps.push(member);
        members.set(member, { kind: "imageBitmap", index });
        continue;
      }
      const offscreenCanvas = glassMessageOffscreenCanvasTransfer(member);
      if (offscreenCanvas) {
        const index = transferValues.length;
        transferValues.push(offscreenCanvas);
        offscreenCanvases.push(member);
        members.set(member, { kind: "offscreenCanvas", index });
        continue;
      }
      const readableStream = glassMessageReadableStreamTransfer(member, prepared);
      if (readableStream) {
        const index = transferValues.length;
        transferValues.push(readableStream.descriptor);
        readableStreams.push(readableStream);
        members.set(member, { kind: "readableStream", index });
        continue;
      }
      const descriptor = glassMessageTransferDescriptor(member);
      if (members.has(descriptor.peer))
        throw glassMessageException("both endpoints of a MessageChannel cannot be transferred together", "DataCloneError");
      members.set(member, prepared.length);
      prepared.push(descriptor);
    }
    const data = glassMessageEncodeMessage(value, members, transferValues);
    const ports = prepared.map(() => glassMakeMessagePort());
    const result = glassMessageDecodeClone(data, ports);
    glassMessageCommitArrayBufferTransfers(buffers);
    glassMessageCommitImageBitmapTransfers(bitmaps);
    glassMessageCommitOffscreenCanvasTransfers(offscreenCanvases);
    for (let index = 0; index < prepared.length; index += 1)
      glassMessageCommitLocalPortTransfer(prepared[index], ports[index]);
    glassMessageCommitReadableStreamTransfers(readableStreams);
    return {
      data: result,
      ports: ports.filter((_, index) => prepared[index].descriptor.hidden !== true),
    };
  };
  const glassMessageStructuredClone = (value, options) => {
    let transfer = [];
    if (options !== undefined) {
      if (options === null || typeof options !== "object")
        throw glassMessageException("structuredClone options must be an object", "TypeError");
      if (options.transfer !== undefined)
        transfer = glassMessageTransferList(options);
    }
    return transfer.length === 0
      ? glassMessageClone(value)
      : glassMessageCloneWithLocalTransfers(value, transfer).data;
  };
  const glassMessageMakeBridgePort = (descriptor) => {
    const bridgeKey = String(descriptor && descriptor.bridge_key || "");
    if (!bridgeKey || bridgeKey.length > __GLASS_MESSAGE_CHANNEL_NAME_LIMIT__)
      throw glassMessageException("MessagePort bridge key is invalid", "DataCloneError");
    const existing = glassMessageBridgeRegistry.get(bridgeKey);
    if (existing && !existing.__glassMessageClosed) return existing;
    const port = Object.create(MessagePortNative.prototype);
    const id = glassNextMessagePortId;
    glassNextMessagePortId += 1;
    globalThis.__glassNextMessagePortId = glassNextMessagePortId;
    glassMessageHidden(port, "__glassMessagePortId", id);
    glassMessageHidden(port, "__glassMessagePortPeer", null);
    glassMessageHidden(port, "__glassMessagePortBridgeKey", bridgeKey);
    glassMessageHidden(port, "__glassMessageListeners", new Map());
    glassMessageHidden(port, "__glassMessageQueue", []);
    glassMessageHidden(port, "__glassMessageStarted", false);
    glassMessageHidden(port, "__glassMessageDeliveryQueued", false);
    glassMessageHidden(port, "__glassMessageClosed", false);
    glassMessageHidden(port, "__glassMessagePortTransferred", false);
    glassMessageInstallHandlers(port);
    glassMessagePortRegistry.set(id, port);
    glassMessageBridgeRegistry.set(bridgeKey, port);
    return port;
  };
  const glassMessageReviveValue = (value, ports, seen = new Map()) => {
    if (!value || typeof value !== "object") return value;
    if (Array.isArray(value)) {
      if (seen.has(value)) return seen.get(value);
      const result = [];
      seen.set(value, result);
      for (const item of value) result.push(glassMessageReviveValue(item, ports, seen));
      return result;
    }
    const marker = value.__glassMessagePortTransferIndex;
    if (Object.keys(value).length === 1 && Number.isSafeInteger(marker))
      return ports[marker] || null;
    if (seen.has(value)) return seen.get(value);
    const result = {};
    seen.set(value, result);
    for (const [key, item] of Object.entries(value))
      glassMessageOwn(result, key, glassMessageReviveValue(item, ports, seen));
    return result;
  };
  const glassMessageDecodeBytes = (bytes) => {
    if (!Array.isArray(bytes) || bytes.length > __GLASS_MESSAGE_BYTES_LIMIT__)
      throw glassMessageException("binary message data is invalid", "DataCloneError");
    const result = bytes.slice();
    for (const byte of result) {
      if (!Number.isInteger(byte) || byte < 0 || byte > 255)
        throw glassMessageException("binary message data is invalid", "DataCloneError");
    }
    return result;
  };
  const glassMessageDecodeClone = (payload, ports) => {
    if (!payload || typeof payload !== "object"
        || payload.__glassMessageClone !== glassMessageCloneMarker
        || !Array.isArray(payload.nodes))
      return glassMessageReviveValue(payload, ports || []);
    const nodes = payload.nodes;
    const cache = new Map();
    const transferValues = Array.isArray(payload.transfer_values)
      ? payload.transfer_values
      : [];
    const transferCache = new Map();
    const decodeTransfer = (index) => {
      if (!Number.isSafeInteger(index) || index < 0 || index >= transferValues.length)
        throw glassMessageException("message transfer reference is invalid", "DataCloneError");
      if (transferCache.has(index)) return transferCache.get(index);
      const result = glassMessageDecodeTransferValue(transferValues[index], ports);
      transferCache.set(index, result);
      return result;
    };
    const decodeNumber = (value) => {
      if (value === "nan") return NaN;
      if (value === "infinity") return Infinity;
      if (value === "negativeInfinity") return -Infinity;
      if (value === "negativeZero") return -0;
      if (typeof value !== "number")
        throw glassMessageException("number message data is invalid", "DataCloneError");
      return value;
    };
    const decodeProperties = (target, properties) => {
      if (!Array.isArray(properties))
        throw glassMessageException("object message data is invalid", "DataCloneError");
      for (const property of properties) {
        if (!Array.isArray(property) || property.length !== 2 || typeof property[0] !== "string")
          throw glassMessageException("object message data is invalid", "DataCloneError");
        glassMessageOwn(target, property[0], decode(property[1]));
      }
    };
    const decodeBytes = bytes => new Uint8Array(glassMessageDecodeBytes(bytes));
    const decode = (descriptor) => {
      if (!descriptor || typeof descriptor !== "object")
        throw glassMessageException("message clone descriptor is invalid", "DataCloneError");
      if (Object.prototype.hasOwnProperty.call(descriptor, "ref")) {
        if (!Number.isSafeInteger(descriptor.ref) || descriptor.ref < 0 || descriptor.ref >= nodes.length)
          throw glassMessageException("message clone reference is invalid", "DataCloneError");
        return decodeNode(descriptor.ref);
      }
      switch (descriptor.type) {
        case "undefined": return undefined;
        case "null": return null;
        case "boolean": return descriptor.value === true;
        case "string":
          if (typeof descriptor.value !== "string") throw glassMessageException("string message data is invalid", "DataCloneError");
          return descriptor.value;
        case "number": return decodeNumber(descriptor.value);
        case "bigint":
          if (typeof BigInt !== "function" || typeof descriptor.value !== "string")
            throw glassMessageException("BigInt message data is unsupported", "DataCloneError");
          try { return BigInt(descriptor.value); }
          catch (_error) { throw glassMessageException("BigInt message data is invalid", "DataCloneError"); }
        case "port":
          if (!Number.isSafeInteger(descriptor.index) || !ports || !ports[descriptor.index])
            throw glassMessageException("MessagePort clone reference is invalid", "DataCloneError");
          return ports[descriptor.index];
        case "transfer": return decodeTransfer(descriptor.index);
        case "hole": return undefined;
        default:
          throw glassMessageException("message clone descriptor is invalid", "DataCloneError");
      }
    };
    const decodeNode = (id) => {
      if (cache.has(id)) return cache.get(id);
      const node = nodes[id];
      if (!node || typeof node !== "object" || typeof node.type !== "string")
        throw glassMessageException("message clone node is invalid", "DataCloneError");
      let result;
      switch (node.type) {
        case "array":
          if (!Number.isSafeInteger(node.length) || node.length < 0
              || node.length > __GLASS_MESSAGE_BYTES_LIMIT__ || !Array.isArray(node.items)
              || node.items.length !== node.length)
            throw glassMessageException("array message data is invalid", "DataCloneError");
          result = [];
          result.length = node.length;
          cache.set(id, result);
          for (let index = 0; index < node.items.length; index += 1) {
            if (node.items[index] && node.items[index].type !== "hole")
              glassMessageOwn(result, String(index), decode(node.items[index]));
          }
          decodeProperties(result, node.properties);
          return result;
        case "object":
          result = {};
          cache.set(id, result);
          decodeProperties(result, node.properties);
          return result;
        case "date":
          result = new Date(decode(node.value));
          cache.set(id, result);
          return result;
        case "regexp":
          try { result = new RegExp(String(node.source), String(node.flags)); }
          catch (_error) { throw glassMessageException("RegExp message data is invalid", "DataCloneError"); }
          cache.set(id, result);
          result.lastIndex = decode(node.lastIndex);
          return result;
        case "map":
          if (!Array.isArray(node.entries)) throw glassMessageException("Map message data is invalid", "DataCloneError");
          result = new Map();
          cache.set(id, result);
          for (const entry of node.entries) {
            if (!Array.isArray(entry) || entry.length !== 2)
              throw glassMessageException("Map message data is invalid", "DataCloneError");
            result.set(decode(entry[0]), decode(entry[1]));
          }
          return result;
        case "set":
          if (!Array.isArray(node.values)) throw glassMessageException("Set message data is invalid", "DataCloneError");
          result = new Set();
          cache.set(id, result);
          for (const item of node.values) result.add(decode(item));
          return result;
        case "arraybuffer":
          result = decodeBytes(node.bytes).buffer;
          cache.set(id, result);
          return result;
        case "dataview": {
          const buffer = decode(node.buffer);
          if (!(buffer instanceof ArrayBuffer) || !Number.isSafeInteger(node.byteOffset)
              || !Number.isSafeInteger(node.byteLength) || node.byteOffset < 0 || node.byteLength < 0
              || node.byteOffset + node.byteLength > buffer.byteLength)
            throw glassMessageException("DataView message data is invalid", "DataCloneError");
          try { result = new DataView(buffer, node.byteOffset, node.byteLength); }
          catch (_error) { throw glassMessageException("DataView message data is invalid", "DataCloneError"); }
          cache.set(id, result);
          return result;
        }
        case "typedarray": {
          const buffer = decode(node.buffer);
          const Constructor = globalThis[String(node.constructor)];
          if (!(buffer instanceof ArrayBuffer) || typeof Constructor !== "function"
              || !glassMessageTypedArrayNames.has(String(node.constructor))
              || !Number.isSafeInteger(node.byteOffset) || !Number.isSafeInteger(node.length)
              || node.byteOffset < 0 || node.length < 0)
            throw glassMessageException("typed array message data is invalid", "DataCloneError");
          const bytesPerElement = Number(Constructor.BYTES_PER_ELEMENT);
          if (!Number.isSafeInteger(bytesPerElement) || bytesPerElement <= 0
              || node.byteOffset % bytesPerElement !== 0
              || node.byteOffset + node.length * bytesPerElement > buffer.byteLength)
            throw glassMessageException("typed array message data is invalid", "DataCloneError");
          try { result = new Constructor(buffer, node.byteOffset, node.length); }
          catch (_error) { throw glassMessageException("typed array message data is invalid", "DataCloneError"); }
          cache.set(id, result);
          return result;
        }
        case "blob":
        case "file": {
          const Constructor = node.type === "file" ? globalThis.File : globalThis.Blob;
          if (typeof Constructor !== "function" || typeof node.mime !== "string")
            throw glassMessageException("Blob message data is unsupported", "DataCloneError");
          const bytes = decodeBytes(node.bytes);
          try {
            result = node.type === "file"
              ? new Constructor([bytes], String(node.name || ""), { type: node.mime, lastModified: decode(node.lastModified) })
              : new Constructor([bytes], { type: node.mime });
          } catch (_error) {
            throw glassMessageException("Blob message data is invalid", "DataCloneError");
          }
          cache.set(id, result);
          return result;
        }
        case "error":
          result = new Error(String(node.message || ""));
          cache.set(id, result);
          result.name = String(node.name || "Error");
          if (node.stack !== null && node.stack !== undefined) result.stack = String(node.stack);
          if (Object.prototype.hasOwnProperty.call(node, "cause")) glassMessageOwn(result, "cause", decode(node.cause));
          return result;
        case "boxed":
          result = Object(decode(node.value));
          cache.set(id, result);
          return result;
        default:
          throw glassMessageException("message clone node is invalid", "DataCloneError");
      }
    };
    return decode(payload.root);
  };
  const glassMessageDecodeEnvelope = (payload) => {
    const descriptors = payload && Array.isArray(payload.transfer_ports)
      ? payload.transfer_ports
      : [];
    const ports = [];
    const provisionalPorts = [];
    try {
      for (const descriptor of descriptors) {
        const bridgeKey = String(descriptor && descriptor.bridge_key || "");
        const existing = glassMessageBridgeRegistry.get(bridgeKey);
        const alreadyRegistered = existing && existing.__glassMessageClosed !== true;
        const port = glassMessageMakeBridgePort(descriptor);
        ports.push(port);
        if (!alreadyRegistered) provisionalPorts.push(port);
      }
      const data = glassMessageDecodeClone(payload && payload.data, ports);
      return {
        data,
        ports: ports.filter((_, index) => descriptors[index] && descriptors[index].hidden !== true),
      };
    } catch (error) {
      for (const port of provisionalPorts) {
        port.__glassMessageClosed = true;
        port.__glassMessageQueue.length = 0;
        const id = Number(port.__glassMessagePortId);
        if (glassMessagePortRegistry.get(id) === port)
          glassMessagePortRegistry.delete(id);
        const bridgeKey = String(port.__glassMessagePortBridgeKey || "");
        if (bridgeKey && glassMessageBridgeRegistry.get(bridgeKey) === port)
          glassMessageBridgeRegistry.delete(bridgeKey);
      }
      throw error;
    }
  };
  const glassMessageEventConstructor = typeof globalThis.__glassMessageEventConstructor === "function"
    ? globalThis.__glassMessageEventConstructor
    : (typeof globalThis.MessageEvent === "function" ? globalThis.MessageEvent : null);
  const GlassMessageEvent = glassMessageEventConstructor || function MessageEvent(type, init) {
    if (!(this instanceof GlassMessageEvent)) throw new TypeError("MessageEvent requires new");
    const settings = init && typeof init === "object" ? init : {};
    this.type = String(type);
    this.bubbles = settings.bubbles === true;
    this.cancelable = settings.cancelable === true;
    this.composed = settings.composed === true;
    this.defaultPrevented = false;
    this.isTrusted = false;
    this.timeStamp = Number(globalThis.performance && globalThis.performance.now && globalThis.performance.now()) || 0;
    this.data = settings.data === undefined ? null : settings.data;
    this.origin = String(settings.origin || "");
    this.lastEventId = String(settings.lastEventId || "");
    this.source = settings.source === undefined ? null : settings.source;
    this.ports = Array.isArray(settings.ports) ? settings.ports.slice() : [];
    this.target = null;
    this.currentTarget = null;
    this.eventPhase = 0;
    this._immediateStopped = false;
  };
  if (!glassMessageEventConstructor) {
    GlassMessageEvent.prototype = Object.create(
      typeof globalThis.Event === "function" ? globalThis.Event.prototype : Object.prototype,
    );
    GlassMessageEvent.prototype.constructor = GlassMessageEvent;
    GlassMessageEvent.prototype.preventDefault = function() {
      if (this.cancelable) this.defaultPrevented = true;
    };
    GlassMessageEvent.prototype.stopPropagation = function() { this.cancelBubble = true; };
    GlassMessageEvent.prototype.stopImmediatePropagation = function() {
      this.cancelBubble = true;
      this._immediateStopped = true;
    };
  }
  globalThis.__glassMessageEventConstructor = GlassMessageEvent;
  globalThis.MessageEvent = GlassMessageEvent;
  const glassMessageInvoke = (callback, target, event) => {
    if (typeof callback === "function") callback.call(target, event);
    else if (callback && typeof callback.handleEvent === "function") callback.handleEvent(event);
  };
  const glassMessageDispatch = (target, event) => {
    if (!event || typeof event !== "object" || !event.type)
      throw new TypeError("native message event is invalid");
    const type = String(event.type);
    if (event.target === null || event.target === undefined) event.target = target;
    event.currentTarget = target;
    event.eventPhase = 2;
    const handler = target["on" + type];
    if (typeof handler === "function") {
      try { handler.call(target, event); } catch (error) {
        glassMessageReportWorkerCallbackException(target, error);
      }
    }
    const listeners = target.__glassMessageListeners instanceof Map
      ? target.__glassMessageListeners
      : new Map();
    const records = (listeners.get(type) || []).slice();
    for (const record of records) {
      try { glassMessageInvoke(record.callback, target, event); } catch (error) {
        glassMessageReportWorkerCallbackException(target, error);
      }
      if (record.once) {
        const current = listeners.get(type) || [];
        listeners.set(type, current.filter(candidate => candidate !== record));
      }
      if (event._immediateStopped === true) break;
    }
    event.currentTarget = null;
    event.eventPhase = 0;
    return event.defaultPrevented !== true;
  };
  const glassMessageReportWorkerCallbackException = (target, error) => {
    if (!target || !Number.isSafeInteger(target.__glassMessagePortId)) return;
    const report = globalThis.__glassReportWorkerMessagePortCallbackException;
    if (typeof report !== "function") return;
    try { report(error); } catch (_reportingError) {}
  };
  const glassMessageEvent = (target, data, ports) => {
    const event = new GlassMessageEvent("message", {
      data,
      origin: "",
      source: null,
      ports: Array.isArray(ports) ? ports : [],
    });
    event.target = target;
    return event;
  };
  const glassMessageErrorEvent = (target) => {
    const event = new GlassMessageEvent("messageerror", {
      data: null,
      origin: "",
      source: null,
      ports: [],
    });
    event.target = target;
    return event;
  };
  const glassMessageSchedule = (target) => {
    if (target.__glassMessageClosed || target.__glassMessageDeliveryQueued) return;
    glassMessageHidden(target, "__glassMessageDeliveryQueued", true);
    const run = () => {
      target.__glassMessageDeliveryQueued = false;
      if (target.__glassMessageClosed || !target.__glassMessageStarted) return;
      const queue = target.__glassMessageQueue;
      while (queue.length > 0 && !target.__glassMessageClosed) {
        const item = queue.shift();
        const event = item && item.messageError === true
          ? glassMessageErrorEvent(target)
          : glassMessageEvent(target, item.data, item.ports);
        glassMessageDispatch(target, event);
      }
    };
    if (typeof globalThis.queueMicrotask === "function") globalThis.queueMicrotask(run);
    else Promise.resolve().then(run);
  };
  const glassMessageEnqueue = (target, data, ports) => {
    if (!target || target.__glassMessageClosed) return;
    const queue = target.__glassMessageQueue;
    if (queue.length >= glassMessagePortQueueLimit)
      throw glassMessageException("native message queue is full", "QuotaExceededError");
    queue.push({ data, ports: Array.isArray(ports) ? ports.slice() : [] });
    if (target.__glassMessageStarted || typeof target.onmessage === "function") {
      target.__glassMessageStarted = true;
      glassMessageSchedule(target);
    }
  };
  const glassMessageEnqueueDecodeFailure = (target) => {
    if (!target || target.__glassMessageClosed) return;
    const queue = target.__glassMessageQueue;
    if (queue.length >= glassMessagePortQueueLimit)
      throw glassMessageException("native message queue is full", "QuotaExceededError");
    queue.push({ messageError: true, data: null, ports: [] });
    if (target.__glassMessageStarted || typeof target.onmessage === "function") {
      target.__glassMessageStarted = true;
      glassMessageSchedule(target);
    }
  };
  const glassMessageStart = (target) => {
    target.__glassMessageStarted = true;
    glassMessageSchedule(target);
  };
  const glassMessageInstallHandlers = (target) => {
    for (const type of ["message", "messageerror", "close"]) {
      let handler = null;
      Object.defineProperty(target, "on" + type, {
        configurable: false,
        enumerable: true,
        get() { return handler; },
        set(value) {
          handler = typeof value === "function" ? value : null;
          if (handler && type === "message") glassMessageStart(target);
        },
      });
    }
  };
  const glassMessageInstallPrototype = (prototype) => {
    prototype.addEventListener = function(type, callback, options) {
      if (callback === null || callback === undefined) return;
      if (typeof callback !== "function" && !(callback && typeof callback.handleEvent === "function"))
        throw new TypeError("message listener must be callable");
      const name = String(type);
      const capture = options === true || Boolean(options && typeof options === "object" && options.capture);
      const once = Boolean(options && typeof options === "object" && options.once);
      const listeners = this.__glassMessageListeners;
      const current = listeners.get(name) || [];
      if (current.some(record => record.callback === callback && record.capture === capture)) return;
      current.push({ callback, capture, once });
      listeners.set(name, current);
      if (name === "message") glassMessageStart(this);
    };
    prototype.removeEventListener = function(type, callback, options) {
      const name = String(type);
      const capture = options === true || Boolean(options && typeof options === "object" && options.capture);
      const listeners = this.__glassMessageListeners;
      const current = listeners.get(name) || [];
      listeners.set(name, current.filter(record => !(record.callback === callback && record.capture === capture)));
    };
    prototype.dispatchEvent = function(event) { return glassMessageDispatch(this, event); };
  };
  const MessagePortNative = typeof globalThis.__glassMessagePortConstructor === "function"
    ? globalThis.__glassMessagePortConstructor
    : function MessagePort() { throw new TypeError("Illegal constructor"); };
  glassMessageInstallPrototype(MessagePortNative.prototype);
  try {
    if (typeof globalThis.EventTarget === "function" && globalThis.EventTarget.prototype)
      Object.setPrototypeOf(MessagePortNative.prototype, globalThis.EventTarget.prototype);
  } catch (_error) {}
  MessagePortNative.prototype.constructor = MessagePortNative;
  MessagePortNative.prototype.start = function() { glassMessageStart(this); };
  MessagePortNative.prototype.close = function() {
    if (this.__glassMessageClosed) return;
    const bridgeKey = String(this.__glassMessagePortBridgeKey || "");
    if (bridgeKey) {
      const workerId = Number.isSafeInteger(globalThis.__glassWorkerId)
        ? globalThis.__glassWorkerId
        : null;
      pushCommand({ kind: "messagePortClose", bridge_key: bridgeKey, worker_id: workerId });
    }
    const peer = this.__glassMessagePortPeer;
    this.__glassMessageClosed = true;
    this.__glassMessageQueue.length = 0;
    glassMessagePortRegistry.delete(this.__glassMessagePortId);
    if (bridgeKey && glassMessageBridgeRegistry.get(bridgeKey) === this)
      glassMessageBridgeRegistry.delete(bridgeKey);
    this.__glassMessagePortBridgeKey = null;
    this.__glassMessagePortPeer = null;
    if (peer && peer.__glassMessagePortPeer === this) {
      peer.__glassMessagePortPeer = null;
      if (!peer.__glassMessageClosed) {
        const event = typeof globalThis.__glassCreateEvent === "function"
          ? globalThis.__glassCreateEvent("close")
          : new globalThis.Event("close", { bubbles: false, cancelable: false });
        glassMessageDispatch(peer, event);
      }
    }
  };
  MessagePortNative.prototype.postMessage = function(message, options) {
    if (this.__glassMessageClosed)
      throw glassMessageException("message port is closed", "InvalidStateError");
    const transfer = glassMessageTransferList(options);
    if (this.__glassMessagePortBridgeKey) {
      const envelope = glassMessageCloneWithTransfers(message, transfer);
      const workerId = Number.isSafeInteger(globalThis.__glassWorkerId)
        ? globalThis.__glassWorkerId
        : null;
      pushCommand({
        kind: "messagePortPostMessage",
        bridge_key: this.__glassMessagePortBridgeKey,
        worker_id: workerId,
        data: envelope.data,
        transfer_ports: envelope.transfer_ports,
      });
      return;
    }
    const peer = this.__glassMessagePortPeer;
    if (!peer || peer.__glassMessageClosed) return;
    const envelope = transfer.length === 0
      ? { data: glassMessageDecodeClone(glassMessageEncodeMessage(message, new Map()), []), ports: [] }
      : glassMessageCloneWithLocalTransfers(message, transfer);
    glassMessageEnqueue(peer, envelope.data, envelope.ports);
  };
  globalThis.__glassMessagePortConstructor = MessagePortNative;
  globalThis.MessagePort = MessagePortNative;
  const glassMakeMessagePort = () => {
    const port = Object.create(MessagePortNative.prototype);
    const id = glassNextMessagePortId;
    glassNextMessagePortId += 1;
    globalThis.__glassNextMessagePortId = glassNextMessagePortId;
    glassMessageHidden(port, "__glassMessagePortId", id);
    glassMessageHidden(port, "__glassMessagePortPeer", null);
    glassMessageHidden(port, "__glassMessagePortBridgeKey", null);
    glassMessageHidden(port, "__glassMessageListeners", new Map());
    glassMessageHidden(port, "__glassMessageQueue", []);
    glassMessageHidden(port, "__glassMessageStarted", false);
    glassMessageHidden(port, "__glassMessageClosed", false);
    glassMessageHidden(port, "__glassMessagePortTransferred", false);
    glassMessageInstallHandlers(port);
    glassMessagePortRegistry.set(id, port);
    return port;
  };
  const MessageChannelNative = typeof globalThis.__glassMessageChannelConstructor === "function"
    ? globalThis.__glassMessageChannelConstructor
    : function MessageChannel() {
        if (!(this instanceof MessageChannelNative)) throw new TypeError("MessageChannel requires new");
        const port1 = glassMakeMessagePort();
        const port2 = glassMakeMessagePort();
        port1.__glassMessagePortPeer = port2;
        port2.__glassMessagePortPeer = port1;
        Object.defineProperty(this, "port1", { configurable: false, enumerable: true, value: port1 });
        Object.defineProperty(this, "port2", { configurable: false, enumerable: true, value: port2 });
      };
  MessageChannelNative.prototype.constructor = MessageChannelNative;
  globalThis.__glassMessageChannelConstructor = MessageChannelNative;
  globalThis.MessageChannel = MessageChannelNative;
  globalThis.__glassDispatchMessagePortById = (portId, payload) => {
    const port = glassMessagePortRegistry.get(Number(portId));
    if (!port || port.__glassMessageClosed) return null;
    let envelope;
    try {
      envelope = glassMessageDecodeEnvelope(payload);
    } catch (_error) {
      glassMessageEnqueueDecodeFailure(port);
      return null;
    }
    if (payload && Array.isArray(payload.object_urls))
      globalThis.__glassInstallObjectUrlTransfers(payload.object_urls);
    glassMessageEnqueue(port, envelope.data, envelope.ports);
    return null;
  };
  globalThis.__glassDispatchMessagePortByBridge = (bridgeKey, payload) => {
    const port = glassMessageBridgeRegistry.get(String(bridgeKey));
    if (!port || port.__glassMessageClosed) return null;
    let envelope;
    try {
      envelope = glassMessageDecodeEnvelope(payload);
    } catch (_error) {
      glassMessageEnqueueDecodeFailure(port);
      return null;
    }
    if (payload && Array.isArray(payload.object_urls))
      globalThis.__glassInstallObjectUrlTransfers(payload.object_urls);
    glassMessageEnqueue(port, envelope.data, envelope.ports);
    return null;
  };
  globalThis.__glassDispatchMessagePortCloseByBridge = (bridgeKey) => {
    const key = String(bridgeKey);
    const port = glassMessageBridgeRegistry.get(key);
    if (!port || port.__glassMessageClosed) return null;
    if (glassMessageBridgeRegistry.get(key) === port)
      glassMessageBridgeRegistry.delete(key);
    if (port.__glassMessagePortBridgeKey === key)
      port.__glassMessagePortBridgeKey = null;
    const peer = port.__glassMessagePortPeer;
    port.__glassMessagePortPeer = null;
    if (peer && peer.__glassMessagePortPeer === port)
      peer.__glassMessagePortPeer = null;
    const event = typeof globalThis.__glassCreateEvent === "function"
      ? globalThis.__glassCreateEvent("close")
      : new globalThis.Event("close", { bubbles: false, cancelable: false });
    glassMessageDispatch(port, event);
    return null;
  };
  const BroadcastChannelNative = typeof globalThis.__glassBroadcastChannelConstructor === "function"
    ? globalThis.__glassBroadcastChannelConstructor
    : function BroadcastChannel(name) {
        if (!(this instanceof BroadcastChannelNative)) throw new TypeError("BroadcastChannel requires new");
        const normalized = String(name);
        if (!normalized || normalized.length > glassMessageChannelNameLimit)
          throw new TypeError("native BroadcastChannel name is invalid");
        Object.defineProperty(this, "name", {
          configurable: false,
          enumerable: true,
          value: normalized,
        });
        glassMessageHidden(this, "__glassMessageListeners", new Map());
        glassMessageHidden(this, "__glassMessageQueue", []);
        glassMessageHidden(this, "__glassMessageStarted", false);
        glassMessageHidden(this, "__glassMessageClosed", false);
        glassMessageInstallHandlers(this);
        const channels = glassBroadcastChannelRegistry.get(normalized) || new Set();
        channels.add(this);
        glassBroadcastChannelRegistry.set(normalized, channels);
      };
  glassMessageInstallPrototype(BroadcastChannelNative.prototype);
  try {
    if (typeof globalThis.EventTarget === "function" && globalThis.EventTarget.prototype)
      Object.setPrototypeOf(BroadcastChannelNative.prototype, globalThis.EventTarget.prototype);
  } catch (_error) {}
  BroadcastChannelNative.prototype.constructor = BroadcastChannelNative;
  BroadcastChannelNative.prototype.postMessage = function(message) {
    if (this.__glassMessageClosed)
      throw glassMessageException("broadcast channel is closed", "InvalidStateError");
    const cloned = glassMessageEncodeMessage(message, new Map());
    const channels = glassBroadcastChannelRegistry.get(this.name) || new Set();
    for (const channel of Array.from(channels)) {
      if (channel !== this && !channel.__glassMessageClosed)
        glassMessageEnqueue(channel, glassMessageDecodeClone(cloned, []), []);
    }
  };
  BroadcastChannelNative.prototype.close = function() {
    if (this.__glassMessageClosed) return;
    this.__glassMessageClosed = true;
    this.__glassMessageQueue.length = 0;
    const channels = glassBroadcastChannelRegistry.get(this.name);
    if (channels) {
      channels.delete(this);
      if (channels.size === 0) glassBroadcastChannelRegistry.delete(this.name);
    }
  };
  globalThis.__glassMessagePortRegistry = glassMessagePortRegistry;
  globalThis.__glassMessageBridgeRegistry = glassMessageBridgeRegistry;
  globalThis.__glassBroadcastChannelRegistry = glassBroadcastChannelRegistry;
  globalThis.__glassNextMessagePortId = glassNextMessagePortId;
  globalThis.__glassBroadcastChannelConstructor = BroadcastChannelNative;
  globalThis.BroadcastChannel = BroadcastChannelNative;
"###;
