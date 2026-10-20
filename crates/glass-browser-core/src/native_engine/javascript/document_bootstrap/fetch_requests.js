  const fetchRequests = globalThis.__glassFetchRequests instanceof Map
    ? globalThis.__glassFetchRequests
    : new Map();
  let nextFetchRequestId = Number.isSafeInteger(globalThis.__glassNextFetchRequestId)
    ? globalThis.__glassNextFetchRequestId
    : 1;
  globalThis.__glassDynamicImport = (referrer, referrerPolicy, specifier, options) => {{
    const intrinsics = globalThis.__glassDynamicImportIntrinsics;
    return new intrinsics.PromiseConstructor((resolve, reject) => {{
      let normalizedReferrer;
      let normalizedRequest;
      try {{
        if (typeof referrer === "symbol")
          throw new intrinsics.TypeErrorConstructor("dynamic import referrer cannot be a Symbol");
        normalizedReferrer = intrinsics.StringConstructor(referrer);
        normalizedRequest = intrinsics.normalize(specifier, options);
      }} catch (error) {{ reject(error); return; }}
      if (!Number.isSafeInteger(nextFetchRequestId)
          || nextFetchRequestId < 1
          || nextFetchRequestId > 0xFFFFFFFF) {{
        reject(new RangeError("dynamic import request id limit exceeded"));
        return;
      }}
      const requestId = nextFetchRequestId;
      nextFetchRequestId += 1;
      globalThis.__glassNextFetchRequestId = nextFetchRequestId;
      const pending = {{
        resolve, reject, signal: null, abortListener: null,
        uploadStreamId: null, dynamicModuleImport: true,
        moduleType: normalizedRequest.moduleType,
      }};
      fetchRequests.set(requestId, pending);
      try {{
        pushCommand({{
          kind: "fetch", request_id: requestId, href: normalizedRequest.specifier,
          credentials: true, method: "GET", headers: {{}}, body: null,
          body_base64: null, content_type: null, mode: "cors",
          redirect: "follow", cache: "default", timeout_ms: null,
          upload_stream_id: null, destination: "module",
          module_referrer: normalizedReferrer,
          referrer_policy: referrerPolicy,
          module_type: normalizedRequest.moduleType,
        }});
      }} catch (error) {{
        fetchRequests.delete(requestId);
        reject(error);
      }}
    }});
  }};
  const formDataEntries = (form) => {{
    if (!form || form.tagName !== "FORM") {{
      throw new TypeError("FormData constructor requires a form element");
    }}
    const entries = [];
    const elementsByIndex = new Map();
    const firstLegendByFieldset = new Map();
    for (const element of elements) {{
      elementsByIndex.set(element.nodeIndex, element);
      if (element.tagName === "LEGEND") {{
        const parent = elementsByIndex.get(element.parentIndex);
        if (parent && parent.tagName === "FIELDSET" && !firstLegendByFieldset.has(parent.nodeIndex)) {{
          firstLegendByFieldset.set(parent.nodeIndex, element);
        }}
      }}
    }}
    for (const control of elements) {{
      const customDefinition = nativeCustomElementDefinitionFor(control);
      if (customDefinition && customDefinition.formAssociated && !customDefinition.extends) {{
        if (nativeFormAssociatedOwnerFor(control)?.nodeIndex !== form.nodeIndex
            || nativeFormAssociatedIsDisabled(control, firstLegendByFieldset)) continue;
        const name = String(control.getAttribute("name") || "");
        const submission = nativeFormAssociatedValues.get(control);
        if (!submission || submission.kind === "null") continue;
        if (submission.kind === "formdata") {{
          for (const entry of submission.entries) entries.push([entry[0], entry[1]]);
        }} else if (name) {{
          entries.push([name, submission.kind === "file"
            ? {{ kind: "file", value: submission.value, filename: submission.filename }}
            : submission.value]);
        }}
        continue;
      }}
      if (control.formOwnerIndex !== form.nodeIndex || control.disabled) continue;
      const name = String(control.getAttribute("name") || "");
      if (!name) continue;
      if (control.tagName === "SELECT") {{
        const options = elements.filter(option =>
          option.tagName === "OPTION"
            && option.parentIndex === control.nodeIndex
            && option.selected
            && !option.disabled
        );
        const selectedOptions = control.getAttribute("multiple") === null
          ? options.slice(0, 1)
          : options;
        for (const option of selectedOptions) {{
          const optionValue = option.getAttribute("value");
          entries.push([name, optionValue === null ? String(option.textContent || "") : String(optionValue)]);
        }}
        continue;
      }}
      const type = String(control.getAttribute("type") || "text").toLowerCase();
      if (["button", "reset", "submit", "image"].includes(type)) continue;
      if (type === "file") {{
        const selectedFiles = control.files && typeof control.files.length === "number"
          ? Array.from(control.files)
          : [];
        if (selectedFiles.length === 0) {{
          entries.push([name, formDataValue(new FileNative([], "", {{ type: "", lastModified: 0 }}), "")]);
        }} else {{
          for (const file of selectedFiles) entries.push([name, formDataValue(file)]);
        }}
        continue;
      }}
      if (["checkbox", "radio"].includes(type) && !control.checked) continue;
      if (control.tagName === "OPTION") continue;
      entries.push([name, String(control.value)]);
    }}
    return entries;
  }};
  const blobPartText = (part) => {{
    if (part && part.__glassNativeBlob === true) return part._text;
    if (typeof part === "string") return part;
    throw new TypeError("native Blob supports only text or Blob parts");
  }};
  const boundedBlobText = (parts) => {{
    if (parts === undefined || parts === null) return "";
    if (!Array.isArray(parts)) throw new TypeError("native Blob parts must be an array");
    const text = parts.map(blobPartText).join("");
    if (text.length > storageValueLimit) throw new RangeError("native Blob size limit exceeded");
    return text;
  }};
  const normalizeBlobType = (options) => {{
    const type = options && typeof options.type === "string"
      ? options.type.toLowerCase()
      : "";
    return /^[\x20-\x7e]*$/.test(type) ? type : "";
  }};
  const BlobNative = typeof globalThis.__glassBlobConstructor === "function"
    ? globalThis.__glassBlobConstructor
    : function(parts, options) {{
    this.__glassNativeBlob = true;
    const payload = boundedBlobParts(parts);
    this._text = payload.text;
    this._bytes = payload.bytes;
    this.size = Array.isArray(this._bytes) ? this._bytes.length : this._text.length;
    this.type = normalizeBlobType(options);
  }};
  const blobUtf8Bytes = (text) => {{
    const bytes = [];
    for (let index = 0; index < text.length; index += 1) {{
      let code = text.charCodeAt(index);
      if (code >= 0xd800 && code <= 0xdbff && index + 1 < text.length) {{
        const low = text.charCodeAt(index + 1);
        if (low >= 0xdc00 && low <= 0xdfff) {{
          code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
          index += 1;
        }}
      }} else if (code >= 0xd800 && code <= 0xdfff) {{
        code = 0xfffd;
      }}
      if (code <= 0x7f) bytes.push(code);
      else if (code <= 0x7ff) bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
      else if (code <= 0xffff) bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
      else bytes.push(0xf0 | (code >> 18), 0x80 | ((code >> 12) & 0x3f), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    }}
    if (bytes.length > storageValueLimit) throw new RangeError("native Blob binary size limit exceeded");
    return bytes;
  }};
  const isBinaryBlobPart = (part) => (part && part.__glassNativeBlob === true && Array.isArray(part._bytes))
    || part instanceof ArrayBuffer
    || (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(part));
  const blobPartBytes = (part) => {{
    if (part && part.__glassNativeBlob === true) return Array.isArray(part._bytes)
      ? part._bytes.slice()
      : blobUtf8Bytes(part._text);
    if (part instanceof ArrayBuffer) {{
      if (part.byteLength > storageValueLimit) throw new RangeError("native Blob binary size limit exceeded");
      return Array.from(new Uint8Array(part));
    }}
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(part)) {{
      if (part.byteLength > storageValueLimit) throw new RangeError("native Blob binary size limit exceeded");
      return Array.from(new Uint8Array(part.buffer, part.byteOffset, part.byteLength));
    }}
    throw new TypeError("native Blob supports only text, buffers, or Blob parts");
  }};
  const boundedBlobParts = (parts) => {{
    if (parts === undefined || parts === null) return {{ text: "", bytes: null }};
    if (!Array.isArray(parts)) throw new TypeError("native Blob parts must be an array");
    let text = "";
    let bytes = null;
    for (const part of parts) {{
      if (bytes === null && !isBinaryBlobPart(part)) {{
        text += blobPartText(part);
        if (text.length > storageValueLimit) throw new RangeError("native Blob size limit exceeded");
        continue;
      }}
      if (bytes === null) bytes = blobUtf8Bytes(text);
      const partBytes = blobPartBytes(part);
      if (bytes.length + partBytes.length > storageValueLimit) throw new RangeError("native Blob binary size limit exceeded");
      for (const value of partBytes) bytes.push(value);
    }}
    return {{ text: bytes === null ? text : utf8TextFromBytes(bytes), bytes }};
  }};
  const base64Digit = (character) => {{
    const code = character.charCodeAt(0);
    if (code >= 65 && code <= 90) return code - 65;
    if (code >= 97 && code <= 122) return code - 97 + 26;
    if (code >= 48 && code <= 57) return code - 48 + 52;
    if (character === "+") return 62;
    if (character === "/") return 63;
    if (character === "=") return -2;
    return -1;
  }};
  const decodeBase64 = (encoded, maxBytes = storageValueLimit) => {{
    if (typeof encoded !== "string" || encoded.length % 4 !== 0) throw new TypeError("native response bytes are not valid base64");
    const bytes = [];
    for (let index = 0; index < encoded.length; index += 4) {{
      const first = base64Digit(encoded[index]);
      const second = base64Digit(encoded[index + 1]);
      const third = base64Digit(encoded[index + 2]);
      const fourth = base64Digit(encoded[index + 3]);
      if (first < 0 || second < 0 || third === -1 || fourth === -1
          || (third === -2 && fourth !== -2)
          || (third === -2 && index + 4 !== encoded.length)
          || (fourth === -2 && index + 4 !== encoded.length)) throw new TypeError("native response bytes are not valid base64");
      bytes.push((first << 2) | (second >> 4));
      if (third !== -2) bytes.push(((second & 15) << 4) | (third >> 2));
      if (fourth !== -2) bytes.push(((third & 3) << 6) | fourth);
    }}
    if (bytes.length > maxBytes) throw new RangeError("native binary payload exceeds its limit");
    return bytes;
  }};
  const encodeBase64 = (bytes, maxBytes = storageValueLimit) => {{
    if (!Array.isArray(bytes)) throw new TypeError("native Blob bytes are invalid");
    if (bytes.length > maxBytes) throw new RangeError("native binary payload exceeds its limit");
    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let encoded = "";
    for (let index = 0; index < bytes.length; index += 3) {{
      const first = bytes[index];
      const second = index + 1 < bytes.length ? bytes[index + 1] : 0;
      const third = index + 2 < bytes.length ? bytes[index + 2] : 0;
      for (const value of [first, second, third]) {{
        if (!Number.isInteger(value) || value < 0 || value > 255) throw new TypeError("native Blob bytes are invalid");
      }}
      encoded += alphabet[first >> 2];
      encoded += alphabet[((first & 3) << 4) | (second >> 4)];
      encoded += index + 1 < bytes.length ? alphabet[((second & 15) << 2) | (third >> 6)] : "=";
      encoded += index + 2 < bytes.length ? alphabet[third & 63] : "=";
    }}
    return encoded;
  }};
  {native_canvas_source}
  const utf8TextFromBytes = (bytes) => {{
    const continuation = value => value >= 0x80 && value <= 0xbf;
    let text = "";
    for (let index = 0; index < bytes.length;) {{
      const first = bytes[index];
      let codePoint = -1;
      let width = 1;
      if (first <= 0x7f) {{
        codePoint = first;
      }} else if (first >= 0xc2 && first <= 0xdf && continuation(bytes[index + 1])) {{
        codePoint = ((first & 0x1f) << 6) | (bytes[index + 1] & 0x3f);
        width = 2;
      }} else if (first === 0xe0 && bytes[index + 1] >= 0xa0 && bytes[index + 1] <= 0xbf && continuation(bytes[index + 2])) {{
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      }} else if (first >= 0xe1 && first <= 0xec && continuation(bytes[index + 1]) && continuation(bytes[index + 2])) {{
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      }} else if (first === 0xed && bytes[index + 1] >= 0x80 && bytes[index + 1] <= 0x9f && continuation(bytes[index + 2])) {{
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      }} else if (first >= 0xee && first <= 0xef && continuation(bytes[index + 1]) && continuation(bytes[index + 2])) {{
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      }} else if (first === 0xf0 && bytes[index + 1] >= 0x90 && bytes[index + 1] <= 0xbf && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {{
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      }} else if (first >= 0xf1 && first <= 0xf3 && continuation(bytes[index + 1]) && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {{
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      }} else if (first === 0xf4 && bytes[index + 1] >= 0x80 && bytes[index + 1] <= 0x8f && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {{
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      }}
      if (codePoint < 0) {{
        text += "\ufffd";
        index += 1;
      }} else {{
        text += String.fromCodePoint(codePoint);
        index += width;
      }}
    }}
    return text;
  }};
  const blobBytes = (blob) => Array.isArray(blob._bytes)
    ? blob._bytes.slice()
    : blobUtf8Bytes(blob._text);
  BlobNative.prototype.text = function() {{
    return Promise.resolve(Array.isArray(this._bytes) ? utf8TextFromBytes(this._bytes) : this._text);
  }};
  BlobNative.prototype.arrayBuffer = function() {{
    return Promise.resolve(new Uint8Array(blobBytes(this)).buffer);
  }};
  BlobNative.prototype.bytes = function() {{
    return Promise.resolve(new Uint8Array(blobBytes(this)));
  }};
  BlobNative.prototype.slice = function(start, end, contentType) {{
    const length = Array.isArray(this._bytes) ? this._bytes.length : this._text.length;
    const normalizePosition = (value, fallback) => {{
      if (value === undefined) return fallback;
      const number = Number(value);
      if (!Number.isFinite(number)) return fallback;
      return number < 0 ? Math.max(length + Math.trunc(number), 0) : Math.min(Math.trunc(number), length);
    }};
    const begin = normalizePosition(start, 0);
    const finish = normalizePosition(end, length);
    if (Array.isArray(this._bytes)) {{
      const blob = new BlobNative([], {{ type: contentType === undefined ? this.type : contentType }});
      blob._bytes = this._bytes.slice(begin, finish);
      blob._text = utf8TextFromBytes(blob._bytes);
      blob.size = blob._bytes.length;
      return blob;
    }}
    return new BlobNative([begin > finish ? "" : this._text.slice(begin, finish)], {{ type: contentType }});
  }};
  const nativeFileInstances = globalThis.__glassNativeFileInstances instanceof WeakSet
    ? globalThis.__glassNativeFileInstances
    : new WeakSet();
  globalThis.__glassNativeFileInstances = nativeFileInstances;
  const FileNative = typeof globalThis.__glassFileConstructor === "function"
    ? globalThis.__glassFileConstructor
    : function(parts, name, options) {{
    if (name === undefined) throw new TypeError("native File requires a name");
    BlobNative.call(this, parts, options);
    this.__glassNativeFile = true;
    this.name = String(name);
    const modified = options && Number.isFinite(Number(options.lastModified))
      ? Number(options.lastModified)
      : 0;
    this.lastModified = Math.max(0, modified);
    nativeFileInstances.add(this);
  }};
  if (typeof globalThis.__glassFileConstructor !== "function") {{
    FileNative.prototype = Object.create(BlobNative.prototype);
    FileNative.prototype.constructor = FileNative;
  }}
  globalThis.__glassBlobConstructor = BlobNative;
  globalThis.__glassFileConstructor = FileNative;
  globalThis.Blob = BlobNative;
  globalThis.File = FileNative;
  const FileListNative = function(entries) {{
    const files = Array.isArray(entries) ? entries : [];
    Object.defineProperty(this, "_files", {{ enumerable: false, configurable: false, value: files.slice() }});
    for (let index = 0; index < this._files.length; index += 1) {{
      Object.defineProperty(this, String(index), {{ enumerable: true, configurable: false, value: this._files[index] }});
    }}
  }};
  FileListNative.prototype.constructor = FileListNative;
  Object.defineProperty(FileListNative.prototype, "length", {{
    configurable: false,
    get() {{ return this._files.length; }},
  }});
  FileListNative.prototype.item = function(index) {{
    const numeric = Number(index);
    return Number.isInteger(numeric) && numeric >= 0 && numeric < this._files.length
      ? this._files[numeric]
      : null;
  }};
  FileListNative.prototype[Symbol.iterator] = function() {{ return this._files[Symbol.iterator](); }};
  globalThis.FileList = FileListNative;
  const nativeFileByteLimit = {native_file_bytes};
  const nativeFormBodyLimit = {native_form_body_bytes};
  const nativeFileFromEntry = (entry) => {{
    const bytes = decodeBase64(String(entry && entry.bytes || ""), nativeFileByteLimit);
    const file = new globalThis.File([], String(entry && entry.name || ""), {{
      type: String(entry && entry.type || ""),
      lastModified: Number(entry && entry.lastModified) || 0,
    }});
    file._bytes = bytes;
    file._text = utf8TextFromBytes(bytes);
    file.size = bytes.length;
    return file;
  }};
  const makeNativeFileList = (entries) => new globalThis.FileList(
    (Array.isArray(entries) ? entries : []).map(nativeFileFromEntry),
  );
  const formDataValue = (value, filename) => {{
    if (value && value.__glassNativeBlob === true) {{
      const defaultFilename = value.__glassNativeFile === true ? value.name : "blob";
      return {{
        kind: "file",
        value,
        filename: filename === undefined ? defaultFilename : String(filename),
      }};
    }}
    return String(value);
  }};
  const nativeFormDataInstances = globalThis.__glassNativeFormDataInstances instanceof WeakSet
    ? globalThis.__glassNativeFormDataInstances
    : new WeakSet();
  globalThis.__glassNativeFormDataInstances = nativeFormDataInstances;
  const FormDataNative = function(form) {{
    this.__glassFormData = true;
    this._entries = [];
    nativeFormDataInstances.add(this);
    if (form !== undefined && form !== null) this._entries = formDataEntries(form);
  }};
  const formDataEntryValue = entry => entry[1].kind === "file" ? entry[1].value : entry[1];
  const formDataIterator = (owner, kind) => {{
    let index = 0;
    const iterator = {{
      next() {{
        if (index >= owner._entries.length) return {{ value: undefined, done: true }};
        const entry = owner._entries[index++];
        if (kind === "keys") return {{ value: entry[0], done: false }};
        if (kind === "values") return {{ value: formDataEntryValue(entry), done: false }};
        return {{ value: [entry[0], formDataEntryValue(entry)], done: false }};
      }},
      [Symbol.iterator]() {{ return this; }},
    }};
    return iterator;
  }};
  FormDataNative.prototype.append = function(name, value, filename) {{
    this._entries.push([String(name), formDataValue(value, filename)]);
  }};
  FormDataNative.prototype.set = function(name, value, filename) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
    this._entries.push([key, formDataValue(value, filename)]);
  }};
  FormDataNative.prototype.delete = function(name) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
  }};
  FormDataNative.prototype.get = function(name) {{
    const key = String(name);
    const entry = this._entries.find(candidate => candidate[0] === key);
    return entry ? formDataEntryValue(entry) : null;
  }};
  FormDataNative.prototype.getAll = function(name) {{
    const key = String(name);
    return this._entries
      .filter(entry => entry[0] === key)
      .map(formDataEntryValue);
  }};
  FormDataNative.prototype.has = function(name) {{
    const key = String(name);
    return this._entries.some(entry => entry[0] === key);
  }};
  FormDataNative.prototype.entries = function() {{
    return formDataIterator(this, "entries");
  }};
  FormDataNative.prototype.keys = function() {{
    return formDataIterator(this, "keys");
  }};
  FormDataNative.prototype.values = function() {{
    return formDataIterator(this, "values");
  }};
  FormDataNative.prototype.forEach = function(callback, thisArg) {{
    if (typeof callback !== "function") throw new TypeError("FormData callback must be callable");
    this._entries.forEach(entry => callback.call(
      thisArg,
      formDataEntryValue(entry),
      entry[0],
      this,
    ));
  }};
  FormDataNative.prototype[Symbol.iterator] = FormDataNative.prototype.entries;
  const escapeFormDataName = value => String(value)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, "\\\"")
    .replace(/\r/g, "%0D")
    .replace(/\n/g, "%0A");
  const serializeFormData = (formData, requestId) => {{
    const boundary = "----GlassNativeForm" + requestId;
    const hasBinaryFile = formData._entries.some(entry =>
      entry[1].kind === "file" && Array.isArray(entry[1].value._bytes)
    );
    if (hasBinaryFile) {{
      const bytes = [];
      const appendBytes = source => {{
        if (bytes.length + source.length > nativeFormBodyLimit) throw new RangeError("native FormData body limit exceeded");
        for (const value of source) bytes.push(value);
      }};
      const appendText = value => appendBytes(blobUtf8Bytes(String(value)));
      for (const entry of formData._entries) {{
        appendText("--" + boundary + "\r\n");
        const file = entry[1].kind === "file" ? entry[1] : null;
        appendText("Content-Disposition: form-data; name=\"" + escapeFormDataName(entry[0]) + "\"");
        if (file) appendText("; filename=\"" + escapeFormDataName(file.filename) + "\"");
        appendText("\r\n");
        if (file) appendText("Content-Type: " + (file.value.type || "application/octet-stream") + "\r\n");
        appendText("\r\n");
        appendBytes(file ? blobBytes(file.value) : blobUtf8Bytes(entry[1]));
        appendText("\r\n");
      }}
      appendText("--" + boundary + "--\r\n");
      return {{
        body: utf8TextFromBytes(bytes),
        bodyBase64: encodeBase64(bytes, nativeFormBodyLimit),
        contentType: "multipart/form-data; boundary=" + boundary,
      }};
    }}
    let body = "";
    for (const entry of formData._entries) {{
      body += "--" + boundary + "\r\n";
      const file = entry[1].kind === "file" ? entry[1] : null;
      body += "Content-Disposition: form-data; name=\"" + escapeFormDataName(entry[0]) + "\"";
      if (file) body += "; filename=\"" + escapeFormDataName(file.filename) + "\"";
      body += "\r\n";
      if (file) body += "Content-Type: " + (file.value.type || "application/octet-stream") + "\r\n";
      body += "\r\n";
      body += file ? file.value._text : entry[1];
      body += "\r\n";
    }}
    body += "--" + boundary + "--\r\n";
    return {{ body, bodyBase64: null, contentType: "multipart/form-data; boundary=" + boundary }};
  }};
  globalThis.FormData = FormDataNative;
  const appendUrlSearchParam = (target, name, value) => {{
    const key = String(name);
    const normalizedValue = String(value);
    if (key.length > storageKeyLimit) throw new RangeError("native URLSearchParams name exceeds its limit");
    if (normalizedValue.length > storageValueLimit) throw new RangeError("native URLSearchParams value exceeds its limit");
    if (target._entries.length >= storageEntryLimit) throw new RangeError("native URLSearchParams entry limit exceeded");
    target._entries.push([key, normalizedValue]);
  }};
  const URLSearchParamsNative = function(init) {{
    this.__glassUrlSearchParams = true;
    this._entries = [];
    if (init === undefined || init === null) return;
    if (typeof init === "string") {{
      const source = init.startsWith("?") ? init.slice(1) : init;
      for (const part of source.split("&")) {{
        if (!part) continue;
        const separator = part.indexOf("=");
        const decode = value => decodeURIComponent(String(value).replace(/\+/g, " "));
        const name = separator < 0 ? part : part.slice(0, separator);
        const value = separator < 0 ? "" : part.slice(separator + 1);
        appendUrlSearchParam(this, decode(name), decode(value));
      }}
      return;
    }}
    if (init.__glassUrlSearchParams === true) {{
      init._entries.forEach(entry => appendUrlSearchParam(this, entry[0], entry[1]));
      return;
    }}
    if (Array.isArray(init)) {{
      for (const pair of init) {{
        if (!Array.isArray(pair) || pair.length !== 2) throw new TypeError("native URLSearchParams pairs must contain two values");
        appendUrlSearchParam(this, pair[0], pair[1]);
      }}
      return;
    }}
    if (typeof init[Symbol.iterator] === "function") {{
      for (const pair of init) {{
        if (!pair || typeof pair[Symbol.iterator] !== "function") throw new TypeError("native URLSearchParams iterable pairs must be iterable");
        const values = Array.from(pair);
        if (values.length !== 2) throw new TypeError("native URLSearchParams iterable pairs must contain two values");
        appendUrlSearchParam(this, values[0], values[1]);
      }}
      return;
    }}
    if (typeof init === "object") {{
      for (const name of Object.keys(init)) appendUrlSearchParam(this, name, init[name]);
      return;
    }}
    throw new TypeError("native URLSearchParams accepts text, pairs, records, or URLSearchParams");
  }};
  URLSearchParamsNative.prototype.append = function(name, value) {{
    appendUrlSearchParam(this, name, value);
  }};
  URLSearchParamsNative.prototype.set = function(name, value) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
    appendUrlSearchParam(this, key, value);
  }};
  URLSearchParamsNative.prototype.delete = function(name, value) {{
    const key = String(name);
    if (arguments.length > 1) {{
      const normalizedValue = String(value);
      this._entries = this._entries.filter(entry => entry[0] !== key || entry[1] !== normalizedValue);
    }} else {{
      this._entries = this._entries.filter(entry => entry[0] !== key);
    }}
  }};
  URLSearchParamsNative.prototype.get = function(name) {{
    const key = String(name);
    const entry = this._entries.find(candidate => candidate[0] === key);
    return entry ? entry[1] : null;
  }};
  URLSearchParamsNative.prototype.getAll = function(name) {{
    const key = String(name);
    return this._entries.filter(entry => entry[0] === key).map(entry => entry[1]);
  }};
  URLSearchParamsNative.prototype.has = function(name, value) {{
    const key = String(name);
    return arguments.length < 2
      ? this._entries.some(entry => entry[0] === key)
      : this._entries.some(entry => entry[0] === key && entry[1] === String(value));
  }};
  Object.defineProperty(URLSearchParamsNative.prototype, "size", {{
    get() {{ return this._entries.length; }},
  }});
  const urlSearchParamsIterator = (owner, kind) => {{
    let index = 0;
    const iterator = {{
      next() {{
        if (index >= owner._entries.length) return {{ value: undefined, done: true }};
        const entry = owner._entries[index++];
        if (kind === "keys") return {{ value: entry[0], done: false }};
        if (kind === "values") return {{ value: entry[1], done: false }};
        return {{ value: [entry[0], entry[1]], done: false }};
      }},
      [Symbol.iterator]() {{ return this; }},
    }};
    return iterator;
  }};
  URLSearchParamsNative.prototype.entries = function() {{
    return urlSearchParamsIterator(this, "entries");
  }};
  URLSearchParamsNative.prototype.keys = function() {{
    return urlSearchParamsIterator(this, "keys");
  }};
  URLSearchParamsNative.prototype.values = function() {{
    return urlSearchParamsIterator(this, "values");
  }};
  URLSearchParamsNative.prototype.forEach = function(callback, thisArg) {{
    if (typeof callback !== "function") throw new TypeError("native URLSearchParams callback must be callable");
    this._entries.slice().forEach(entry => callback.call(thisArg, entry[1], entry[0], this));
  }};
  URLSearchParamsNative.prototype.sort = function() {{
    for (let index = 1; index < this._entries.length; index += 1) {{
      const current = this._entries[index];
      let position = index;
      while (position > 0 && this._entries[position - 1][0] > current[0]) {{
        this._entries[position] = this._entries[position - 1];
        position -= 1;
      }}
      this._entries[position] = current;
    }}
  }};
  URLSearchParamsNative.prototype[Symbol.iterator] = URLSearchParamsNative.prototype.entries;
  URLSearchParamsNative.prototype.toString = function() {{
    const encode = value => encodeURIComponent(String(value))
      .replace(/[!'()~]/g, character => "%" + character.charCodeAt(0).toString(16).toUpperCase())
      .replace(/%20/g, "+");
    return this._entries.map(entry => encode(entry[0]) + "=" + encode(entry[1])).join("&");
  }};
  globalThis.URLSearchParams = URLSearchParamsNative;
  const nativeUrlParts = (input) => {{
    const source = String(input);
    const hashIndex = source.indexOf("#");
    const hash = hashIndex < 0 ? "" : source.slice(hashIndex);
    const withoutHash = hashIndex < 0 ? source : source.slice(0, hashIndex);
    const queryIndex = withoutHash.indexOf("?");
    const search = queryIndex < 0 ? "" : withoutHash.slice(queryIndex);
    const main = queryIndex < 0 ? withoutHash : withoutHash.slice(0, queryIndex);
    const match = main.match(/^([A-Za-z][A-Za-z0-9+.-]*:)(.*)$/);
    if (!match) throw new TypeError("native URL requires an absolute or resolvable URL");
    const protocol = match[1].toLowerCase();
    let rest = match[2];
    let authority = "";
    let pathname = "";
    let hasAuthorityDelimiter = false;
    if (rest.startsWith("//")) {{
      hasAuthorityDelimiter = true;
      rest = rest.slice(2);
      const slash = rest.indexOf("/");
      if (slash < 0) {{
        authority = rest;
        pathname = protocol === "http:" || protocol === "https:" ? "/" : "";
      }} else {{
        authority = rest.slice(0, slash);
        pathname = rest.slice(slash) || "/";
      }}
    }} else pathname = rest || ((protocol === "http:" || protocol === "https:") ? "/" : "");
    const at = authority.lastIndexOf("@");
    const userInfo = at < 0 ? "" : authority.slice(0, at);
    const credentialSeparator = userInfo.indexOf(":");
    const username = credentialSeparator < 0 ? userInfo : userInfo.slice(0, credentialSeparator);
    const password = credentialSeparator < 0 ? "" : userInfo.slice(credentialSeparator + 1);
    const hostPort = at < 0 ? authority : authority.slice(at + 1);
    let hostname = hostPort;
    let port = "";
    if (hostname.startsWith("[")) {{
      const closing = hostname.indexOf("]");
      if (closing >= 0) {{
        const suffix = hostname.slice(closing + 1);
        hostname = hostname.slice(0, closing + 1);
        if (suffix.startsWith(":")) port = suffix.slice(1);
      }}
    }} else {{
      const colon = hostname.lastIndexOf(":");
      if (colon >= 0 && /^[0-9]*$/.test(hostname.slice(colon + 1))) {{
        port = hostname.slice(colon + 1);
        hostname = hostname.slice(0, colon);
      }}
    }}
    hostname = hostname.toLowerCase();
    const origin = protocol === "http:" || protocol === "https:"
      ? protocol + "//" + hostPort.toLowerCase()
      : "null";
    const prefix = authority || (protocol === "file:" && hasAuthorityDelimiter)
      ? protocol + "//" + authority
      : protocol;
    const href = prefix + pathname + search + hash;
    return {{
      href,
      protocol,
      origin,
      authority,
      username,
      password,
      host: hostPort,
      hostname,
      port,
      pathname,
      search,
      hash,
      hasAuthorityDelimiter,
    }};
  }};
  const nativeUrlResolve = (input, base) => {{
    const value = String(input);
    const baseHref = base === undefined
      ? undefined
      : base && base.__glassUrl === true ? base.href : String(base);
    try {{
      return __glassNativeCanonicalUrl(value, baseHref);
    }} catch (_) {{
      throw new TypeError("native URL is invalid");
    }}
  }};
  const URLNative = function(input, base) {{
    const parts = nativeUrlParts(nativeUrlResolve(input && input.__glassUrl === true ? input.href : input, base));
    const state = {{
      hasAuthority: Boolean(parts.authority),
      prefix: parts.authority || (parts.protocol === "file:" && parts.hasAuthorityDelimiter)
        ? parts.protocol + "//" + parts.authority
        : parts.protocol,
      origin: parts.origin,
      protocol: parts.protocol,
      username: parts.username,
      password: parts.password,
      host: parts.host,
      hostname: parts.hostname,
      port: parts.port,
      pathname: parts.pathname,
      search: parts.search,
      hash: parts.hash,
    }};
    const searchParams = new URLSearchParamsNative(parts.search);
    const updateParts = (next) => {{
      state.hasAuthority = Boolean(next.authority);
      state.prefix = next.authority || (next.protocol === "file:" && next.hasAuthorityDelimiter)
        ? next.protocol + "//" + next.authority
        : next.protocol;
      state.origin = next.origin;
      state.protocol = next.protocol;
      state.username = next.username;
      state.password = next.password;
      state.host = next.host;
      state.hostname = next.hostname;
      state.port = next.port;
      state.pathname = next.pathname;
      state.search = next.search;
      state.hash = next.hash;
      searchParams._entries = new URLSearchParamsNative(next.search)._entries;
    }};
    const credentialsFor = (username, password) => username || password
      ? username + (password ? ":" + password : "") + "@"
      : "";
    const currentCredentials = () => credentialsFor(state.username, state.password);
    const currentAuthority = () => currentCredentials() + state.host;
    const requireAuthority = () => {{
      if (!state.hasAuthority) throw new TypeError("native URL authority mutation requires a host");
    }};
    const replaceAuthority = (authority, protocol = state.protocol) => {{
      requireAuthority();
      const next = nativeUrlParts(protocol + "//" + authority + state.pathname + state.search + state.hash);
      updateParts(next);
    }};
    const validateHost = value => {{
      const source = String(value);
      if (!source || /[\/\?#@\s]/.test(source)) throw new TypeError("native URL host is invalid");
      if (!source.startsWith("[") && source.includes(":") && !/:[0-9]+$/.test(source))
        throw new TypeError("native URL host port is invalid");
      const parsed = nativeUrlParts(state.protocol + "//" + source + "/");
      if (!parsed.host || (parsed.port && Number(parsed.port) > 65535))
        throw new TypeError("native URL host is invalid");
      return parsed.host;
    }};
    const normalizeCredential = value => encodeURIComponent(String(value));
    const currentHref = () => state.prefix + state.pathname + state.search + state.hash;
    const syncSearch = () => {{
      const encoded = searchParams.toString();
      state.search = encoded ? "?" + encoded : "";
    }};
    for (const method of ["append", "set", "delete", "sort"]) {{
      const original = searchParams[method];
      searchParams[method] = function(...args) {{
        const result = original.apply(searchParams, args);
        syncSearch();
        return result;
      }};
    }}
    Object.defineProperty(this, "__glassUrl", {{ value: true }});
    const define = (name, getter) => Object.defineProperty(this, name, {{ enumerable: true, get: getter }});
    Object.defineProperty(this, "href", {{
      enumerable: true,
      get: currentHref,
      set: value => updateParts(nativeUrlParts(nativeUrlResolve(value, currentHref()))),
    }});
    define("origin", () => state.origin);
    Object.defineProperty(this, "protocol", {{
      enumerable: true,
      get: () => state.protocol,
      set: value => {{
        const source = String(value).toLowerCase();
        const protocol = source.endsWith(":") ? source : source + ":";
        if (!["http:", "https:"].includes(protocol)) throw new TypeError("native URL protocol is unsupported");
        if (protocol !== state.protocol) replaceAuthority(currentAuthority(), protocol);
      }},
    }});
    Object.defineProperty(this, "username", {{
      enumerable: true,
      get: () => state.username,
      set: value => replaceAuthority(credentialsFor(normalizeCredential(value), state.password) + state.host),
    }});
    Object.defineProperty(this, "password", {{
      enumerable: true,
      get: () => state.password,
      set: value => replaceAuthority(credentialsFor(state.username, normalizeCredential(value)) + state.host),
    }});
    Object.defineProperty(this, "host", {{
      enumerable: true,
      get: () => state.host,
      set: value => replaceAuthority(currentCredentials() + validateHost(value)),
    }});
    Object.defineProperty(this, "hostname", {{
      enumerable: true,
      get: () => state.hostname,
      set: value => {{
        const source = String(value);
        if (!source || /[\/\?#@\s]/.test(source) || (!source.startsWith("[") && source.includes(":")))
          throw new TypeError("native URL hostname is invalid");
        if (source.startsWith("[") && !/^\[[^\]]+\]$/.test(source))
          throw new TypeError("native URL hostname is invalid");
        const host = source + (state.port ? ":" + state.port : "");
        replaceAuthority(currentCredentials() + validateHost(host));
      }},
    }});
    Object.defineProperty(this, "port", {{
      enumerable: true,
      get: () => state.port,
      set: value => {{
        const source = String(value);
        if (source && (!/^[0-9]+$/.test(source) || Number(source) > 65535))
          throw new TypeError("native URL port is invalid");
        const host = state.hostname + (source ? ":" + source : "");
        replaceAuthority(currentCredentials() + validateHost(host));
      }},
    }});
    Object.defineProperty(this, "pathname", {{
      enumerable: true,
      get: () => state.pathname,
      set: value => {{
        const source = String(value).replace(/\\/g, "%5C").replace(/[\?#]/g, character => encodeURIComponent(character));
        const path = source.startsWith("/") ? source : "/" + source;
        const canonical = __glassNativeCanonicalUrl("http://glass.invalid" + path, undefined);
        state.pathname = nativeUrlParts(canonical).pathname;
      }},
    }});
    Object.defineProperty(this, "search", {{
      enumerable: true,
      get: () => state.search,
      set: value => {{
        const source = String(value);
        searchParams._entries = new URLSearchParamsNative(source)._entries;
        syncSearch();
      }},
    }});
    Object.defineProperty(this, "hash", {{
      enumerable: true,
      get: () => state.hash,
      set: value => {{
        const source = String(value);
        const canonical = __glassNativeCanonicalUrl("http://glass.invalid/" + (!source ? "" : source.startsWith("#") ? source : "#" + source), undefined);
        state.hash = nativeUrlParts(canonical).hash;
      }},
    }});
    define("searchParams", () => searchParams);
    Object.freeze(this);
  }};
  URLNative.prototype.toString = function() {{ return this.href; }};
  URLNative.prototype.toJSON = function() {{ return this.href; }};
  globalThis.URL = URLNative;
  const nativeObjectUrlRegistry = globalThis.__glassObjectUrlRegistry instanceof Map
    ? globalThis.__glassObjectUrlRegistry
    : new Map();
  let nativeNextObjectUrlId = Number.isSafeInteger(globalThis.__glassNextObjectUrlId)
    ? globalThis.__glassNextObjectUrlId
    : 1;
  const nativeObjectUrlOrigin = () => {{
    try {{ return nativeUrlParts(nativeUrlResolve(host.url, undefined)).origin; }}
    catch (_) {{ return "null"; }}
  }};
  URLNative.createObjectURL = function(object) {{
    if (!object || object.__glassNativeBlob !== true)
      throw new TypeError("native URL.createObjectURL requires a Blob");
    if (nativeObjectUrlRegistry.size >= {max_commands})
      throw new RangeError("native object URL limit exceeded");
    const origin = nativeObjectUrlOrigin();
    let url;
    do {{
      url = "blob:" + origin + "/glass-native-" + nativeNextObjectUrlId;
      nativeNextObjectUrlId += 1;
    }} while (nativeObjectUrlRegistry.has(url));
    nativeObjectUrlRegistry.set(url, {{ blob: object, origin }});
    globalThis.__glassNextObjectUrlId = nativeNextObjectUrlId;
    return url;
  }};
  URLNative.revokeObjectURL = function(value) {{
    nativeObjectUrlRegistry.delete(String(value));
  }};
  globalThis.__glassObjectUrlRegistry = nativeObjectUrlRegistry;
  globalThis.__glassGetObjectUrlResource = (value) => {{
    const entry = nativeObjectUrlRegistry.get(String(value));
    if (!entry) return null;
    return {{
      contentType: entry.blob.type || null,
      bodyBase64: encodeBase64(blobBytes(entry.blob), storageValueLimit),
    }};
  }};
  globalThis.__glassInstallObjectUrlTransfers = (transfers) => {{
    if (!Array.isArray(transfers)) throw new TypeError("native object URL transfers are invalid");
    for (const transfer of transfers) {{
      if (!transfer || typeof transfer !== "object")
        throw new TypeError("native object URL transfer is invalid");
      const href = String(transfer.href || "");
      if (!href.startsWith("blob:"))
        throw new TypeError("native object URL transfer scheme is invalid");
      const resource = transfer.resource;
      if (!resource || typeof resource !== "object")
        throw new TypeError("native object URL transfer resource is invalid");
      const bytes = decodeBase64(String(resource.body || ""));
      const contentType = resource.contentType === null || resource.contentType === undefined
        ? ""
        : String(resource.contentType);
      if (contentType.length > storageValueLimit)
        throw new RangeError("native object URL transfer content type is too large");
      if (!nativeObjectUrlRegistry.has(href) && nativeObjectUrlRegistry.size >= {max_commands})
        throw new RangeError("native object URL limit exceeded");
      const blob = new BlobNative([new Uint8Array(bytes)], {{ type: contentType }});
      nativeObjectUrlRegistry.set(href, {{ blob, origin: nativeObjectUrlOrigin() }});
    }}
    return null;
  }};
  const requestHeaderNameNative = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const nativeFetchMethod = (value) => {{
    const method = String(value).toUpperCase();
    if (!method || method.length > {fetch_method_limit} || !requestHeaderNameNative.test(method)
        || ["CONNECT", "TRACE", "TRACK"].includes(method))
      throw new TypeError("native Request method is unsupported");
    return method;
  }};
  const forbiddenRequestHeaderNative = (name) => [
    "accept-charset", "accept-encoding", "access-control-request-headers",
    "access-control-request-method", "connection", "content-length",
    "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
    "origin", "referer", "te", "trailer", "transfer-encoding", "upgrade",
    "user-agent", "via",
  ].includes(name) || name.startsWith("proxy-") || name.startsWith("sec-");
  const validateNativeRequestHeader = (name, value) => {{
    const normalizedName = String(name).toLowerCase();
    if (!requestHeaderNameNative.test(String(name)) || normalizedName.length > {fetch_header_name_limit}) throw new TypeError("native Headers name is invalid");
    if (forbiddenRequestHeaderNative(normalizedName)) throw new TypeError("native Headers name is forbidden");
    const normalizedValue = String(value);
    if (normalizedValue.length > {fetch_header_value_limit} || /[\u0000-\u001f\u007f]/.test(normalizedValue)) throw new TypeError("native Headers value is invalid");
    return [normalizedName, normalizedValue];
  }};
  const validateNativeHeaderEntries = (entries) => {{
    if (entries.length > {fetch_header_count_limit}) throw new RangeError("native Headers limit exceeded");
    let totalBytes = 0;
    for (const entry of entries) {{
      totalBytes += entry[0].length + entry[1].length;
      if (totalBytes > {fetch_header_bytes_limit}) throw new RangeError("native Headers exceed their limit");
    }}
  }};
  const HeadersNative = function(init) {{
    this.__glassHeaders = true;
    this._entries = [];
    if (init === undefined || init === null) return;
    if (init.__glassHeaders === true) {{
      init._entries.forEach(entry => this.append(entry[0], entry[1]));
      return;
    }}
    if (Array.isArray(init)) {{
      for (const entry of init) {{
        if (!Array.isArray(entry) || entry.length !== 2) throw new TypeError("native Headers pairs must contain two values");
        this.append(entry[0], entry[1]);
      }}
      return;
    }}
    if (typeof init !== "object") throw new TypeError("native Headers accepts records, pairs, or Headers");
    for (const name of Object.keys(init)) this.append(name, init[name]);
  }};
  HeadersNative.prototype.append = function(name, value) {{
    const normalized = validateNativeRequestHeader(name, value);
    const existing = this._entries.find(entry => entry[0] === normalized[0]);
    const nextValue = existing ? existing[1] + ", " + normalized[1] : normalized[1];
    const nextEntries = existing
      ? this._entries.map(entry => entry[0] === normalized[0] ? [entry[0], nextValue] : entry)
      : this._entries.concat([[normalized[0], normalized[1]]]);
    validateNativeHeaderEntries(nextEntries);
    this._entries = nextEntries;
  }};
  HeadersNative.prototype.set = function(name, value) {{
    const normalized = validateNativeRequestHeader(name, value);
    const nextEntries = this._entries.filter(entry => entry[0] !== normalized[0]);
    nextEntries.push(normalized);
    validateNativeHeaderEntries(nextEntries);
    this._entries = nextEntries;
  }};
  HeadersNative.prototype.delete = function(name) {{
    const normalizedName = validateNativeRequestHeader(name, "")[0];
    this._entries = this._entries.filter(entry => entry[0] !== normalizedName);
  }};
  HeadersNative.prototype.get = function(name) {{
    const normalizedName = validateNativeRequestHeader(name, "")[0];
    const entry = this._entries.find(candidate => candidate[0] === normalizedName);
    return entry ? entry[1] : null;
  }};
  HeadersNative.prototype.has = function(name) {{
    return this.get(name) !== null;
  }};
  const nativeHeaderIterator = (owner, kind) => {{
    let index = 0;
    const iterator = {{
      next() {{
        if (index >= owner._entries.length) return {{ value: undefined, done: true }};
        const entry = owner._entries[index++];
        if (kind === "keys") return {{ value: entry[0], done: false }};
        if (kind === "values") return {{ value: entry[1], done: false }};
        return {{ value: [entry[0], entry[1]], done: false }};
      }},
      [Symbol.iterator]() {{ return this; }},
    }};
    return iterator;
  }};
  HeadersNative.prototype.entries = function() {{ return nativeHeaderIterator(this, "entries"); }};
  HeadersNative.prototype.keys = function() {{ return nativeHeaderIterator(this, "keys"); }};
  HeadersNative.prototype.values = function() {{ return nativeHeaderIterator(this, "values"); }};
  HeadersNative.prototype.forEach = function(callback, thisArg) {{
    if (typeof callback !== "function") throw new TypeError("native Headers callback must be callable");
    this._entries.slice().forEach(entry => callback.call(thisArg, entry[1], entry[0], this));
  }};
  Object.defineProperty(HeadersNative.prototype, "size", {{
    get() {{ return this._entries.length; }},
  }});
  HeadersNative.prototype[Symbol.iterator] = HeadersNative.prototype.entries;
  globalThis.Headers = HeadersNative;
  const nativeBodylessMethods = ["GET", "HEAD"];
  const nativeRequestUrl = (value) => {{
    const source = value && value.__glassUrl === true ? value.href : String(value);
    try {{
      return nativeUrlParts(nativeUrlResolve(source, host.url)).href;
    }} catch (_) {{
      throw new TypeError("native Request URL is invalid");
    }}
  }};
  const nativeRequestReferrerState = globalThis.__glassNativeRequestReferrerState instanceof WeakMap
    ? globalThis.__glassNativeRequestReferrerState
    : new WeakMap();
  globalThis.__glassNativeRequestReferrerState = nativeRequestReferrerState;
  const nativeReferrerPolicyValues = new Set([
    "", "no-referrer", "no-referrer-when-downgrade", "same-origin", "origin",
    "strict-origin", "origin-when-cross-origin", "strict-origin-when-cross-origin", "unsafe-url",
  ]);
  const nativeRequestReferrer = (value, inherited) => {{
    const client = {{ referrer: "about:client", url: host.url }};
    if (value === undefined) return inherited || client;
    const source = String(value);
    if (source === "") return {{ referrer: "", url: "" }};
    if (source === "about:client") return inherited || client;
    let href;
    try {{ href = nativeRequestUrl(source); }}
    catch (_) {{ throw new TypeError("native Request referrer is invalid"); }}
    if (nativeUrlParts(href).origin !== nativeUrlParts(host.url).origin) return client;
    const url = new URLNative(href);
    url.username = "";
    url.password = "";
    url.hash = "";
    return {{ referrer: url.href, url: url.href }};
  }};
  const nativeRequestReferrerPolicy = (value) => {{
    const policy = value === undefined ? "" : String(value);
    if (!nativeReferrerPolicyValues.has(policy))
      throw new TypeError("native Request referrer policy is unsupported");
    return policy;
  }};
  const RequestNative = function(input, init) {{
    const source = input && input.__glassRequest === true ? input : null;
    const sourceUrl = input && input.__glassUrl === true ? input : null;
    const overrides = init && typeof init === "object" ? init : {{}};
    const requestInitIsNonEmpty = Object.keys(overrides).length > 0;
    const hasBodyOverride = Object.prototype.hasOwnProperty.call(overrides, "body");
    if (source && !hasBodyOverride && (source.bodyUsed || (source.body && source.body.locked)))
      throw new TypeError("native Request body is unusable");
    if (!source && !sourceUrl && typeof input !== "string")
      throw new TypeError("native Request URL must be a string");
    const href = nativeRequestUrl(source ? source.url : sourceUrl || input);
    const settings = Object.assign({{}}, source ? source._settings : {{}}, overrides);
    const inheritedPayload = source && !Object.prototype.hasOwnProperty.call(overrides, "body")
      ? source.__glassRequestBodyPayload
      : null;
    let payload;
    if (inheritedPayload && inheritedPayload.stream) {{
      const [sourceBody, cloneBody] = source.body.tee();
      source.__glassRequestBodyPayload.stream = sourceBody;
      source.__glassRequestBodyState.stream = sourceBody;
      payload = {{ bodyNull: false, bytes: null, stream: cloneBody, contentType: inheritedPayload.contentType }};
    }} else if (inheritedPayload) {{
      payload = {{
        bodyNull: inheritedPayload.bodyNull,
        bytes: inheritedPayload.bytes.slice(),
        stream: null,
        contentType: inheritedPayload.contentType,
      }};
    }} else {{
      payload = nativeRequestBodyPayload(settings);
    }}
    const method = nativeFetchMethod(settings.method === undefined ? "GET" : settings.method);
    if (nativeBodylessMethods.includes(method) && settings.body !== undefined && settings.body !== null)
      throw new TypeError("native " + method + " Requests must not have a body");
    const mode = settings.mode === undefined ? "cors" : String(settings.mode).toLowerCase();
    if (!["cors", "no-cors", "same-origin"].includes(mode)) throw new TypeError("native Request mode is unsupported");
    const redirect = settings.redirect === undefined ? "follow" : String(settings.redirect).toLowerCase();
    if (!["follow", "error", "manual"].includes(redirect)) throw new TypeError("native Request redirect mode is unsupported");
    const cache = settings.cache === undefined ? "default" : String(settings.cache).toLowerCase();
    if (!["default", "no-store", "reload", "no-cache", "force-cache", "only-if-cached"].includes(cache))
      throw new TypeError("native Request cache mode is unsupported");
    if (cache === "only-if-cached" && mode !== "same-origin")
      throw new TypeError("native only-if-cached Requests require same-origin mode");
    const credentialsMode = settings.credentials === undefined ? "same-origin" : String(settings.credentials);
    if (!["omit", "same-origin", "include"].includes(credentialsMode))
      throw new TypeError("native Request credentials mode is unsupported");
    const referrer = nativeRequestReferrer(
      Object.prototype.hasOwnProperty.call(overrides, "referrer") ? overrides.referrer : undefined,
      source && !requestInitIsNonEmpty ? nativeRequestReferrerState.get(source) : null,
    );
    const referrerPolicy = nativeRequestReferrerPolicy(
      source && requestInitIsNonEmpty
        && !Object.prototype.hasOwnProperty.call(overrides, "referrerPolicy")
        ? ""
        : settings.referrerPolicy,
    );
    const headers = new HeadersNative(settings.headers);
    settings.method = method;
    settings.mode = mode;
    settings.redirect = redirect;
    settings.cache = cache;
    settings.credentials = credentialsMode;
    settings.referrer = referrer.referrer;
    settings.referrerPolicy = referrerPolicy;
    settings.headers = headers;
    Object.defineProperty(this, "__glassRequest", {{ value: true }});
    Object.defineProperty(this, "_settings", {{ value: settings }});
    Object.defineProperty(this, "__glassRequestBodyPayload", {{ value: payload }});
    Object.defineProperty(this, "__glassRequestBodyState", {{ value: {{ used: false, stream: null }} }});
    this.method = method;
    this.url = href;
    this.headers = headers;
    this.mode = mode;
    this.redirect = redirect;
    this.cache = cache;
    this.credentials = credentialsMode;
    this.referrer = referrer.referrer;
    this.referrerPolicy = referrerPolicy;
    this.signal = settings.signal === undefined ? null : settings.signal;
    const requestBodyState = this.__glassRequestBodyState;
    requestBodyState.stream = payload.bodyNull
      ? null
      : payload.stream || new ReadableStreamNative(payload.bytes, null, () => {{ requestBodyState.used = true; }});
    Object.defineProperty(this, "body", {{
      configurable: true,
      enumerable: true,
      get() {{ return requestBodyState.stream; }},
    }});
    Object.defineProperty(this, "bodyUsed", {{
      configurable: true,
      get() {{ return nativeRequestBodyIsUsed(this); }},
    }});
    nativeRequestReferrerState.set(this, referrer);
    Object.freeze(this);
  }};
  RequestNative.prototype.clone = function() {{
    if (this.bodyUsed || (this.body && this.body.locked)) throw new TypeError("native Request body is unusable");
    return new RequestNative(this);
  }};
  globalThis.Request = RequestNative;
  const nativeAbortError = () => {{
    const error = new Error("The operation was aborted");
    error.name = "AbortError";
    return error;
  }};
  const nativeTimeoutError = () => {{
    const error = new Error("The XMLHttpRequest operation timed out");
    error.name = "TimeoutError";
    return error;
  }};
  const nativeOpaqueResponseError = () => {{
    const error = new TypeError("opaque native fetch response body is unavailable");
    error.name = "TypeError";
    return error;
  }};
  const AbortSignalNative = function() {{
    this.aborted = false;
    this.reason = undefined;
    this.onabort = null;
    this._abortListeners = [];
  }};
  AbortSignalNative.prototype.addEventListener = function(type, listener) {{
    if (type !== "abort" || typeof listener !== "function" || this.aborted) return;
    if (!this._abortListeners.includes(listener)) this._abortListeners.push(listener);
  }};
  AbortSignalNative.prototype.removeEventListener = function(type, listener) {{
    if (type !== "abort") return;
    this._abortListeners = this._abortListeners.filter(candidate => candidate !== listener);
  }};
  AbortSignalNative.prototype.throwIfAborted = function() {{
    if (this.aborted) throw this.reason;
  }};
  const dispatchAbort = (signal) => {{
    const event = {{ type: "abort", target: signal }};
    const listeners = signal._abortListeners.slice();
    signal._abortListeners = [];
    for (const listener of listeners) listener.call(signal, event);
    if (typeof signal.onabort === "function") signal.onabort.call(signal, event);
  }};
  const AbortControllerNative = function() {{
    this.signal = new AbortSignalNative();
  }};
  AbortControllerNative.prototype.abort = function(reason) {{
    if (this.signal.aborted) return;
    this.signal.aborted = true;
    this.signal.reason = reason === undefined ? nativeAbortError() : reason;
    dispatchAbort(this.signal);
  }};
  AbortSignalNative.timeout = function(delay) {{
    const numeric = Number(delay);
    if (!Number.isFinite(numeric) || numeric < 0 || numeric > 2147483647)
      throw new RangeError("native AbortSignal timeout is outside the bounded range");
    const signal = new AbortSignalNative();
    scheduleTimer(() => {{
      if (signal.aborted) return;
      signal.aborted = true;
      signal.reason = nativeTimeoutError();
      dispatchAbort(signal);
    }}, Math.trunc(numeric), [], false);
    return signal;
  }};
  AbortSignalNative.abort = function(reason) {{
    const signal = new AbortSignalNative();
    signal.aborted = true;
    signal.reason = reason === undefined ? nativeAbortError() : reason;
    return signal;
  }};
  AbortSignalNative.any = function(signals) {{
    if (!signals || typeof signals[Symbol.iterator] !== "function")
      throw new TypeError("native AbortSignal.any requires an iterable");
    const iterator = signals[Symbol.iterator]();
    if (!iterator || typeof iterator.next !== "function")
      throw new TypeError("native AbortSignal.any iterable is invalid");
    const inputs = [];
    while (true) {{
      const step = iterator.next();
      if (!step || typeof step !== "object") throw new TypeError("native AbortSignal.any iterator is invalid");
      if (step.done) break;
      if (inputs.length >= {max_commands}) throw new RangeError("native AbortSignal.any signal limit exceeded");
      const signal = step.value;
      if (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function")
        throw new TypeError("native AbortSignal.any input is invalid");
      inputs.push(signal);
    }}
    const combined = new AbortSignalNative();
    const alreadyAborted = inputs.find(signal => signal.aborted);
    if (alreadyAborted) {{
      combined.aborted = true;
      combined.reason = alreadyAborted.reason === undefined ? nativeAbortError() : alreadyAborted.reason;
      return combined;
    }}
    const listeners = [];
    const abortFrom = (source) => {{
      if (combined.aborted) return;
      combined.aborted = true;
      combined.reason = source.reason === undefined ? nativeAbortError() : source.reason;
      dispatchAbort(combined);
      for (const [signal, listener] of listeners) signal.removeEventListener("abort", listener);
    }};
    for (const signal of inputs) {{
      const listener = () => abortFrom(signal);
      listeners.push([signal, listener]);
      signal.addEventListener("abort", listener);
    }}
    return combined;
  }};
  globalThis.AbortSignal = AbortSignalNative;
  globalThis.AbortController = AbortControllerNative;
  const clearFetchAbortListener = (pending) => {{
    if (pending.signal && pending.abortListener) pending.signal.removeEventListener("abort", pending.abortListener);
  }};
  const fetchNativeDispatch = (input, options, streamedBodyBytes, streamedBodyStream, internalDestination) => {{
    const sourceRequest = input && input.__glassRequest === true ? input : null;
    const sourceUrl = input && input.__glassUrl === true ? input : null;
    if (typeof input !== "string" && !sourceRequest && !sourceUrl) throw new TypeError("native fetch requires a URL string, URL, or Request");
    const href = nativeRequestUrl(sourceRequest ? sourceRequest.url : sourceUrl || input);
    const optionsObject = options && typeof options === "object" ? options : {{}};
    const requestInitIsNonEmpty = Object.keys(optionsObject).length > 0;
    const destination = internalDestination === undefined || internalDestination === null
      ? null
      : String(internalDestination);
    if (destination !== null && destination !== "font")
      throw new TypeError("native fetch destination is unsupported");
    const hasBodyOverride = options && typeof options === "object" && Object.prototype.hasOwnProperty.call(options, "body");
    const sourceBodyPayload = (() => {{
      const payload = sourceRequest && !hasBodyOverride && sourceRequest.body !== null
        ? sourceRequest.__glassRequestBodyPayload
        : null;
      if (payload && payload.stream && streamedBodyBytes !== undefined)
        return Object.assign({{}}, payload, {{ bytes: streamedBodyBytes, stream: null }});
      return payload;
    }})();
    const settings = Object.assign(
      {{}},
      sourceRequest ? sourceRequest._settings : {{}},
      optionsObject,
    );
    const signal = settings.signal === undefined ? null : settings.signal;
    if (signal !== null && (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function")) throw new TypeError("native fetch signal is invalid");
    if (signal && signal.aborted) return Promise.reject(signal.reason === undefined ? nativeAbortError() : signal.reason);
    const mode = settings.mode === undefined ? "cors" : String(settings.mode).toLowerCase();
    if (!["cors", "no-cors", "same-origin"].includes(mode))
      return Promise.reject(new TypeError("native fetch mode is unsupported"));
    const redirect = settings.redirect === undefined ? "follow" : String(settings.redirect).toLowerCase();
    if (!["follow", "error", "manual"].includes(redirect))
      return Promise.reject(new TypeError("native fetch redirect mode is unsupported"));
    const cache = settings.cache === undefined ? "default" : String(settings.cache).toLowerCase();
    if (!["default", "no-store", "reload", "no-cache", "force-cache", "only-if-cached"].includes(cache))
      return Promise.reject(new TypeError("native fetch cache mode is unsupported"));
    if (cache === "only-if-cached" && mode !== "same-origin")
      return Promise.reject(new TypeError("native only-if-cached fetches require same-origin mode"));
    const credentialsMode = settings.credentials === undefined ? "same-origin" : String(settings.credentials);
    if (!["omit", "same-origin", "include"].includes(credentialsMode))
      return Promise.reject(new TypeError("native fetch credentials mode is unsupported"));
    let referrer;
    let referrerPolicy;
    try {{
      const hasReferrerOverride = Object.prototype.hasOwnProperty.call(optionsObject, "referrer")
        && optionsObject.referrer !== undefined;
      referrer = nativeRequestReferrer(
        hasReferrerOverride ? optionsObject.referrer : undefined,
        sourceRequest && !requestInitIsNonEmpty
          ? nativeRequestReferrerState.get(sourceRequest)
          : null,
      );
      referrerPolicy = nativeRequestReferrerPolicy(
        sourceRequest && requestInitIsNonEmpty
          && !Object.prototype.hasOwnProperty.call(optionsObject, "referrerPolicy")
          ? ""
          : settings.referrerPolicy,
      );
    }} catch (error) {{ return Promise.reject(error); }}
    let method;
    try {{ method = nativeFetchMethod(settings.method === undefined ? "GET" : settings.method); }}
    catch (error) {{ return Promise.reject(error); }}
    const hasStreamedBody = streamedBodyStream !== undefined;
    const blobBody = sourceBodyPayload || hasStreamedBody
      ? null
      : settings.body && settings.body.__glassNativeBlob === true
      ? settings.body
      : null;
    const binaryBody = sourceBodyPayload || hasStreamedBody
      ? null
      : streamedBodyBytes !== undefined
      ? streamedBodyBytes
      : settings.body instanceof ArrayBuffer
      ? Array.from(new Uint8Array(settings.body))
      : typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(settings.body)
        ? Array.from(new Uint8Array(settings.body.buffer, settings.body.byteOffset, settings.body.byteLength))
        : null;
    const rawBody = sourceBodyPayload || streamedBodyBytes !== undefined || hasStreamedBody || settings.body === undefined || settings.body === null || blobBody || binaryBody !== null
      ? null
      : String(settings.body);
    const formData = sourceBodyPayload
      ? null
      : settings.body && settings.body.__glassFormData === true
      ? settings.body
      : null;
    const urlSearchParams = sourceBodyPayload
      ? null
      : settings.body && settings.body.__glassUrlSearchParams === true
      ? settings.body
      : null;
    let body = hasStreamedBody
      ? null
      : sourceBodyPayload
      ? utf8TextFromBytes(sourceBodyPayload.bytes)
      : blobBody
      ? blobBody._text
      : binaryBody !== null
        ? utf8TextFromBytes(binaryBody)
        : rawBody;
    let bodyBase64 = hasStreamedBody
      ? null
      : sourceBodyPayload
      ? encodeBase64(sourceBodyPayload.bytes, nativeFormBodyLimit)
      : streamedBodyBytes !== undefined
      ? encodeBase64(streamedBodyBytes, nativeFormBodyLimit)
      : blobBody && Array.isArray(blobBody._bytes)
      ? encodeBase64(blobBody._bytes, nativeFormBodyLimit)
      : binaryBody !== null
        ? encodeBase64(binaryBody, nativeFormBodyLimit)
        : null;
    const usesSourceBody = sourceBodyPayload !== null;
    if (usesSourceBody && streamedBodyBytes === undefined && !hasStreamedBody) {{
      if (!nativeRequestBodyUse(sourceRequest)) return Promise.reject(new TypeError("native Request body is unusable"));
    }}
    const requestHeaderName = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
    const forbiddenRequestHeader = (name) => [
      "accept-charset", "accept-encoding", "access-control-request-headers",
      "access-control-request-method", "connection", "content-length",
      "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
      "origin", "referer", "te", "trailer", "transfer-encoding", "upgrade",
      "user-agent", "via",
    ].includes(name) || name.startsWith("proxy-") || name.startsWith("sec-");
    const normalizeRequestHeaders = (input) => {{
      if (input === undefined || input === null) return {{}};
      if (typeof input !== "object") throw new TypeError("native fetch headers must be an object");
      if (input.__glassHeaders === true) {{
        const record = {{}};
        input._entries.forEach(entry => {{ record[entry[0]] = entry[1]; }});
        input = record;
      }}
      const normalized = {{}};
      let count = 0;
      let totalBytes = 0;
      for (const name of Object.keys(input)) {{
        const normalizedName = String(name).toLowerCase();
        if (!requestHeaderName.test(String(name)) || normalizedName.length > {fetch_header_name_limit}) throw new TypeError("native fetch header name is invalid");
        if (forbiddenRequestHeader(normalizedName)) throw new TypeError("native fetch header is forbidden");
        const value = String(input[name]);
        if (value.length > {fetch_header_value_limit} || /[\u0000-\u001f\u007f]/.test(value)) throw new TypeError("native fetch header value is invalid");
        if (Object.prototype.hasOwnProperty.call(normalized, normalizedName)) normalized[normalizedName] += ", " + value;
        else {{
          count += 1;
          if (count > {fetch_header_count_limit}) throw new RangeError("native fetch header limit exceeded");
          normalized[normalizedName] = value;
        }}
        totalBytes += normalizedName.length + value.length;
        if (totalBytes > {fetch_header_bytes_limit}) throw new RangeError("native fetch headers exceed their limit");
      }}
      return normalized;
    }};
    const requestHeaders = normalizeRequestHeaders(settings.headers);
    let contentType = null;
    if (Object.prototype.hasOwnProperty.call(requestHeaders, "content-type")) {{
      contentType = requestHeaders["content-type"];
      delete requestHeaders["content-type"];
    }}
    if (contentType === null && sourceBodyPayload && sourceBodyPayload.contentType !== null)
      contentType = sourceBodyPayload.contentType;
    const requestId = nextFetchRequestId;
    if (formData) {{
      if (contentType !== null) return Promise.reject(new TypeError("FormData chooses its own Content-Type boundary"));
      const serialized = serializeFormData(formData, requestId);
      body = serialized.body;
      bodyBase64 = serialized.bodyBase64;
      contentType = serialized.contentType;
    }}
    if (urlSearchParams) {{
      if (contentType !== null) return Promise.reject(new TypeError("URLSearchParams chooses its own Content-Type"));
      body = urlSearchParams.toString();
      bodyBase64 = null;
      contentType = "application/x-www-form-urlencoded;charset=UTF-8";
    }}
    if (blobBody && contentType === null && blobBody.type) contentType = blobBody.type;
    if (nativeBodylessMethods.includes(method) && (body !== null || hasStreamedBody)) {{
      return Promise.reject(new TypeError(method + " fetch requests must not have a body"));
    }}
    const objectUrlEntry = nativeObjectUrlRegistry.get(href);
    if (objectUrlEntry) {{
      if (!nativeBodylessMethods.includes(method))
        return Promise.reject(new TypeError("native object URL fetches support only GET and HEAD"));
      const objectBytes = blobBytes(objectUrlEntry.blob);
      const responseBytes = method === "HEAD" ? [] : objectBytes;
      const objectContentType = objectUrlEntry.blob.type || null;
      const objectHeaders = [["content-length", String(objectBytes.length)]];
      if (objectContentType) objectHeaders.unshift(["content-type", objectContentType]);
      return Promise.resolve(responseFromFetch({{
        url: href,
        status: 200,
        statusText: "OK",
        headers: objectHeaders,
        contentType: objectContentType,
        body: utf8TextFromBytes(responseBytes),
        bodyBase64: encodeBase64(responseBytes),
        bodyNull: method === "HEAD",
        redirected: false,
        opaque: false,
        opaqueRedirect: false,
      }}));
    }}
    if (href.startsWith("blob:"))
      return Promise.reject(new TypeError("native object URL is revoked or unavailable"));
    nextFetchRequestId += 1;
    globalThis.__glassNextFetchRequestId = nextFetchRequestId;
    const credentials = credentialsMode !== "omit";
    const timeoutSetting = settings.__glassTimeoutMs === undefined
      ? null
      : Number(settings.__glassTimeoutMs);
    const timeoutMs = timeoutSetting === 0 ? null : timeoutSetting;
    if (timeoutMs !== null && (!Number.isSafeInteger(timeoutMs) || timeoutMs < 0 || timeoutMs > {max_native_xhr_timeout_ms}))
      throw new RangeError("native fetch timeout is outside the bounded XHR range");
    if (hasStreamedBody) {{
      try {{ startFetchUpload(streamedBodyStream, requestId); }}
      catch (error) {{ return Promise.reject(error); }}
    }}
    return new Promise((resolve, reject) => {{
      const pending = {{ resolve, reject, signal, abortListener: null, uploadStreamId: hasStreamedBody ? requestId : null }};
      const abort = () => {{
        if (fetchRequests.get(requestId) !== pending) return;
        fetchRequests.delete(requestId);
        clearFetchAbortListener(pending);
        if (pending.uploadStreamId !== null)
          cancelFetchUploadGroup(fetchUploadGroups.get(pending.uploadStreamId), "native fetch was aborted");
        reject(signal.reason === undefined ? nativeAbortError() : signal.reason);
      }};
      pending.abortListener = abort;
      fetchRequests.set(requestId, pending);
      if (signal) signal.addEventListener("abort", abort);
      if (!fetchRequests.has(requestId)) return;
      pushCommand({{ kind: "fetch", request_id: requestId, href, credentials, credentials_mode: credentialsMode, referrer: referrer.referrer, referrer_url: referrer.url, referrer_policy: referrerPolicy || documentReferrerPolicy.value, method, headers: requestHeaders, body, body_base64: bodyBase64, content_type: contentType, mode, redirect, cache, timeout_ms: timeoutMs, upload_stream_id: hasStreamedBody ? requestId : null, destination }});
    }});
  }};
