pub(in crate::native_engine::javascript) const NATIVE_FONT_FACE_SCRIPT: &str = r###"
  const nativeFontFaceError = (message, name = "InvalidStateError") => {
    const error = new Error(String(message));
    error.name = name;
    return error;
  };
  const nativeFontFaceStates = globalThis.__glassFontFaceStates instanceof WeakMap
    ? globalThis.__glassFontFaceStates
    : new WeakMap();
  globalThis.__glassFontFaceStates = nativeFontFaceStates;
  const nativeFontFaceSetStates = globalThis.__glassFontFaceSetStates instanceof WeakMap
    ? globalThis.__glassFontFaceSetStates
    : new WeakMap();
  globalThis.__glassFontFaceSetStates = nativeFontFaceSetStates;
  const nativeFontFaceInstallRequests = globalThis.__glassFontFaceInstallRequests instanceof Map
    ? globalThis.__glassFontFaceInstallRequests
    : new Map();
  globalThis.__glassFontFaceInstallRequests = nativeFontFaceInstallRequests;
  const nativeFontFaceDescriptorDefaults = {
    style: "normal",
    weight: "normal",
    stretch: "normal",
    unicodeRange: "U+0-10FFFF",
    variant: "normal",
    featureSettings: "normal",
    variationSettings: "normal",
    display: "auto",
    ascentOverride: "normal",
    descentOverride: "normal",
    lineGapOverride: "normal",
    sizeAdjust: "100%",
  };
  const nativeFontFaceDescriptorNames = Object.keys(nativeFontFaceDescriptorDefaults);
  const nativeFontFaceByteLimit = 4 * 1024 * 1024;
  const nativeFontFaceMediaTypes = new Set([
    "", "font/ttf", "font/otf", "font/woff", "font/woff2",
    "application/font-woff", "application/x-font-ttf",
    "application/x-font-opentype", "application/vnd.ms-fontobject",
  ]);
  const nativeFontFaceContentTypeAllowed = (value) => {
    const mediaType = String(value || "").split(";", 1)[0].trim().toLowerCase();
    return nativeFontFaceMediaTypes.has(mediaType);
  };
  const nativeFontFaceHexDigit = (value) => {
    const code = String(value || "").charCodeAt(0);
    if (code >= 48 && code <= 57) return code - 48;
    if (code >= 65 && code <= 70) return code - 65 + 10;
    if (code >= 97 && code <= 102) return code - 97 + 10;
    return -1;
  };
  const nativeFontFaceDataBytes = (href) => {
    const data = String(href).slice(5);
    const comma = data.indexOf(",");
    if (comma < 0) throw nativeFontFaceError("FontFace data URL is malformed", "NetworkError");
    const metadata = data.slice(0, comma);
    const payload = data.slice(comma + 1);
    const mediaType = metadata.split(";", 1)[0].trim().toLowerCase();
    if (!nativeFontFaceMediaTypes.has(mediaType))
      throw nativeFontFaceError("FontFace data URL has an unsupported media type", "NetworkError");
    const isBase64 = metadata.split(";").slice(1).some((part) => part.trim().toLowerCase() === "base64");
    if (isBase64) {
      let encoded;
      try { encoded = decodeURIComponent(payload); }
      catch (_error) { throw nativeFontFaceError("FontFace data URL base64 is malformed", "NetworkError"); }
      try { return decodeBase64(encoded, nativeFontFaceByteLimit); }
      catch (error) { throw nativeFontFaceError(String(error), "NetworkError"); }
    }
    const bytes = [];
    for (let index = 0; index < payload.length;) {
      if (payload[index] === "%") {
        if (index + 2 >= payload.length) throw nativeFontFaceError("FontFace data URL is malformed", "NetworkError");
        const high = nativeFontFaceHexDigit(payload[index + 1]);
        const low = nativeFontFaceHexDigit(payload[index + 2]);
        if (high < 0 || low < 0) throw nativeFontFaceError("FontFace data URL is malformed", "NetworkError");
        bytes.push((high << 4) | low);
        index += 3;
        continue;
      }
      const codePoint = payload.codePointAt(index);
      if (codePoint === undefined) break;
      const character = String.fromCodePoint(codePoint);
      bytes.push(...blobUtf8Bytes(character));
      index += character.length;
      if (bytes.length > nativeFontFaceByteLimit)
        throw nativeFontFaceError("FontFace data URL exceeds its byte limit", "NetworkError");
    }
    if (bytes.length === 0) throw nativeFontFaceError("FontFace data URL is empty", "NetworkError");
    return bytes;
  };
  const nativeFontFaceInlineBytes = (href) => {
    const normalized = String(href);
    if (normalized.slice(0, 5).toLowerCase() === "data:") return nativeFontFaceDataBytes(normalized);
    const entry = typeof nativeObjectUrlRegistry !== "undefined"
      ? nativeObjectUrlRegistry.get(normalized)
      : null;
    if (!entry) return null;
    const contentType = entry.blob && entry.blob.type ? entry.blob.type : "";
    if (!nativeFontFaceContentTypeAllowed(contentType))
      throw nativeFontFaceError("FontFace Blob has an unsupported media type", "NetworkError");
    const bytes = blobBytes(entry.blob);
    if (bytes.length === 0) throw nativeFontFaceError("FontFace Blob is empty", "NetworkError");
    if (bytes.length > nativeFontFaceByteLimit)
      throw nativeFontFaceError("FontFace Blob exceeds its byte limit", "NetworkError");
    return bytes;
  };
  const nativeFontFaceSourceLimit = 32;
  const nativeFontFaceSupportedFormats = new Set([
    "woff", "woff2", "truetype", "opentype", "collection",
  ]);
  const nativeFontFaceSupportedTechs = new Set();
  const nativeFontFaceSplitList = (value) => {
    const text = String(value);
    const parts = [];
    let start = 0;
    let depth = 0;
    let quote = null;
    let escaped = false;
    for (let index = 0; index < text.length; index += 1) {
      const character = text[index];
      if (quote !== null) {
        if (escaped) {
          escaped = false;
        } else if (character === "\\") {
          escaped = true;
        } else if (character === quote) {
          quote = null;
        }
        continue;
      }
      if (character === "\"" || character === "'") {
        quote = character;
      } else if (character === "(") {
        depth += 1;
      } else if (character === ")") {
        depth -= 1;
        if (depth < 0) throw nativeFontFaceError("FontFace source list is malformed", "SyntaxError");
      } else if (character === "," && depth === 0) {
        const part = text.slice(start, index).trim();
        if (!part) throw nativeFontFaceError("FontFace source list contains an empty entry", "SyntaxError");
        parts.push(part);
        if (parts.length >= nativeFontFaceSourceLimit)
          throw nativeFontFaceError("FontFace source list exceeds its limit", "RangeError");
        start = index + 1;
      }
    }
    if (quote !== null || depth !== 0)
      throw nativeFontFaceError("FontFace source list is malformed", "SyntaxError");
    const finalPart = text.slice(start).trim();
    if (!finalPart) throw nativeFontFaceError("FontFace source list contains an empty entry", "SyntaxError");
    parts.push(finalPart);
    if (parts.length > nativeFontFaceSourceLimit)
      throw nativeFontFaceError("FontFace source list exceeds its limit", "RangeError");
    return parts;
  };
  const nativeFontFaceFormatName = (value) => String(value).trim()
    .replace(/^(["'])(.*)\1$/, "$2")
    .trim()
    .toLowerCase();
  const nativeFontFaceDescriptorSupported = (suffix) => {
    let rest = String(suffix || "").trim();
    let supported = true;
    while (rest) {
      const match = rest.match(/^(format|tech)\s*\(/i);
      if (!match) throw nativeFontFaceError("FontFace source has an unsupported descriptor", "SyntaxError");
      const name = String(match[1]).toLowerCase();
      const open = match[0].lastIndexOf("(");
      let depth = 1;
      let quote = null;
      let escaped = false;
      let close = -1;
      for (let index = open + 1; index < rest.length; index += 1) {
        const character = rest[index];
        if (quote !== null) {
          if (escaped) escaped = false;
          else if (character === "\\") escaped = true;
          else if (character === quote) quote = null;
          continue;
        }
        if (character === "\"" || character === "'") quote = character;
        else if (character === "(") depth += 1;
        else if (character === ")") {
          depth -= 1;
          if (depth === 0) {
            close = index;
            break;
          }
        }
      }
      if (quote !== null || close < 0)
        throw nativeFontFaceError("FontFace source descriptor is malformed", "SyntaxError");
      const values = nativeFontFaceSplitList(rest.slice(open + 1, close))
        .map(nativeFontFaceFormatName);
      if (values.length === 0 || values.some((value) => !value))
        throw nativeFontFaceError(`FontFace ${name}() must contain a value`, "SyntaxError");
      if (name === "format") {
        if (!values.some((value) => nativeFontFaceSupportedFormats.has(value))) supported = false;
      } else if (!values.every((value) => nativeFontFaceSupportedTechs.has(value))) {
        supported = false;
      }
      rest = rest.slice(close + 1).trim();
    }
    return supported;
  };
  const nativeFontFaceSources = (source) => {
    const text = String(source === undefined || source === null ? "" : source).trim();
    if (!text) throw nativeFontFaceError("FontFace source must not be empty", "SyntaxError");
    const candidates = [];
    for (const entry of nativeFontFaceSplitList(text)) {
      const match = entry.match(/^(url|local)\(\s*(?:"([^"]*)"|'([^']*)'|([^)]*))\s*\)\s*(.*)$/i);
      if (!match) {
        throw nativeFontFaceError("FontFace source must contain url() or local()", "SyntaxError");
      }
      const value = String(match[2] === undefined ? match[3] === undefined ? match[4] : match[3] : match[2]).trim();
      if (!value) throw nativeFontFaceError("FontFace URL source must not be empty", "SyntaxError");
      if (!nativeFontFaceDescriptorSupported(String(match[5] || "").trim())) continue;
      if (String(match[1]).toLowerCase() === "local") candidates.push({ kind: "local", family: value });
      else candidates.push({ kind: "url", href: new URLNative(value, host.url).href });
    }
    if (candidates.length === 0)
      throw nativeFontFaceError("FontFace source has no supported format", "NetworkError");
    return candidates;
  };
  const nativeFontFaceBufferBytes = (source) => {
    if (typeof ArrayBuffer === "function" && source instanceof ArrayBuffer)
      return Array.from(new Uint8Array(source));
    if (typeof ArrayBuffer === "function" && typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(source))
      return Array.from(new Uint8Array(source.buffer, source.byteOffset, source.byteLength));
    return null;
  };
  const nativeFontFaceCandidateBytes = (candidate, weight, style, stretch) => {
    if (candidate.kind === "local") {
      let encoded;
      try { encoded = globalThis.__glassNativeLocalFont(candidate.family, weight, style, stretch); }
      catch (_error) { encoded = null; }
      if (typeof encoded !== "string" || !encoded)
        throw nativeFontFaceError("FontFace local() source was not found", "NetworkError");
      try { return decodeBase64(encoded, nativeFontFaceByteLimit); }
      catch (error) { throw nativeFontFaceError(String(error), "NetworkError"); }
    }
    const href = candidate.href;
    const inlineBytes = nativeFontFaceInlineBytes(href);
    if (inlineBytes !== null) return inlineBytes;
    return fetchNative(href, { credentials: "same-origin", mode: "cors" }, "font").then((response) => {
      if (!response || response.ok !== true)
        throw nativeFontFaceError("FontFace source response was not successful", "NetworkError");
      const contentType = response.headers && response.headers.get
        ? response.headers.get("content-type") || ""
        : "";
      if (!nativeFontFaceContentTypeAllowed(contentType))
        throw nativeFontFaceError("FontFace source has an unsupported media type", "NetworkError");
      return response.arrayBuffer().then((buffer) => {
        const bytes = Array.from(new Uint8Array(buffer));
        if (bytes.length === 0) throw nativeFontFaceError("FontFace source is empty", "NetworkError");
        if (bytes.length > nativeFontFaceByteLimit)
          throw nativeFontFaceError("FontFace source exceeds its byte limit", "NetworkError");
        return bytes;
      });
    });
  };
  const nativeFontFaceTryCandidates = (candidates, index, lastError, weight, style, stretch) => {
    if (index >= candidates.length) {
      throw lastError || nativeFontFaceError("FontFace sources could not be loaded", "NetworkError");
    }
    return Promise.resolve()
      .then(() => nativeFontFaceCandidateBytes(candidates[index], weight, style, stretch))
      .catch((error) => nativeFontFaceTryCandidates(candidates, index + 1, error, weight, style, stretch));
  };
  let nextFontFaceRequestId = Number.isSafeInteger(globalThis.__glassNextFontFaceRequestId)
    ? globalThis.__glassNextFontFaceRequestId
    : 1;
  globalThis.__glassResolveFontFaceInstall = (requestId, payload) => {
    const id = Number(requestId);
    const pending = nativeFontFaceInstallRequests.get(id);
    if (!pending) return null;
    nativeFontFaceInstallRequests.delete(id);
    if (payload && payload.error) {
      pending.reject(nativeFontFaceError(String(payload.error), "NetworkError"));
    } else {
      pending.resolve();
    }
    return null;
  };
  const nativeFontFaceNormalizeFamily = (value) => String(value).trim()
    .replace(/^("|')(.*)\1$/, "$2");
  const nativeFontFaceDescriptorKey = (descriptor, index) => [
    Number(index),
    String(descriptor && descriptor.family || ""),
    String(descriptor && descriptor.weight || "400"),
    String(descriptor && descriptor.style || "normal"),
    String(descriptor && descriptor.stretch || "normal"),
    String(descriptor && descriptor.variant || "normal"),
    String(descriptor && descriptor.featureSettings || "normal"),
    String(descriptor && descriptor.variationSettings || "normal"),
    String(descriptor && descriptor.display || "auto"),
    String(descriptor && descriptor.ascentOverride || "normal"),
    String(descriptor && descriptor.descentOverride || "normal"),
    String(descriptor && descriptor.lineGapOverride || "normal"),
    String(descriptor && descriptor.sizeAdjust || "100%"),
  ].join("\u0000");
  const nativeFontFaceSettledPromise = (face, status) => {
    const promise = status === "loaded"
      ? Promise.resolve(face)
      : Promise.reject(nativeFontFaceError("native CSS font face failed to load", "NetworkError"));
    promise.catch(() => {});
    return promise;
  };
  const nativeFontFaceSetState = (set) => {
    let state = nativeFontFaceSetStates.get(set);
    if (state) return state;
    state = {
      faces: new Set(),
      cssFaces: new Map(),
      dynamicFaces: new Set(),
      pending: new Set(),
      cycleFaces: new Set(),
      cycleErrors: new Set(),
      status: "loaded",
      readyPromise: Promise.resolve(set),
      readyResolve: null,
      initialized: false,
    };
    nativeFontFaceSetStates.set(set, state);
    return state;
  };
  const nativeFontFaceState = (face) => nativeFontFaceStates.get(face) || null;
  const nativeFontFaceDispatch = (set, type, faces) => {
    if (typeof createEvent !== "function" || typeof dispatchTarget !== "function") return;
    const event = createEvent(type);
    event.fontfaces = Array.from(faces || []);
    try { dispatchTarget(set, event); } catch (_error) {}
  };
  const nativeFontFaceResetReady = (set, state) => {
    state.readyPromise = new Promise((resolve) => { state.readyResolve = resolve; });
    return state.readyPromise;
  };
  const nativeFontFaceBegin = (set, face) => {
    const state = nativeFontFaceSetState(set);
    const idle = state.pending.size === 0;
    state.pending.add(face);
    state.cycleFaces.add(face);
    if (idle) {
      state.cycleErrors.clear();
      state.status = "loading";
      nativeFontFaceResetReady(set, state);
      nativeFontFaceDispatch(set, "loading", [face]);
    }
  };
  const nativeFontFaceFinish = (set, face, error) => {
    const state = nativeFontFaceSetState(set);
    if (!state.pending.delete(face)) return;
    if (error) state.cycleErrors.add(face);
    if (state.pending.size !== 0) return;
    state.status = "loaded";
    const faces = Array.from(state.cycleFaces);
    const errors = Array.from(state.cycleErrors);
    const loaded = faces.filter((candidate) => !state.cycleErrors.has(candidate));
    if (state.readyResolve) state.readyResolve(set);
    state.readyResolve = null;
    if (loaded.length > 0) nativeFontFaceDispatch(set, "loadingdone", loaded);
    if (errors.length > 0) nativeFontFaceDispatch(set, "loadingerror", errors);
    state.cycleFaces.clear();
    state.cycleErrors.clear();
  };
  const nativeFontFaceDescriptorValueLimit = 1024;
  const nativeFontFaceDescriptorError = (name) => {
    throw nativeFontFaceError(`FontFace ${name} descriptor is invalid`, "SyntaxError");
  };
  const nativeFontFaceDescriptorText = (name, value) => {
    const text = String(value);
    if (text.length > nativeFontFaceDescriptorValueLimit)
      throw nativeFontFaceError(`FontFace ${name} descriptor is too long`, "RangeError");
    return text;
  };
  const nativeFontFaceWeightToken = (token) => {
    const normalized = String(token).toLowerCase();
    if (normalized === "normal") return { text: "normal", value: 400 };
    if (normalized === "bold") return { text: "bold", value: 700 };
    if (!/^[0-9]+$/.test(normalized)) return null;
    const value = Number(normalized);
    if (!Number.isSafeInteger(value) || value < 1 || value > 1000) return null;
    return { text: String(value), value };
  };
  const nativeFontFaceWeightDescriptor = (text) => {
    const trimmed = text.trim();
    if (!trimmed) return null;
    const tokens = trimmed.split(/\s+/);
    if (tokens.length > 2) return null;
    const parsed = tokens.map(nativeFontFaceWeightToken);
    if (parsed.some((token) => token === null)) return null;
    if (parsed.length === 2 && parsed[0].value > parsed[1].value) return null;
    return parsed.map((token) => token.text).join(" ");
  };
  const nativeFontFaceStretchToken = (token) => {
    const normalized = String(token).toLowerCase();
    const keywords = {
      "ultra-condensed": 500,
      "extra-condensed": 625,
      condensed: 750,
      "semi-condensed": 875,
      normal: 1000,
      "semi-expanded": 1125,
      expanded: 1250,
      "extra-expanded": 1500,
      "ultra-expanded": 2000,
    };
    if (Object.prototype.hasOwnProperty.call(keywords, normalized))
      return { text: normalized, value: keywords[normalized] };
    const match = normalized.match(/^((?:[0-9]+(?:\.[0-9]{1,3})?)|(?:\.[0-9]{1,3}))%$/);
    if (!match) return null;
    const pieces = match[1].split(".");
    const whole = pieces[0] ? Number(pieces[0]) : 0;
    const fraction = pieces[1] || "";
    const milli = whole * 1000 + Number((fraction + "000").slice(0, 3));
    if (!Number.isSafeInteger(milli)) return null;
    const value = Math.floor((milli + 50) / 100);
    if (value < 500 || value > 2000) return null;
    return { text: `${match[1]}%`, value };
  };
  const nativeFontFaceStretchDescriptor = (text) => {
    const trimmed = text.trim();
    if (!trimmed) return null;
    const tokens = trimmed.split(/\s+/);
    if (tokens.length > 2) return null;
    const parsed = tokens.map(nativeFontFaceStretchToken);
    if (parsed.some((token) => token === null)) return null;
    if (parsed.length === 2 && parsed[0].value > parsed[1].value) return null;
    return parsed.map((token) => token.text).join(" ");
  };
  const nativeFontFaceUnicodeRangeDescriptor = (text) => {
    const tokens = text.split(",");
    if (tokens.length === 0 || tokens.length > 32) return null;
    const normalized = [];
    for (const raw of tokens) {
      const token = raw.trim();
      if (!token || token.length > 16) return null;
      const match = token.match(/^u\+([0-9a-f?]+)(?:-([0-9a-f?]+))?$/i);
      if (!match) return null;
      const first = match[1];
      const second = match[2];
      let start;
      let end;
      if (second !== undefined) {
        if (first.includes("?") || second.includes("?")) return null;
        start = Number.parseInt(first, 16);
        end = Number.parseInt(second, 16);
      } else if (first.includes("?")) {
        if (!/^[0-9a-f]*\?+$/.test(first.toLowerCase())) return null;
        start = Number.parseInt(first.replace(/\?/g, "0"), 16);
        end = Number.parseInt(first.replace(/\?/g, "f"), 16);
      } else {
        start = Number.parseInt(first, 16);
        end = start;
      }
      if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end)
        || start < 0 || start > end || end > 0x10ffff) return null;
      normalized.push(`U+${first.toUpperCase()}${second === undefined ? "" : `-${second.toUpperCase()}`}`);
    }
    return normalized.join(", ");
  };
  const nativeFontFaceVariationDescriptor = (text) => {
    const trimmed = text.trim();
    if (trimmed.toLowerCase() === "normal") return "normal";
    if (!trimmed) return null;
    const entries = trimmed.split(",");
    if (entries.length === 0 || entries.length > 8) return null;
    const normalized = [];
    for (const raw of entries) {
      const match = raw.trim().match(/^(['"])([\s\S]{4})\1\s+([+-]?(?:(?:[0-9]+(?:\.[0-9]{1,3})?)|(?:\.[0-9]{1,3})))$/);
      if (!match) return null;
      for (const character of match[2]) {
        const code = character.charCodeAt(0);
        if (code < 0x20 || code > 0x7e || ["'", "\"", "\\", ","].includes(character)) return null;
      }
      const magnitude = Math.abs(Number(match[3]));
      if (!Number.isFinite(magnitude) || magnitude > 32768) return null;
      normalized.push(`${match[1]}${match[2]}${match[1]} ${match[3]}`);
    }
    return normalized.join(", ");
  };
  const nativeFontFaceVariantDescriptor = (text) => {
    const trimmed = text.trim();
    if (trimmed.toLowerCase() === "normal") return "normal";
    if (!trimmed) return null;
    const tokens = trimmed.split(/\s+/);
    if (tokens.length > 16) return null;
    const groups = {
      commonLigatures: new Set(["common-ligatures", "no-common-ligatures"]),
      discretionaryLigatures: new Set(["discretionary-ligatures", "no-discretionary-ligatures"]),
      historicalLigatures: new Set(["historical-ligatures", "no-historical-ligatures"]),
      contextualLigatures: new Set(["contextual", "no-contextual"]),
      caps: new Set(["small-caps", "all-small-caps", "petite-caps", "all-petite-caps", "unicase", "titling-caps"]),
      position: new Set(["sub", "super"]),
      alternates: new Set(["historical-forms"]),
      eastAsianForm: new Set(["jis78", "jis83", "jis90", "jis04", "simplified", "traditional"]),
      eastAsianWidth: new Set(["full-width", "proportional-width"]),
      eastAsianRuby: new Set(["ruby"]),
      numericFigure: new Set(["lining-nums", "oldstyle-nums"]),
      numericSpacing: new Set(["proportional-nums", "tabular-nums"]),
      numericFraction: new Set(["diagonal-fractions", "stacked-fractions"]),
      numericOrdinal: new Set(["ordinal"]),
      numericSlashedZero: new Set(["slashed-zero"]),
    };
    const seen = new Set();
    const normalized = [];
    for (const raw of tokens) {
      const token = raw.toLowerCase();
      if (token === "normal" || token === "none" || token === "inherit"
        || token === "initial" || token === "unset" || token === "revert"
        || token === "revert-layer") return null;
      let group = null;
      for (const name of Object.keys(groups)) {
        if (groups[name].has(token)) {
          group = name;
          break;
        }
      }
      if (group === null || seen.has(group)) return null;
      seen.add(group);
      normalized.push(token);
    }
    return normalized.length === 0 ? null : normalized.join(" ");
  };
  const nativeFontFaceFeatureSettingsDescriptor = (text) => {
    const trimmed = text.trim();
    if (trimmed.toLowerCase() === "normal") return "normal";
    if (!trimmed) return null;
    const entries = trimmed.split(",");
    if (entries.length === 0 || entries.length > 16) return null;
    const normalized = [];
    for (const raw of entries) {
      const match = raw.trim().match(/^(['"])([\s\S]{4})\1(?:\s*(on|off|[0-9]+))?$/i);
      if (!match) return null;
      for (const character of match[2]) {
        const code = character.charCodeAt(0);
        if (code < 0x20 || code > 0x7e || ["'", "\"", ","].includes(character)) return null;
      }
      let value = 1;
      if (match[3] !== undefined) {
        if (match[3].toLowerCase() === "off") value = 0;
        else if (match[3].toLowerCase() !== "on") {
          value = Number(match[3]);
          if (!Number.isSafeInteger(value) || value < 0 || value > 65535) return null;
        }
      }
      const tag = match[2];
      const entry = `"${tag}" ${value}`;
      const existing = normalized.findIndex((candidate) => candidate.tag === tag);
      if (existing >= 0) normalized[existing] = { tag, entry };
      else normalized.push({ tag, entry });
    }
    return normalized.map((candidate) => candidate.entry).join(", ");
  };
  const nativeFontFaceSizeAdjustDescriptor = (text) => {
    const match = text.trim().match(/^((?:[0-9]+(?:\.[0-9]{1,3})?)|(?:\.[0-9]{1,3}))%$/);
    if (!match) return null;
    const pieces = match[1].split(".");
    const whole = pieces[0] ? Number(pieces[0]) : 0;
    const fraction = pieces[1] || "";
    const milliPercentage = whole * 1000 + Number((fraction + "000").slice(0, 3));
    const adjusted = Math.floor((milliPercentage + 50) / 100);
    if (!Number.isSafeInteger(adjusted) || adjusted < 250 || adjusted > 4000) return null;
    return adjusted % 10 === 0
      ? `${adjusted / 10}%`
      : `${Math.floor(adjusted / 10)}.${adjusted % 10}%`;
  };
  const nativeFontFaceMetricOverrideDescriptor = (text) => {
    const trimmed = text.trim().toLowerCase();
    if (trimmed === "normal") return "normal";
    const match = trimmed.match(/^((?:[0-9]+(?:\.[0-9]{1,3})?)|(?:\.[0-9]{1,3}))%$/);
    if (!match) return null;
    const pieces = match[1].split(".");
    const whole = pieces[0] ? Number(pieces[0]) : 0;
    const fraction = pieces[1] || "";
    const milliPercentage = whole * 1000 + Number((fraction + "000").slice(0, 3));
    const value = Math.floor((milliPercentage + 50) / 100);
    if (!Number.isSafeInteger(value) || value < 0 || value > 10000) return null;
    return value % 10 === 0
      ? `${value / 10}%`
      : `${Math.floor(value / 10)}.${value % 10}%`;
  };

  const nativeFontFaceDescriptorValue = (name, value) => {
    const text = nativeFontFaceDescriptorText(name, value);
    if (name === "style") {
      const normalized = text.trim().toLowerCase();
      if (!["normal", "italic"].includes(normalized)) return nativeFontFaceDescriptorError(name);
      return normalized;
    }
    if (name === "weight") {
      const normalized = nativeFontFaceWeightDescriptor(text);
      return normalized === null ? nativeFontFaceDescriptorError(name) : normalized;
    }
    if (name === "stretch") {
      const normalized = nativeFontFaceStretchDescriptor(text);
      return normalized === null ? nativeFontFaceDescriptorError(name) : normalized;
    }
    if (name === "unicodeRange") {
      const normalized = nativeFontFaceUnicodeRangeDescriptor(text);
      return normalized === null ? nativeFontFaceDescriptorError(name) : normalized;
    }
    if (["ascentOverride", "descentOverride", "lineGapOverride"].includes(name)) {
      const normalized = nativeFontFaceMetricOverrideDescriptor(text);
      return normalized === null ? nativeFontFaceDescriptorError(name) : normalized;
    }
    if (name === "variationSettings") {
      const normalized = nativeFontFaceVariationDescriptor(text);
      return normalized === null ? nativeFontFaceDescriptorError(name) : normalized;
    }
    if (name === "variant") {
      const normalized = nativeFontFaceVariantDescriptor(text);
      return normalized === null ? nativeFontFaceDescriptorError(name) : normalized;
    }
    if (name === "featureSettings") {
      const normalized = nativeFontFaceFeatureSettingsDescriptor(text);
      return normalized === null ? nativeFontFaceDescriptorError(name) : normalized;
    }
    if (name === "sizeAdjust") {
      const normalized = nativeFontFaceSizeAdjustDescriptor(text);
      return normalized === null ? nativeFontFaceDescriptorError(name) : normalized;
    }
    if (name !== "display") return text;
    const normalized = text.trim().toLowerCase();
    if (!["auto", "block", "swap", "fallback", "optional"].includes(normalized))
      throw nativeFontFaceError("FontFace display descriptor is invalid", "SyntaxError");
    return normalized;
  };
  const FontFaceNative = typeof globalThis.__glassFontFaceConstructor === "function"
    ? globalThis.__glassFontFaceConstructor
    : function FontFace(family, source, descriptors) {
        if (!(this instanceof FontFaceNative)) throw new TypeError("FontFace requires new");
        const normalizedFamily = nativeFontFaceNormalizeFamily(family);
        if (!normalizedFamily) throw nativeFontFaceError("FontFace family must not be empty", "SyntaxError");
        const settings = descriptors && typeof descriptors === "object" ? descriptors : {};
        let binaryBytes = null;
        try { binaryBytes = nativeFontFaceBufferBytes(source); }
        catch (_error) { throw nativeFontFaceError("FontFace BufferSource is detached", "TypeError"); }
        const state = {
          family: normalizedFamily,
          source: binaryBytes === null ? String(source === undefined || source === null ? "" : source) : "",
          binaryBytes,
          status: "unloaded",
          loadedPromise: null,
          staticFace: false,
          staticStatus: null,
          set: null,
        };
        for (const name of nativeFontFaceDescriptorNames) {
          state[name] = settings[name] === undefined
            ? nativeFontFaceDescriptorDefaults[name]
            : nativeFontFaceDescriptorValue(name, settings[name]);
        }
        nativeFontFaceStates.set(this, state);
      };
  const nativeFontFaceDescriptor = (name) => ({
    enumerable: true,
    configurable: true,
    get() {
      const state = nativeFontFaceState(this);
      if (!state) throw new TypeError("Illegal FontFace receiver");
      return state[name];
    },
    set(value) {
      const state = nativeFontFaceState(this);
      if (!state) throw new TypeError("Illegal FontFace receiver");
      state[name] = nativeFontFaceDescriptorValue(name, value);
    },
  });
  Object.defineProperty(FontFaceNative.prototype, "family", {
    enumerable: true,
    configurable: true,
    get() {
      const state = nativeFontFaceState(this);
      if (!state) throw new TypeError("Illegal FontFace receiver");
      return state.family;
    },
    set(value) {
      const state = nativeFontFaceState(this);
      if (!state) throw new TypeError("Illegal FontFace receiver");
      const family = nativeFontFaceNormalizeFamily(value);
      if (!family) throw nativeFontFaceError("FontFace family must not be empty", "SyntaxError");
      state.family = family;
    },
  });
  for (const name of nativeFontFaceDescriptorNames) {
    Object.defineProperty(FontFaceNative.prototype, name, nativeFontFaceDescriptor(name));
  }
  Object.defineProperty(FontFaceNative.prototype, "status", {
    enumerable: true,
    configurable: true,
    get() {
      const state = nativeFontFaceState(this);
      if (!state) throw new TypeError("Illegal FontFace receiver");
      return state.status;
    },
  });
  Object.defineProperty(FontFaceNative.prototype, "loaded", {
    enumerable: true,
    configurable: true,
    get() {
      const state = nativeFontFaceState(this);
      if (!state) throw new TypeError("Illegal FontFace receiver");
      if (!state.loadedPromise) state.loadedPromise = nativeFontFaceSettledPromise(this, state.status);
      return state.loadedPromise;
    },
  });
  const nativeFontFaceLoad = function() {
    const state = nativeFontFaceState(this);
    if (!state) return Promise.reject(new TypeError("Illegal FontFace receiver"));
    if (state.status === "loaded" && state.loadedPromise) return state.loadedPromise;
    if (state.status === "loading" && state.loadedPromise) return state.loadedPromise;
    const set = state.set;
    if (set) nativeFontFaceBegin(set, this);
    state.status = "loading";
    const nativeFontFaceFail = (error) => {
      state.status = "error";
      const rejection = Promise.reject(error);
      rejection.catch(() => {});
      state.loadedPromise = rejection;
      if (set) nativeFontFaceFinish(set, this, error);
      return rejection;
    };
    const sourcePromise = Promise.resolve().then(() => {
      if (state.staticFace) {
        if (state.staticStatus === "loaded") return null;
        throw nativeFontFaceError("native CSS font face failed to load", "NetworkError");
      }
      if (state.binaryBytes !== null) {
        if (state.binaryBytes.length === 0)
          throw nativeFontFaceError("FontFace BufferSource is empty", "NetworkError");
        if (state.binaryBytes.length > nativeFontFaceByteLimit)
          throw nativeFontFaceError("FontFace BufferSource exceeds its byte limit", "NetworkError");
        return state.binaryBytes.slice();
      }
      return nativeFontFaceTryCandidates(
        nativeFontFaceSources(state.source),
        0,
        null,
        state.weight,
        state.style,
        state.stretch,
      );
    });
    const promise = sourcePromise.then((bytes) => {
      let accepted = Promise.resolve();
      if (bytes !== null) {
        const requestId = nextFontFaceRequestId;
        nextFontFaceRequestId += 1;
        globalThis.__glassNextFontFaceRequestId = nextFontFaceRequestId;
        accepted = new Promise((resolve, reject) => {
          nativeFontFaceInstallRequests.set(requestId, { resolve, reject });
          try {
            pushCommand({
              kind: "fontFaceInstall",
              request_id: requestId,
              family: state.family,
              weight: state.weight,
              style: state.style,
              stretch: state.stretch,
              unicode_range: state.unicodeRange,
              variant: state.variant,
              feature_settings: state.featureSettings,
              size_adjust: state.sizeAdjust,
              ascent_override: state.ascentOverride,
              descent_override: state.descentOverride,
              line_gap_override: state.lineGapOverride,
              display: state.display,
              variation_settings: state.variationSettings,
              body_base64: encodeBase64(bytes, nativeFontFaceByteLimit),
            });
          } catch (error) {
            nativeFontFaceInstallRequests.delete(requestId);
            reject(error);
          }
        });
      }
      return accepted.then(() => {
        state.status = "loaded";
        state.loadedPromise = Promise.resolve(this);
        if (set) nativeFontFaceFinish(set, this, null);
        return this;
      }, nativeFontFaceFail);
    }, nativeFontFaceFail);
    state.loadedPromise = promise;
    return promise;
  };
  FontFaceNative.prototype.load = nativeFontFaceLoad;
  FontFaceNative.prototype.toString = function() { return "[object FontFace]"; };
  try { Object.setPrototypeOf(FontFaceNative.prototype, Object.prototype); } catch (_error) {}
  globalThis.__glassFontFaceConstructor = FontFaceNative;
  globalThis.FontFace = FontFaceNative;

  const nativeFontFaceAdd = (set, face) => {
    const setState = nativeFontFaceSetState(set);
    const faceState = nativeFontFaceState(face);
    if (!faceState) throw new TypeError("FontFaceSet.add requires a FontFace");
    if (faceState.set && faceState.set !== set) {
      throw nativeFontFaceError("FontFace is already in another FontFaceSet", "InvalidModificationError");
    }
    if (faceState.set === set) return set;
    if (setState.faces.size >= 256) throw new RangeError("native FontFaceSet limit exceeded");
    faceState.set = set;
    faceState.staticFace = false;
    setState.dynamicFaces.add(face);
    setState.faces.add(face);
    return set;
  };
  const FontFaceSetNative = typeof globalThis.__glassFontFaceSetConstructor === "function"
    ? globalThis.__glassFontFaceSetConstructor
    : function FontFaceSet(fonts) {
        if (!(this instanceof FontFaceSetNative)) throw new TypeError("FontFaceSet requires new");
        if (nativeFontFaceSetStates.has(this)) return this;
        EventTargetNative.call(this);
        nativeFontFaceSetState(this);
        if (fonts !== undefined && fonts !== null) {
          if (typeof fonts[Symbol.iterator] !== "function") throw new TypeError("FontFaceSet fonts must be iterable");
          for (const face of fonts) nativeFontFaceAdd(this, face);
        }
      };
  try { Object.setPrototypeOf(FontFaceSetNative.prototype, EventTargetNative.prototype); } catch (_error) {}
  FontFaceSetNative.prototype.constructor = FontFaceSetNative;
  Object.defineProperty(FontFaceSetNative.prototype, "status", {
    enumerable: true,
    configurable: true,
    get() {
      const state = nativeFontFaceSetStates.get(this);
      if (!state) throw new TypeError("Illegal FontFaceSet receiver");
      return state.status;
    },
  });
  Object.defineProperty(FontFaceSetNative.prototype, "ready", {
    enumerable: true,
    configurable: true,
    get() {
      const state = nativeFontFaceSetStates.get(this);
      if (!state) throw new TypeError("Illegal FontFaceSet receiver");
      return state.readyPromise;
    },
  });
  Object.defineProperty(FontFaceSetNative.prototype, "size", {
    enumerable: true,
    configurable: true,
    get() {
      const state = nativeFontFaceSetStates.get(this);
      if (!state) throw new TypeError("Illegal FontFaceSet receiver");
      return state.faces.size;
    },
  });
  FontFaceSetNative.prototype.add = function(face) { return nativeFontFaceAdd(this, face); };
  FontFaceSetNative.prototype.delete = function(face) {
    const state = nativeFontFaceSetStates.get(this);
    const faceState = nativeFontFaceState(face);
    if (!state || !faceState || faceState.set !== this || faceState.staticFace) return false;
    state.dynamicFaces.delete(face);
    state.faces.delete(face);
    faceState.set = null;
    return true;
  };
  FontFaceSetNative.prototype.clear = function() {
    const state = nativeFontFaceSetStates.get(this);
    if (!state) throw new TypeError("Illegal FontFaceSet receiver");
    for (const face of Array.from(state.dynamicFaces)) this.delete(face);
  };
  FontFaceSetNative.prototype.forEach = function(callback, thisArg) {
    const state = nativeFontFaceSetStates.get(this);
    if (!state) throw new TypeError("Illegal FontFaceSet receiver");
    if (typeof callback !== "function") throw new TypeError("FontFaceSet.forEach callback must be callable");
    for (const face of Array.from(state.faces)) callback.call(thisArg, face, face, this);
  };
  FontFaceSetNative.prototype.values = function() {
    const state = nativeFontFaceSetStates.get(this);
    if (!state) throw new TypeError("Illegal FontFaceSet receiver");
    return state.faces.values();
  };
  FontFaceSetNative.prototype.keys = FontFaceSetNative.prototype.values;
  FontFaceSetNative.prototype.entries = function() {
    const state = nativeFontFaceSetStates.get(this);
    if (!state) throw new TypeError("Illegal FontFaceSet receiver");
    return Array.from(state.faces, (face) => [face, face])[Symbol.iterator]();
  };
  FontFaceSetNative.prototype[Symbol.iterator] = FontFaceSetNative.prototype.values;
  const nativeFontFaceFamilies = (font) => {
    const source = String(font).trim();
    const size = source.match(/\b\d+(?:\.\d+)?(?:px|pt|pc|in|cm|mm|em|rem|ex|ch|vw|vh|vmin|vmax|%)\b/i);
    if (!size) throw nativeFontFaceError("invalid CSS font shorthand", "SyntaxError");
    let familyText = source.slice(size.index + size[0].length)
      .replace(/^\s*\/\s*[^\s]+\s*/, "")
      .trim();
    if (!familyText) throw nativeFontFaceError("invalid CSS font family list", "SyntaxError");
    return familyText.split(",").map(nativeFontFaceNormalizeFamily).filter(Boolean);
  };
  const nativeFontFaceMatches = (face, families) => {
    const state = nativeFontFaceState(face);
    if (!state) return false;
    const family = nativeFontFaceNormalizeFamily(state.family).toLowerCase();
    return families.some((candidate) => nativeFontFaceNormalizeFamily(candidate).toLowerCase() === family);
  };
  FontFaceSetNative.prototype.check = function(font, _text) {
    const state = nativeFontFaceSetStates.get(this);
    if (!state) throw new TypeError("Illegal FontFaceSet receiver");
    const families = nativeFontFaceFamilies(font);
    const matching = Array.from(state.faces).filter((face) => nativeFontFaceMatches(face, families));
    return matching.length === 0 || matching.every((face) => nativeFontFaceState(face).status === "loaded");
  };
  FontFaceSetNative.prototype.load = function(font, _text) {
    const state = nativeFontFaceSetStates.get(this);
    if (!state) return Promise.reject(new TypeError("Illegal FontFaceSet receiver"));
    let families;
    try { families = nativeFontFaceFamilies(font); }
    catch (error) { return Promise.reject(error); }
    const matching = Array.from(state.faces).filter((face) => nativeFontFaceMatches(face, families));
    if (matching.length === 0) return Promise.resolve([]);
    return Promise.all(matching.map((face) => {
      const faceState = nativeFontFaceState(face);
      if (faceState.status === "loaded") return face;
      return face.load();
    })).then(() => matching);
  };
  globalThis.__glassFontFaceSetConstructor = FontFaceSetNative;
  globalThis.FontFaceSet = FontFaceSetNative;
  const fontFaceSet = globalThis.__glassFontFaceSet
    && nativeFontFaceSetStates.has(globalThis.__glassFontFaceSet)
    ? globalThis.__glassFontFaceSet
    : new FontFaceSetNative();
  globalThis.__glassFontFaceSet = fontFaceSet;
  for (const type of ["loading", "loadingdone", "loadingerror"]) {
    const property = "on" + type;
    if (!Object.prototype.hasOwnProperty.call(fontFaceSet, property))
      installEventHandlerProperty(fontFaceSet, type);
  }
  const nativeFontFaceRefresh = () => {
    const setState = nativeFontFaceSetState(fontFaceSet);
    const descriptors = (Array.isArray(host.font_faces) ? host.font_faces : []).slice(0, 256);
    const ordered = [];
    const changed = [];
    const currentKeys = new Set();
    for (let index = 0; index < descriptors.length; index += 1) {
      const descriptor = descriptors[index] || {};
      const key = nativeFontFaceDescriptorKey(descriptor, index);
      currentKeys.add(key);
      let face = setState.cssFaces.get(key);
      if (!face) {
        face = Object.create(FontFaceNative.prototype);
        FontFaceNative.call(face, String(descriptor.family || ""), "", {
          weight: String(descriptor.weight || "400"),
          style: descriptor.style === "italic" ? "italic" : "normal",
          stretch: String(descriptor.stretch || "normal"),
          unicodeRange: String(descriptor.unicodeRange || "U+0-10FFFF"),
          variant: String(descriptor.variant || "normal"),
          featureSettings: String(descriptor.featureSettings || "normal"),
          variationSettings: String(descriptor.variationSettings || "normal"),
          sizeAdjust: String(descriptor.sizeAdjust || "100%"),
          ascentOverride: String(descriptor.ascentOverride || "normal"),
          descentOverride: String(descriptor.descentOverride || "normal"),
          lineGapOverride: String(descriptor.lineGapOverride || "normal"),
          display: String(descriptor.display || "auto"),
        });
        const faceState = nativeFontFaceState(face);
        faceState.staticFace = true;
        faceState.staticStatus = descriptor.status === "loaded" ? "loaded" : "error";
        faceState.status = faceState.staticStatus;
        faceState.set = fontFaceSet;
        faceState.loadedPromise = nativeFontFaceSettledPromise(face, faceState.status);
        setState.cssFaces.set(key, face);
        if (setState.initialized) changed.push([face, faceState.staticStatus]);
      } else {
        const faceState = nativeFontFaceState(face);
        const nextStatus = descriptor.status === "loaded" ? "loaded" : "error";
        if (faceState.staticStatus !== nextStatus) {
          faceState.staticStatus = nextStatus;
          faceState.status = nextStatus;
          faceState.loadedPromise = nativeFontFaceSettledPromise(face, nextStatus);
          if (setState.initialized) changed.push([face, nextStatus]);
        }
      }
      ordered.push(face);
    }
    for (const [key, face] of Array.from(setState.cssFaces.entries())) {
      if (currentKeys.has(key)) continue;
      setState.cssFaces.delete(key);
      const faceState = nativeFontFaceState(face);
      if (faceState) faceState.set = null;
    }
    setState.faces = new Set([...ordered, ...Array.from(setState.dynamicFaces)]);
    globalThis.__glassFontFaceDescriptors = descriptors.map((descriptor, index) => ({
      key: nativeFontFaceDescriptorKey(descriptor, index),
    }));
    if (!setState.initialized) {
      setState.initialized = true;
      setState.status = "loaded";
      setState.readyPromise = Promise.resolve(fontFaceSet);
      setState.readyResolve = null;
      return;
    }
    if (changed.length === 0) return;
    for (const [face] of changed) nativeFontFaceBegin(fontFaceSet, face);
    for (const [face, status] of changed) {
      nativeFontFaceFinish(fontFaceSet, face, status !== "loaded" ? nativeFontFaceError("native CSS font face failed to load", "NetworkError") : null);
    }
  };
  globalThis.__glassRefreshFontFaces = nativeFontFaceRefresh;
"###;
