  const workerCryptoPool = globalThis.__glassWorkerCryptoPool instanceof Array
    ? globalThis.__glassWorkerCryptoPool
    : [];
  const workerCryptoRandomSource = typeof globalThis.__glassNativeRandomBytes === "function"
    ? globalThis.__glassNativeRandomBytes
    : null;
  let workerCryptoOffset = Number.isSafeInteger(globalThis.__glassWorkerCryptoOffset)
    ? globalThis.__glassWorkerCryptoOffset
    : 0;
  if (workerCryptoOffset > 0) {
    workerCryptoPool.splice(0, workerCryptoOffset);
    workerCryptoOffset = 0;
  }
  workerCryptoPool.push(...initialWorkerCryptoBytes);
  if (workerCryptoPool.length > __GLASS_WORKER_WORKER_CRYPTO_POOL_LIMIT__)
    workerCryptoPool.splice(0, workerCryptoPool.length - __GLASS_WORKER_WORKER_CRYPTO_POOL_LIMIT__);
  const workerCryptoTake = (count) => {
    if (!Number.isSafeInteger(count) || count < 0 || count > __GLASS_WORKER_WORKER_CRYPTO_VALUES_LIMIT__)
      throw new RangeError("native Worker crypto value is outside the bounded range");
    if (workerCryptoOffset + count > workerCryptoPool.length) {
      if (workerCryptoOffset > 0) {
        workerCryptoPool.splice(0, workerCryptoOffset);
        workerCryptoOffset = 0;
      }
      const needed = count - workerCryptoPool.length;
      if (needed > 0 && workerCryptoRandomSource) {
        const refill = workerCryptoRandomSource(needed);
        if (!Array.isArray(refill) || refill.length !== needed)
          throw new WorkerDOMExceptionNative("native Worker crypto refill failed", "OperationError");
        for (const byte of refill) workerCryptoPool.push(byte);
      }
    }
    if (workerCryptoOffset + count > workerCryptoPool.length)
      throw new WorkerDOMExceptionNative("native Worker crypto pool exhausted", "QuotaExceededError");
    const bytes = workerCryptoPool.slice(workerCryptoOffset, workerCryptoOffset + count);
    workerCryptoOffset += count;
    globalThis.__glassWorkerCryptoOffset = workerCryptoOffset;
    return bytes;
  };
  const workerCryptoIntegerArray = (value) => {
    const tag = Object.prototype.toString.call(value);
    return ["[object Int8Array]", "[object Uint8Array]", "[object Uint8ClampedArray]",
      "[object Int16Array]", "[object Uint16Array]", "[object Int32Array]",
      "[object Uint32Array]", "[object BigInt64Array]", "[object BigUint64Array]"].includes(tag);
  };
  const workerCrypto = globalThis.__glassWorkerCryptoObject instanceof Object
    ? globalThis.__glassWorkerCryptoObject
    : {};
  workerCrypto.getRandomValues = (value) => {
    if (!workerCryptoIntegerArray(value))
      throw new TypeError("native Worker crypto requires an integer typed array");
    if (value.byteLength > __GLASS_WORKER_WORKER_CRYPTO_VALUES_LIMIT__)
      throw new WorkerDOMExceptionNative("native Worker crypto value is too large", "QuotaExceededError");
    new Uint8Array(value.buffer, value.byteOffset, value.byteLength).set(workerCryptoTake(value.byteLength));
    return value;
  };
  workerCrypto.randomUUID = () => {
    const bytes = workerCryptoTake(16);
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    const hex = bytes.map(byte => byte.toString(16).padStart(2, "0")).join("");
    return hex.slice(0, 8) + "-" + hex.slice(8, 12) + "-" + hex.slice(12, 16)
      + "-" + hex.slice(16, 20) + "-" + hex.slice(20);
  };
  globalThis.__glassWorkerCryptoPool = workerCryptoPool;
  globalThis.__glassWorkerCryptoObject = workerCrypto;
  const workerCryptoDigestSource = typeof globalThis.__glassNativeCryptoDigest === "function"
    ? globalThis.__glassNativeCryptoDigest
    : null;
  const workerCryptoBufferInput = (input) => {
    if (input instanceof ArrayBuffer) return Array.from(new Uint8Array(input));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(input))
      return Array.from(new Uint8Array(input.buffer, input.byteOffset, input.byteLength));
    throw new TypeError("native Worker crypto digest data must be an ArrayBuffer or view");
  };
  const workerSubtle = globalThis.__glassWorkerSubtleCrypto instanceof Object
    ? globalThis.__glassWorkerSubtleCrypto
    : {};
  let workerCryptoKeyStore = workerSubtle.__glassNativeKeyStore;
  if (!(workerCryptoKeyStore instanceof WeakMap)) {
    workerCryptoKeyStore = new WeakMap();
    Object.defineProperty(workerSubtle, "__glassNativeKeyStore", {
      configurable: false,
      enumerable: false,
      value: workerCryptoKeyStore,
    });
  }
  const WorkerCryptoKeyNative = typeof globalThis.__glassWorkerCryptoKeyConstructor === "function"
    ? globalThis.__glassWorkerCryptoKeyConstructor
    : function CryptoKey() { throw new TypeError("Illegal constructor"); };
  WorkerCryptoKeyNative.prototype.constructor = WorkerCryptoKeyNative;
  globalThis.__glassWorkerCryptoKeyConstructor = WorkerCryptoKeyNative;
  globalThis.CryptoKey = WorkerCryptoKeyNative;
  const workerCryptoHashName = (value) => {
    const candidate = value && typeof value === "object" ? value.name : value;
    const name = String(candidate || "").toUpperCase();
    if (!["SHA-1", "SHA-256", "SHA-384", "SHA-512"].includes(name))
      throw new WorkerDOMExceptionNative("native Worker crypto hash is unsupported", "NotSupportedError");
    return name;
  };
  const workerCryptoHmacHash = (algorithm) => {
    const name = algorithm && typeof algorithm === "object" ? algorithm.name : algorithm;
    if (String(name || "").toUpperCase() !== "HMAC")
      throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
    if (!algorithm || typeof algorithm !== "object")
      throw new WorkerDOMExceptionNative("native Worker HMAC hash is required", "DataError");
    return workerCryptoHashName(algorithm.hash);
  };
  const workerCryptoUsages = (values) => {
    if (!Array.isArray(values)) throw new TypeError("native Worker crypto usages must be an array");
    const usages = [];
    for (const value of values) {
      const usage = String(value);
      if (!["sign", "verify"].includes(usage) || usages.includes(usage))
        throw new SyntaxError("native Worker crypto usage is invalid or duplicated");
      usages.push(usage);
    }
    return usages;
  };
  const workerCryptoKeyState = (key) => {
    const state = workerCryptoKeyStore.get(key);
    if (!state)
      throw new WorkerDOMExceptionNative("native Worker CryptoKey is not recognized", "InvalidAccessError");
    return state;
  };
  const workerCryptoMakeKey = (bytes, hashName, extractable, usages) => {
    const key = Object.create(WorkerCryptoKeyNative.prototype);
    const algorithm = Object.freeze({
      name: "HMAC",
      hash: Object.freeze({ name: hashName }),
      length: bytes.length * 8,
    });
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {
      type: { configurable: false, enumerable: true, value: "secret" },
      extractable: { configurable: false, enumerable: true, value: Boolean(extractable) },
      algorithm: { configurable: false, enumerable: true, value: algorithm },
      usages: { configurable: false, enumerable: true, value: normalizedUsages },
    });
    workerCryptoKeyStore.set(key, {
      kind: "HMAC",
      bytes: bytes.slice(),
      extractable: Boolean(extractable),
      hashName,
      usages: normalizedUsages,
    });
    return Object.freeze(key);
  };
  const workerCryptoOperationHash = (algorithm, state) => {
    const name = algorithm && typeof algorithm === "object" ? algorithm.name : algorithm;
    if (String(name || "").toUpperCase() !== "HMAC")
      throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
    if (algorithm && typeof algorithm === "object" && algorithm.hash !== undefined
        && workerCryptoHashName(algorithm.hash) !== state.hashName)
      throw new WorkerDOMExceptionNative("native Worker CryptoKey hash does not match", "InvalidAccessError");
    return state.hashName;
  };
  const workerCryptoHmacSource = typeof globalThis.__glassNativeCryptoHmac === "function"
    ? globalThis.__glassNativeCryptoHmac
    : null;
  const workerCryptoHmacBytes = (state, data) => {
    if (!workerCryptoHmacSource)
      throw new WorkerDOMExceptionNative("native Worker crypto HMAC is unavailable", "OperationError");
    const encoded = workerCryptoHmacSource(
      state.hashName,
      encodeWorkerBase64(state.bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(data, __GLASS_WORKER_FETCH_BODY_LIMIT__),
    );
    return decodeWorkerBase64(encoded, __GLASS_WORKER_FETCH_BODY_LIMIT__);
  };
  const workerCryptoEd25519Source = typeof globalThis.__glassNativeCryptoEd25519 === "function"
    ? globalThis.__glassNativeCryptoEd25519
    : null;
  const workerCryptoEd25519Algorithm = (algorithm) => {
    const name = algorithm && typeof algorithm === "object" ? algorithm.name : algorithm;
    if (String(name || "").toUpperCase() !== "ED25519")
      throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
  };
  const workerCryptoEd25519Usages = (values) => {
    if (!Array.isArray(values)) throw new TypeError("native Worker Ed25519 usages must be an array");
    const usages = [];
    for (const value of values) {
      const usage = String(value);
      if (!["sign", "verify"].includes(usage) || usages.includes(usage))
        throw new SyntaxError("native Worker Ed25519 usage is invalid or duplicated");
      usages.push(usage);
    }
    return usages;
  };
  const workerCryptoEd25519PublicKey = (privateBytes) => {
    if (!workerCryptoEd25519Source)
      throw new WorkerDOMExceptionNative("native Worker Ed25519 is unavailable", "OperationError");
    const encoded = workerCryptoEd25519Source(
      "public",
      encodeWorkerBase64(privateBytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      "",
      "",
    );
    const bytes = decodeWorkerBase64(encoded, __GLASS_WORKER_FETCH_BODY_LIMIT__);
    if (bytes.length !== 32)
      throw new WorkerDOMExceptionNative("native Worker Ed25519 public key is invalid", "OperationError");
    return bytes;
  };
  const workerCryptoEd25519Bytes = (operation, state, data, signature = []) => {
    if (!workerCryptoEd25519Source)
      throw new WorkerDOMExceptionNative("native Worker Ed25519 is unavailable", "OperationError");
    const encoded = workerCryptoEd25519Source(
      operation,
      encodeWorkerBase64(state.bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(data, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(signature, __GLASS_WORKER_FETCH_BODY_LIMIT__),
    );
    if (operation === "verify") return encoded === "1";
    return decodeWorkerBase64(encoded, __GLASS_WORKER_FETCH_BODY_LIMIT__);
  };
  const workerCryptoMakeEd25519PublicKey = (bytes, extractable, usages) => {
    if (!workerCryptoEd25519Source)
      throw new WorkerDOMExceptionNative("native Worker Ed25519 is unavailable", "OperationError");
    try {
      if (workerCryptoEd25519Source(
        "validate",
        encodeWorkerBase64(bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
        "",
        "",
      ) !== "1") throw new Error("invalid public key");
    } catch (_error) {
      throw new WorkerDOMExceptionNative("native Worker Ed25519 public key is invalid", "DataError");
    }
    const key = Object.create(WorkerCryptoKeyNative.prototype);
    const algorithm = Object.freeze({ name: "Ed25519" });
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {
      type: { configurable: false, enumerable: true, value: "public" },
      extractable: { configurable: false, enumerable: true, value: Boolean(extractable) },
      algorithm: { configurable: false, enumerable: true, value: algorithm },
      usages: { configurable: false, enumerable: true, value: normalizedUsages },
    });
    workerCryptoKeyStore.set(key, {
      kind: "ED25519-PUBLIC",
      bytes: bytes.slice(),
      extractable: Boolean(extractable),
      usages: normalizedUsages,
    });
    return Object.freeze(key);
  };
  const workerCryptoMakeEd25519PrivateKey = (privateBytes, publicBytes, extractable, usages) => {
    const key = Object.create(WorkerCryptoKeyNative.prototype);
    const algorithm = Object.freeze({ name: "Ed25519" });
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {
      type: { configurable: false, enumerable: true, value: "private" },
      extractable: { configurable: false, enumerable: true, value: Boolean(extractable) },
      algorithm: { configurable: false, enumerable: true, value: algorithm },
      usages: { configurable: false, enumerable: true, value: normalizedUsages },
    });
    workerCryptoKeyStore.set(key, {
      kind: "ED25519-PRIVATE",
      bytes: privateBytes.slice(),
      publicBytes: publicBytes.slice(),
      extractable: Boolean(extractable),
      usages: normalizedUsages,
    });
    return Object.freeze(key);
  };
  const workerCryptoDeriveSource = typeof globalThis.__glassNativeCryptoDerive === "function"
    ? globalThis.__glassNativeCryptoDerive
    : null;
  const workerCryptoAesSource = typeof globalThis.__glassNativeCryptoAesGcm === "function"
    ? globalThis.__glassNativeCryptoAesGcm
    : null;
  const workerCryptoAesCbcSource = typeof globalThis.__glassNativeCryptoAesCbc === "function"
    ? globalThis.__glassNativeCryptoAesCbc
    : null;
  const workerCryptoAesCtrSource = typeof globalThis.__glassNativeCryptoAesCtr === "function"
    ? globalThis.__glassNativeCryptoAesCtr
    : null;
  const workerCryptoAesUsages = (values) => {
    if (!Array.isArray(values)) throw new TypeError("native Worker crypto usages must be an array");
    const usages = [];
    for (const value of values) {
      const usage = String(value);
      if (!["encrypt", "decrypt"].includes(usage) || usages.includes(usage))
        throw new SyntaxError("native Worker AES usage is invalid or duplicated");
      usages.push(usage);
    }
    return usages;
  };
  const workerCryptoAesAlgorithm = (algorithm) => {
    if (!algorithm || typeof algorithm !== "object")
      throw new WorkerDOMExceptionNative("native Worker AES-GCM algorithm is required", "TypeError");
    if (String(algorithm.name || "").toUpperCase() !== "AES-GCM")
      throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
    const iv = workerCryptoBufferInput(algorithm.iv);
    if (iv.length !== 12)
      throw new WorkerDOMExceptionNative("native Worker AES-GCM IV must be 12 bytes", "DataError");
    const additionalData = algorithm.additionalData === undefined
      ? []
      : workerCryptoBufferInput(algorithm.additionalData);
    if (additionalData.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
      throw new WorkerDOMExceptionNative("native Worker AES-GCM additional data is too large", "DataError");
    const tagLength = algorithm.tagLength === undefined ? 128 : Number(algorithm.tagLength);
    if (tagLength !== 128)
      throw new WorkerDOMExceptionNative("native Worker AES-GCM tag length is unsupported", "NotSupportedError");
    return { iv, additionalData, tagLength };
  };
  const workerCryptoAesCbcAlgorithm = (algorithm) => {
    if (!algorithm || typeof algorithm !== "object")
      throw new WorkerDOMExceptionNative("native Worker AES-CBC algorithm is required", "TypeError");
    if (String(algorithm.name || "").toUpperCase() !== "AES-CBC")
      throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
    const iv = workerCryptoBufferInput(algorithm.iv);
    if (iv.length !== 16)
      throw new WorkerDOMExceptionNative("native Worker AES-CBC IV must be 16 bytes", "DataError");
    return { iv };
  };
  const workerCryptoAesCtrAlgorithm = (algorithm) => {
    if (!algorithm || typeof algorithm !== "object")
      throw new WorkerDOMExceptionNative("native Worker AES-CTR algorithm is required", "TypeError");
    if (String(algorithm.name || "").toUpperCase() !== "AES-CTR")
      throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
    const counter = workerCryptoBufferInput(algorithm.counter);
    if (counter.length !== 16)
      throw new WorkerDOMExceptionNative("native Worker AES-CTR counter must be 16 bytes", "DataError");
    const length = Number(algorithm.length);
    if (!Number.isSafeInteger(length) || length < 1 || length > 128)
      throw new WorkerDOMExceptionNative("native Worker AES-CTR counter length is invalid", "DataError");
    return { counter, length };
  };
  const workerCryptoMakeAesKey = (bytes, extractable, usages, algorithmName = "AES-GCM") => {
    const key = Object.create(WorkerCryptoKeyNative.prototype);
    const algorithm = Object.freeze({ name: algorithmName, length: bytes.length * 8 });
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {
      type: { configurable: false, enumerable: true, value: "secret" },
      extractable: { configurable: false, enumerable: true, value: Boolean(extractable) },
      algorithm: { configurable: false, enumerable: true, value: algorithm },
      usages: { configurable: false, enumerable: true, value: normalizedUsages },
    });
    workerCryptoKeyStore.set(key, {
      kind: algorithmName,
      bytes: bytes.slice(),
      extractable: Boolean(extractable),
      length: bytes.length * 8,
      usages: normalizedUsages,
    });
    return Object.freeze(key);
  };
  const workerCryptoDeriveUsages = (values) => {
    if (!Array.isArray(values)) throw new TypeError("native Worker crypto usages must be an array");
    const usages = [];
    for (const value of values) {
      const usage = String(value);
      if (!["deriveBits", "deriveKey"].includes(usage) || usages.includes(usage))
        throw new SyntaxError("native Worker derivation usage is invalid or duplicated");
      usages.push(usage);
    }
    return usages;
  };
  const workerCryptoMakeDeriveKey = (kind, bytes, usages) => {
    const key = Object.create(WorkerCryptoKeyNative.prototype);
    const algorithm = Object.freeze({ name: kind });
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {
      type: { configurable: false, enumerable: true, value: "secret" },
      extractable: { configurable: false, enumerable: true, value: false },
      algorithm: { configurable: false, enumerable: true, value: algorithm },
      usages: { configurable: false, enumerable: true, value: normalizedUsages },
    });
    workerCryptoKeyStore.set(key, {
      kind,
      bytes: bytes.slice(),
      extractable: false,
      usages: normalizedUsages,
    });
    return Object.freeze(key);
  };
  const workerCryptoHmacDefaultLength = (hashName) =>
    ["SHA-384", "SHA-512"].includes(hashName) ? 1024 : 512;
  const workerCryptoGenerateKey = (algorithm, extractable, keyUsages) => {
    if (!algorithm || typeof algorithm !== "object")
      throw new WorkerDOMExceptionNative("native crypto key algorithm is required", "TypeError");
    const algorithmName = String(algorithm.name || "").toUpperCase();
    if (algorithmName === "ED25519") {
      const usages = workerCryptoEd25519Usages(keyUsages);
      const privateBytes = workerCryptoTake(32);
      const publicBytes = workerCryptoEd25519PublicKey(privateBytes);
      return Object.freeze({
        privateKey: workerCryptoMakeEd25519PrivateKey(
          privateBytes,
          publicBytes,
          extractable,
          usages.filter(usage => usage === "sign"),
        ),
        publicKey: workerCryptoMakeEd25519PublicKey(
          publicBytes,
          true,
          usages.filter(usage => usage === "verify"),
        ),
      });
    }
    if (algorithmName === "HMAC") {
      const hashName = workerCryptoHashName(algorithm.hash);
      const length = algorithm.length === undefined
        ? workerCryptoHmacDefaultLength(hashName)
        : Number(algorithm.length);
      if (!Number.isSafeInteger(length) || length <= 0 || length % 8 !== 0
          || length / 8 > __GLASS_WORKER_WORKER_CRYPTO_VALUES_LIMIT__)
        throw new WorkerDOMExceptionNative("native HMAC key length is invalid", "DataError");
      return workerCryptoMakeKey(
        workerCryptoTake(length / 8),
        hashName,
        extractable,
        workerCryptoUsages(keyUsages),
      );
    }
    if (["AES-GCM", "AES-CBC", "AES-CTR"].includes(algorithmName)) {
      const length = Number(algorithm.length);
      if (![128, 192, 256].includes(length))
        throw new WorkerDOMExceptionNative("native AES key length is invalid", "DataError");
      return workerCryptoMakeAesKey(
        workerCryptoTake(length / 8),
        extractable,
        workerCryptoAesUsages(keyUsages),
        algorithmName,
      );
    }
    throw new WorkerDOMExceptionNative("native crypto algorithm is unsupported", "NotSupportedError");
  };
  const workerCryptoDeriveAlgorithm = (algorithm, state) => {
    if (!algorithm || typeof algorithm !== "object")
      throw new WorkerDOMExceptionNative("native crypto derivation algorithm is required", "TypeError");
    const name = String(algorithm.name || "").toUpperCase();
    if (name !== state.kind)
      throw new WorkerDOMExceptionNative("native crypto derivation algorithm does not match the key", "InvalidAccessError");
    const hashName = workerCryptoHashName(algorithm.hash);
    const salt = workerCryptoBufferInput(algorithm.salt);
    if (salt.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
      throw new WorkerDOMExceptionNative("native crypto derivation salt is too large", "DataError");
    if (name === "HKDF") {
      const info = workerCryptoBufferInput(algorithm.info);
      if (info.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
        throw new WorkerDOMExceptionNative("native HKDF info is too large", "DataError");
      return { hashName, salt, info, iterations: 0 };
    }
    const iterations = Number(algorithm.iterations);
    if (!Number.isSafeInteger(iterations) || iterations < 1 || iterations > 100000)
      throw new WorkerDOMExceptionNative("native PBKDF2 iterations are invalid", "DataError");
    return { hashName, salt, info: [], iterations };
  };
  const workerCryptoDeriveHashLength = (hashName) =>
    ({ "SHA-1": 20, "SHA-256": 32, "SHA-384": 48, "SHA-512": 64 })[hashName] || 0;
  const workerCryptoCheckDeriveWork = (state, algorithm, length) => {
    const hashLength = workerCryptoDeriveHashLength(algorithm.hashName);
    const blockCount = Math.ceil(length / hashLength);
    if (state.kind === "HKDF" && length > hashLength * 255)
      throw new WorkerDOMExceptionNative("native HKDF output is too large", "DataError");
    const workPerBlock = state.kind === "HKDF"
      ? algorithm.info.length + hashLength + 1
      : algorithm.salt.length + 4 + algorithm.iterations * hashLength;
    if (blockCount > 0 && workPerBlock > Math.floor(__GLASS_WORKER_NATIVE_CRYPTO_DERIVE_WORK__ / blockCount))
      throw new WorkerDOMExceptionNative("native crypto derivation work is too large", "DataError");
  };
  const workerCryptoDeriveBytes = (state, algorithm, length) => {
    if (!workerCryptoDeriveSource)
      throw new WorkerDOMExceptionNative("native crypto derivation is unavailable", "OperationError");
    if (length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
      throw new WorkerDOMExceptionNative("native crypto derivation output is too large", "DataError");
    workerCryptoCheckDeriveWork(state, algorithm, length);
    const encoded = workerCryptoDeriveSource(
      state.kind,
      algorithm.hashName,
      encodeWorkerBase64(state.bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(algorithm.salt, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(algorithm.info, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      algorithm.iterations,
      length * 8,
    );
    return decodeWorkerBase64(encoded, __GLASS_WORKER_FETCH_BODY_LIMIT__);
  };
  const workerCryptoDerivedKey = (state, algorithm, targetAlgorithm, extractable, keyUsages) => {
    if (!targetAlgorithm || typeof targetAlgorithm !== "object")
      throw new WorkerDOMExceptionNative("native derived key algorithm is required", "TypeError");
    const name = String(targetAlgorithm.name || "").toUpperCase();
    if (name === "HMAC") {
      const hashName = workerCryptoHashName(targetAlgorithm.hash);
      const length = targetAlgorithm.length === undefined
        ? workerCryptoHmacDefaultLength(hashName)
        : Number(targetAlgorithm.length);
      if (!Number.isSafeInteger(length) || length <= 0 || length % 8 !== 0
          || length / 8 > __GLASS_WORKER_WORKER_CRYPTO_VALUES_LIMIT__)
        throw new WorkerDOMExceptionNative("native derived HMAC key length is invalid", "DataError");
      return workerCryptoMakeKey(
        workerCryptoDeriveBytes(state, algorithm, length / 8),
        hashName,
        extractable,
        workerCryptoUsages(keyUsages),
      );
    }
    if (["AES-GCM", "AES-CBC", "AES-CTR"].includes(name)) {
      const length = Number(targetAlgorithm.length);
      if (![128, 192, 256].includes(length))
        throw new WorkerDOMExceptionNative("native derived AES key length is invalid", "DataError");
      return workerCryptoMakeAesKey(
        workerCryptoDeriveBytes(state, algorithm, length / 8),
        extractable,
        workerCryptoAesUsages(keyUsages),
        name,
      );
    }
    throw new WorkerDOMExceptionNative("native derived key algorithm is unsupported", "NotSupportedError");
  };
  const workerCryptoJwkDecode = (value) => {
    if (typeof value !== "string" || value.length > Math.ceil(__GLASS_WORKER_FETCH_BODY_LIMIT__ / 3) * 4
        || !/^[A-Za-z0-9_-]*$/.test(value) || value.length % 4 === 1)
      throw new WorkerDOMExceptionNative("native Worker JWK key material is invalid", "DataError");
    const remainder = value.length % 4;
    const standard = value.replace(/-/g, "+").replace(/_/g, "/")
      + (remainder === 0 ? "" : "=".repeat(4 - remainder));
    return decodeWorkerBase64(standard, __GLASS_WORKER_FETCH_BODY_LIMIT__);
  };
  const workerCryptoJwkEncode = (bytes) =>
    encodeWorkerBase64(bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__)
      .replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
  const workerCryptoJwkUsages = (jwk, usages) => {
    if (jwk.key_ops === undefined) return usages;
    if (!Array.isArray(jwk.key_ops))
      throw new WorkerDOMExceptionNative("native Worker JWK key_ops is invalid", "DataError");
    const operations = [];
    for (const value of jwk.key_ops) {
      const operation = String(value);
      if (!["sign", "verify", "encrypt", "decrypt"].includes(operation)
          || operations.includes(operation))
        throw new WorkerDOMExceptionNative("native Worker JWK key_ops is invalid", "DataError");
      operations.push(operation);
    }
    if (usages.some(usage => !operations.includes(usage)))
      throw new WorkerDOMExceptionNative("native Worker JWK key_ops does not permit usage", "DataError");
    return usages;
  };
  const workerCryptoImportJwk = (jwk, algorithm, extractable, keyUsages) => {
    if (!jwk || typeof jwk !== "object" || Array.isArray(jwk))
      throw new WorkerDOMExceptionNative("native Worker JWK is invalid", "DataError");
    if (jwk.ext !== undefined && typeof jwk.ext !== "boolean")
      throw new WorkerDOMExceptionNative("native Worker JWK ext is invalid", "DataError");
    if (jwk.ext === false && extractable)
      throw new WorkerDOMExceptionNative("native Worker JWK is not extractable", "DataError");
    const algorithmName = algorithm && typeof algorithm === "object"
      ? String(algorithm.name || "").toUpperCase()
      : "";
    if (algorithmName === "ED25519") {
      if (jwk.kty !== "OKP" || jwk.crv !== "Ed25519" || typeof jwk.x !== "string"
          || (jwk.alg !== undefined && jwk.alg !== "EdDSA"))
        throw new WorkerDOMExceptionNative("native Worker Ed25519 JWK is invalid", "DataError");
      const publicBytes = workerCryptoJwkDecode(jwk.x);
      if (publicBytes.length !== 32)
        throw new WorkerDOMExceptionNative("native Worker Ed25519 JWK public key is invalid", "DataError");
      const usages = workerCryptoJwkUsages(jwk, workerCryptoEd25519Usages(keyUsages));
      if (jwk.d !== undefined) {
        if (typeof jwk.d !== "string" || usages.some(usage => usage !== "sign"))
          throw new WorkerDOMExceptionNative("native Worker Ed25519 private JWK usage is invalid", "DataError");
        const privateBytes = workerCryptoJwkDecode(jwk.d);
        if (privateBytes.length !== 32)
          throw new WorkerDOMExceptionNative("native Worker Ed25519 JWK private key is invalid", "DataError");
        const derivedPublicBytes = workerCryptoEd25519PublicKey(privateBytes);
        if (derivedPublicBytes.length !== publicBytes.length
            || derivedPublicBytes.some((value, index) => value !== publicBytes[index]))
          throw new WorkerDOMExceptionNative("native Worker Ed25519 JWK key pair does not match", "DataError");
        return workerCryptoMakeEd25519PrivateKey(
          privateBytes,
          publicBytes,
          extractable,
          usages,
        );
      }
      if (usages.some(usage => usage !== "verify"))
        throw new WorkerDOMExceptionNative("native Worker Ed25519 public JWK usage is invalid", "DataError");
      return workerCryptoMakeEd25519PublicKey(publicBytes, extractable, usages);
    }
    if (jwk.kty !== "oct" || typeof jwk.k !== "string")
      throw new WorkerDOMExceptionNative("native Worker JWK is invalid", "DataError");
    const bytes = workerCryptoJwkDecode(jwk.k);
    if (algorithmName === "HMAC") {
      if (bytes.length === 0)
        throw new WorkerDOMExceptionNative("native Worker HMAC JWK key is empty", "DataError");
      const hashName = workerCryptoHashName(algorithm.hash);
      const expectedAlgorithm = ({ "SHA-1": "HS1", "SHA-256": "HS256", "SHA-384": "HS384", "SHA-512": "HS512" })[hashName];
      if (jwk.alg !== undefined && jwk.alg !== expectedAlgorithm)
        throw new WorkerDOMExceptionNative("native Worker HMAC JWK algorithm does not match", "DataError");
      return workerCryptoMakeKey(
        bytes,
        hashName,
        extractable,
        workerCryptoJwkUsages(jwk, workerCryptoUsages(keyUsages)),
      );
    }
    if (algorithmName === "AES-GCM") {
      const expectedAlgorithm = ({ 16: "A128GCM", 24: "A192GCM", 32: "A256GCM" })[bytes.length];
      if (!expectedAlgorithm || (jwk.alg !== undefined && jwk.alg !== expectedAlgorithm))
        throw new WorkerDOMExceptionNative("native Worker AES-GCM JWK key length or algorithm is invalid", "DataError");
      return workerCryptoMakeAesKey(
        bytes,
        extractable,
        workerCryptoJwkUsages(jwk, workerCryptoAesUsages(keyUsages)),
      );
    }
    throw new WorkerDOMExceptionNative("native Worker JWK algorithm is unsupported", "NotSupportedError");
  };
  const workerCryptoExportJwk = (state) => {
    if (state.kind === "ED25519-PUBLIC") {
      return { kty: "OKP", crv: "Ed25519", x: workerCryptoJwkEncode(state.bytes), alg: "EdDSA", ext: Boolean(state.extractable), key_ops: Array.from(state.usages) };
    }
    if (state.kind === "ED25519-PRIVATE") {
      return { kty: "OKP", crv: "Ed25519", x: workerCryptoJwkEncode(state.publicBytes), d: workerCryptoJwkEncode(state.bytes), alg: "EdDSA", ext: Boolean(state.extractable), key_ops: Array.from(state.usages) };
    }
    if (state.kind === "HMAC") {
      const alg = ({ "SHA-1": "HS1", "SHA-256": "HS256", "SHA-384": "HS384", "SHA-512": "HS512" })[state.hashName];
      return { kty: "oct", k: workerCryptoJwkEncode(state.bytes), alg, ext: Boolean(state.extractable), key_ops: Array.from(state.usages) };
    }
    if (state.kind === "AES-GCM") {
      const alg = ({ 16: "A128GCM", 24: "A192GCM", 32: "A256GCM" })[state.bytes.length];
      return { kty: "oct", k: workerCryptoJwkEncode(state.bytes), alg, ext: Boolean(state.extractable), key_ops: Array.from(state.usages) };
    }
    throw new WorkerDOMExceptionNative("native Worker JWK key kind is unsupported", "NotSupportedError");
  };
  const workerCryptoAesBytes = (operation, state, algorithm, data) => {
    if (!workerCryptoAesSource)
      throw new WorkerDOMExceptionNative("native Worker AES-GCM is unavailable", "OperationError");
    const encoded = workerCryptoAesSource(
      operation,
      encodeWorkerBase64(state.bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(algorithm.iv, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(algorithm.additionalData, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      algorithm.tagLength,
      encodeWorkerBase64(data, __GLASS_WORKER_FETCH_BODY_LIMIT__),
    );
    return decodeWorkerBase64(encoded, __GLASS_WORKER_FETCH_BODY_LIMIT__);
  };
  const workerCryptoAesCbcBytes = (operation, state, algorithm, data) => {
    if (!workerCryptoAesCbcSource)
      throw new WorkerDOMExceptionNative("native Worker AES-CBC is unavailable", "OperationError");
    const encoded = workerCryptoAesCbcSource(
      operation,
      encodeWorkerBase64(state.bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(algorithm.iv, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(data, __GLASS_WORKER_FETCH_BODY_LIMIT__),
    );
    return decodeWorkerBase64(encoded, __GLASS_WORKER_FETCH_BODY_LIMIT__);
  };
  const workerCryptoAesCtrBytes = (state, algorithm, data) => {
    if (!workerCryptoAesCtrSource)
      throw new WorkerDOMExceptionNative("native Worker AES-CTR is unavailable", "OperationError");
    const encoded = workerCryptoAesCtrSource(
      encodeWorkerBase64(state.bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      encodeWorkerBase64(algorithm.counter, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      algorithm.length,
      encodeWorkerBase64(data, __GLASS_WORKER_FETCH_BODY_LIMIT__),
    );
    return decodeWorkerBase64(encoded, __GLASS_WORKER_FETCH_BODY_LIMIT__);
  };
  workerSubtle.digest = (algorithm, data) => {
    const name = algorithm && typeof algorithm === "object"
      ? String(algorithm.name || "")
      : String(algorithm || "");
    if (!["SHA-1", "SHA-256", "SHA-384", "SHA-512"].includes(name.toUpperCase()))
      return Promise.reject(new WorkerDOMExceptionNative("native Worker crypto digest algorithm is unsupported", "NotSupportedError"));
    let bytes;
    try { bytes = workerCryptoBufferInput(data); } catch (error) { return Promise.reject(error); }
    if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
      return Promise.reject(new WorkerDOMExceptionNative("native Worker crypto digest data is too large", "DataError"));
    if (!workerCryptoDigestSource)
      return Promise.reject(new WorkerDOMExceptionNative("native Worker crypto digest is unavailable", "OperationError"));
    try {
      const encoded = encodeWorkerBase64(bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__);
      const digest = workerCryptoDigestSource(name.toUpperCase(), encoded);
      return Promise.resolve(new Uint8Array(decodeWorkerBase64(digest, __GLASS_WORKER_FETCH_BODY_LIMIT__)).buffer);
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.generateKey = (algorithm, extractable = false, keyUsages = []) => {
    try {
      return Promise.resolve(workerCryptoGenerateKey(algorithm, extractable, keyUsages));
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.deriveBits = (algorithm, baseKey, length) => {
    try {
      const state = workerCryptoKeyState(baseKey);
      if (!["HKDF", "PBKDF2"].includes(state.kind))
        throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
      if (!state.usages.includes("deriveBits"))
        throw new WorkerDOMExceptionNative("native Worker CryptoKey cannot derive bits", "InvalidAccessError");
      if (!Number.isSafeInteger(length) || length < 0 || length % 8 !== 0
          || length / 8 > __GLASS_WORKER_FETCH_BODY_LIMIT__)
        throw new WorkerDOMExceptionNative("native Worker crypto derivation length is invalid", "DataError");
      const normalized = workerCryptoDeriveAlgorithm(algorithm, state);
      return Promise.resolve(new Uint8Array(
        workerCryptoDeriveBytes(state, normalized, length / 8)
      ).buffer);
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.deriveKey = (algorithm, baseKey, derivedKeyAlgorithm, extractable = false, keyUsages = []) => {
    try {
      const state = workerCryptoKeyState(baseKey);
      if (!["HKDF", "PBKDF2"].includes(state.kind))
        throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
      if (!state.usages.includes("deriveKey"))
        throw new WorkerDOMExceptionNative("native Worker CryptoKey cannot derive a key", "InvalidAccessError");
      const normalized = workerCryptoDeriveAlgorithm(algorithm, state);
      return Promise.resolve(workerCryptoDerivedKey(
        state, normalized, derivedKeyAlgorithm, extractable, keyUsages
      ));
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.importKey = (format, keyData, algorithm, extractable = false, keyUsages = []) => {
    try {
      const normalizedFormat = String(format).toLowerCase();
      if (normalizedFormat === "jwk")
        return Promise.resolve(workerCryptoImportJwk(keyData, algorithm, extractable, keyUsages));
      if (normalizedFormat !== "raw")
        throw new WorkerDOMExceptionNative("native Worker crypto key format is unsupported", "NotSupportedError");
      const bytes = workerCryptoBufferInput(keyData);
      if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
        throw new WorkerDOMExceptionNative("native Worker crypto key data is invalid", "DataError");
      const algorithmName = algorithm && typeof algorithm === "object"
        ? String(algorithm.name || "").toUpperCase()
        : "";
      if (["HKDF", "PBKDF2"].includes(algorithmName)) {
        if (extractable)
          throw new WorkerDOMExceptionNative("native Worker derivation keys are not extractable", "SyntaxError");
        return Promise.resolve(workerCryptoMakeDeriveKey(
          algorithmName, bytes, workerCryptoDeriveUsages(keyUsages)
        ));
      }
      if (algorithmName === "ED25519") {
        if (bytes.length !== 32)
          throw new WorkerDOMExceptionNative("native Worker Ed25519 public key length is invalid", "DataError");
        const usages = workerCryptoEd25519Usages(keyUsages);
        if (usages.some(usage => usage !== "verify"))
          throw new WorkerDOMExceptionNative("native Worker Ed25519 public key usage is invalid", "SyntaxError");
        return Promise.resolve(workerCryptoMakeEd25519PublicKey(bytes, extractable, usages));
      }
      if (bytes.length === 0)
        throw new WorkerDOMExceptionNative("native Worker crypto key data is invalid", "DataError");
      if (["AES-GCM", "AES-CBC", "AES-CTR"].includes(algorithmName)) {
        if (![16, 24, 32].includes(bytes.length))
          throw new WorkerDOMExceptionNative("native Worker AES key length is invalid", "DataError");
        return Promise.resolve(workerCryptoMakeAesKey(
          bytes, extractable, workerCryptoAesUsages(keyUsages), algorithmName
        ));
      }
      const hashName = workerCryptoHmacHash(algorithm);
      return Promise.resolve(workerCryptoMakeKey(bytes, hashName, extractable, workerCryptoUsages(keyUsages)));
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.exportKey = (format, key) => {
    try {
      const normalizedFormat = String(format).toLowerCase();
      const state = workerCryptoKeyState(key);
      if (normalizedFormat === "jwk") {
        if (!state.extractable)
          throw new WorkerDOMExceptionNative("native Worker CryptoKey is not extractable", "InvalidAccessError");
        return Promise.resolve(workerCryptoExportJwk(state));
      }
      if (normalizedFormat !== "raw")
        throw new WorkerDOMExceptionNative("native Worker crypto key format is unsupported", "NotSupportedError");
      if (!state.extractable)
        throw new WorkerDOMExceptionNative("native Worker CryptoKey is not extractable", "InvalidAccessError");
      if (state.kind !== "HMAC" && !["AES-GCM", "AES-CBC", "AES-CTR"].includes(state.kind)
          && state.kind !== "ED25519-PUBLIC")
        throw new WorkerDOMExceptionNative("native Worker crypto raw export is unsupported", "NotSupportedError");
      return Promise.resolve(new Uint8Array(state.bytes.slice()).buffer);
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.sign = (algorithm, key, data) => {
    try {
      const state = workerCryptoKeyState(key);
      if (state.kind === "ED25519-PRIVATE") {
        workerCryptoEd25519Algorithm(algorithm);
        if (!state.usages.includes("sign"))
          throw new WorkerDOMExceptionNative("native Worker Ed25519 private key cannot sign", "InvalidAccessError");
        const bytes = workerCryptoBufferInput(data);
        if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
          throw new WorkerDOMExceptionNative("native Worker Ed25519 data is too large", "DataError");
        return Promise.resolve(new Uint8Array(
          workerCryptoEd25519Bytes("sign", state, bytes)
        ).buffer);
      }
      if (state.kind !== "HMAC")
        throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
      workerCryptoOperationHash(algorithm, state);
      if (!state.usages.includes("sign"))
        throw new WorkerDOMExceptionNative("native Worker CryptoKey cannot sign", "InvalidAccessError");
      return Promise.resolve(new Uint8Array(workerCryptoHmacBytes(state, workerCryptoBufferInput(data))).buffer);
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.verify = (algorithm, key, signature, data) => {
    try {
      const state = workerCryptoKeyState(key);
      if (state.kind === "ED25519-PUBLIC") {
        workerCryptoEd25519Algorithm(algorithm);
        if (!state.usages.includes("verify"))
          throw new WorkerDOMExceptionNative("native Worker Ed25519 public key cannot verify", "InvalidAccessError");
        const expected = workerCryptoBufferInput(signature);
        const bytes = workerCryptoBufferInput(data);
        if (expected.length !== 64 || bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
          throw new WorkerDOMExceptionNative("native Worker Ed25519 signature or data is invalid", "DataError");
        return Promise.resolve(workerCryptoEd25519Bytes("verify", state, bytes, expected));
      }
      if (state.kind !== "HMAC")
        throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
      workerCryptoOperationHash(algorithm, state);
      if (!state.usages.includes("verify"))
        throw new WorkerDOMExceptionNative("native Worker CryptoKey cannot verify", "InvalidAccessError");
      const expected = workerCryptoBufferInput(signature);
      const actual = workerCryptoHmacBytes(state, workerCryptoBufferInput(data));
      let difference = actual.length === expected.length ? 0 : 1;
      const length = Math.max(actual.length, expected.length);
      for (let index = 0; index < length; index += 1)
        difference |= (actual[index] || 0) ^ (expected[index] || 0);
      return Promise.resolve(difference === 0);
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.encrypt = (algorithm, key, data) => {
    try {
      const state = workerCryptoKeyState(key);
      if (!["AES-GCM", "AES-CBC", "AES-CTR"].includes(state.kind))
        throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
      if (!state.usages.includes("encrypt"))
        throw new WorkerDOMExceptionNative("native Worker CryptoKey cannot encrypt", "InvalidAccessError");
      const bytes = workerCryptoBufferInput(data);
      if (state.kind === "AES-GCM") {
        const normalized = workerCryptoAesAlgorithm(algorithm);
        if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__ - 16)
          throw new WorkerDOMExceptionNative("native Worker AES-GCM data is too large", "DataError");
        return Promise.resolve(new Uint8Array(workerCryptoAesBytes("encrypt", state, normalized, bytes)).buffer);
      }
      if (state.kind === "AES-CBC") {
        const normalized = workerCryptoAesCbcAlgorithm(algorithm);
        if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__ - 16)
          throw new WorkerDOMExceptionNative("native Worker AES-CBC data is too large", "DataError");
        try {
          return Promise.resolve(new Uint8Array(
            workerCryptoAesCbcBytes("encrypt", state, normalized, bytes)
          ).buffer);
        } catch (_error) {
          throw new WorkerDOMExceptionNative("native Worker AES-CBC operation failed", "OperationError");
        }
      }
      const normalized = workerCryptoAesCtrAlgorithm(algorithm);
      if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
        throw new WorkerDOMExceptionNative("native Worker AES-CTR data is too large", "DataError");
      try {
        return Promise.resolve(new Uint8Array(
          workerCryptoAesCtrBytes(state, normalized, bytes)
        ).buffer);
      } catch (_error) {
        throw new WorkerDOMExceptionNative("native Worker AES-CTR operation failed", "OperationError");
      }
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.decrypt = (algorithm, key, data) => {
    try {
      const state = workerCryptoKeyState(key);
      if (!["AES-GCM", "AES-CBC", "AES-CTR"].includes(state.kind))
        throw new WorkerDOMExceptionNative("native Worker crypto algorithm is unsupported", "NotSupportedError");
      if (!state.usages.includes("decrypt"))
        throw new WorkerDOMExceptionNative("native Worker CryptoKey cannot decrypt", "InvalidAccessError");
      const bytes = workerCryptoBufferInput(data);
      if (state.kind === "AES-GCM") {
        const normalized = workerCryptoAesAlgorithm(algorithm);
        if (bytes.length < 16 || bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
          throw new WorkerDOMExceptionNative("native Worker AES-GCM ciphertext is invalid", "DataError");
        try {
          return Promise.resolve(new Uint8Array(workerCryptoAesBytes("decrypt", state, normalized, bytes)).buffer);
        } catch (_error) {
          return Promise.reject(new WorkerDOMExceptionNative("native Worker AES-GCM authentication failed", "OperationError"));
        }
      }
      if (state.kind === "AES-CBC") {
        const normalized = workerCryptoAesCbcAlgorithm(algorithm);
        if (bytes.length === 0 || bytes.length % 16 !== 0 || bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
          throw new WorkerDOMExceptionNative("native Worker AES-CBC ciphertext is invalid", "DataError");
        try {
          return Promise.resolve(new Uint8Array(
            workerCryptoAesCbcBytes("decrypt", state, normalized, bytes)
          ).buffer);
        } catch (_error) {
          return Promise.reject(new WorkerDOMExceptionNative("native Worker AES-CBC padding is invalid", "OperationError"));
        }
      }
      const normalized = workerCryptoAesCtrAlgorithm(algorithm);
      if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
        throw new WorkerDOMExceptionNative("native Worker AES-CTR ciphertext is too large", "DataError");
      try {
        return Promise.resolve(new Uint8Array(
          workerCryptoAesCtrBytes(state, normalized, bytes)
        ).buffer);
      } catch (_error) {
        return Promise.reject(new WorkerDOMExceptionNative("native Worker AES-CTR operation failed", "OperationError"));
      }
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.wrapKey = (format, key, wrappingKey, wrapAlgorithm) => {
    try {
      if (String(format).toLowerCase() !== "raw")
        throw new WorkerDOMExceptionNative("native Worker crypto key format is unsupported", "NotSupportedError");
      const source = workerCryptoKeyState(key);
      if (!source.extractable)
        throw new WorkerDOMExceptionNative("native Worker CryptoKey is not extractable", "InvalidAccessError");
      if (!["HMAC", "AES-GCM"].includes(source.kind))
        throw new WorkerDOMExceptionNative("native Worker crypto key kind is unsupported", "NotSupportedError");
      const wrapping = workerCryptoKeyState(wrappingKey);
      if (wrapping.kind !== "AES-GCM")
        throw new WorkerDOMExceptionNative("native Worker wrapping key is unsupported", "NotSupportedError");
      if (!wrapping.usages.includes("encrypt"))
        throw new WorkerDOMExceptionNative("native Worker wrapping key cannot encrypt", "InvalidAccessError");
      const normalized = workerCryptoAesAlgorithm(wrapAlgorithm);
      if (source.bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__ - 16)
        throw new WorkerDOMExceptionNative("native Worker wrapped key is too large", "DataError");
      return Promise.resolve(new Uint8Array(
        workerCryptoAesBytes("encrypt", wrapping, normalized, source.bytes.slice())
      ).buffer);
    } catch (error) {
      return Promise.reject(error);
    }
  };
  workerSubtle.unwrapKey = (format, wrappedKey, unwrappedKeyAlgorithm, unwrappingKey, unwrapAlgorithm, extractable = false, keyUsages = []) => {
    try {
      if (String(format).toLowerCase() !== "raw")
        throw new WorkerDOMExceptionNative("native Worker crypto key format is unsupported", "NotSupportedError");
      const unwrapping = workerCryptoKeyState(unwrappingKey);
      if (unwrapping.kind !== "AES-GCM")
        throw new WorkerDOMExceptionNative("native Worker unwrapping key is unsupported", "NotSupportedError");
      if (!unwrapping.usages.includes("decrypt"))
        throw new WorkerDOMExceptionNative("native Worker unwrapping key cannot decrypt", "InvalidAccessError");
      const normalized = workerCryptoAesAlgorithm(unwrapAlgorithm);
      const wrapped = workerCryptoBufferInput(wrappedKey);
      if (wrapped.length < 16 || wrapped.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
        throw new WorkerDOMExceptionNative("native Worker wrapped key is invalid", "DataError");
      let bytes;
      try {
        bytes = workerCryptoAesBytes("decrypt", unwrapping, normalized, wrapped);
      } catch (_error) {
        throw new WorkerDOMExceptionNative("native Worker wrapped key authentication failed", "OperationError");
      }
      const algorithmName = unwrappedKeyAlgorithm && typeof unwrappedKeyAlgorithm === "object"
        ? String(unwrappedKeyAlgorithm.name || "").toUpperCase()
        : "";
      if (algorithmName === "HMAC") {
        if (bytes.length === 0)
          throw new WorkerDOMExceptionNative("native Worker unwrapped HMAC key is invalid", "DataError");
        const hashName = workerCryptoHmacHash(unwrappedKeyAlgorithm);
        return Promise.resolve(workerCryptoMakeKey(
          bytes, hashName, extractable, workerCryptoUsages(keyUsages)
        ));
      }
      if (algorithmName === "AES-GCM") {
        const length = Number(unwrappedKeyAlgorithm.length);
        if (![128, 192, 256].includes(length) || bytes.length !== length / 8)
          throw new WorkerDOMExceptionNative("native Worker unwrapped AES-GCM key length is invalid", "DataError");
        return Promise.resolve(workerCryptoMakeAesKey(
          bytes, extractable, workerCryptoAesUsages(keyUsages)
        ));
      }
      throw new WorkerDOMExceptionNative("native Worker unwrapped key algorithm is unsupported", "NotSupportedError");
    } catch (error) {
      return Promise.reject(error);
    }
  };
  globalThis.__glassWorkerSubtleCrypto = workerSubtle;
  workerCrypto.subtle = workerSubtle;
  globalThis.crypto = workerCrypto;
  try { delete globalThis.__glassNativeRandomBytes; } catch (_error) {}
  try { delete globalThis.__glassNativeCryptoDigest; } catch (_error) {}
  try { delete globalThis.__glassNativeCryptoHmac; } catch (_error) {}
  try { delete globalThis.__glassNativeCryptoEd25519; } catch (_error) {}
  try { delete globalThis.__glassNativeCryptoAesGcm; } catch (_error) {}
  try { delete globalThis.__glassNativeCryptoAesCbc; } catch (_error) {}
  try { delete globalThis.__glassNativeCryptoAesCtr; } catch (_error) {}
  try { delete globalThis.__glassNativeCryptoDerive; } catch (_error) {}
  try { delete globalThis.__glassNativeArrayBufferIsAttached; } catch (_error) {}
  try { delete globalThis.__glassNativeDetachArrayBuffer; } catch (_error) {}
