(() => {{
  let cachedHostState = null;
  let cachedGeometrySnapshot = null;
  let cachedGeometryMap = null;
  let cachedMutationShadow = null;
  // Kept in the installer closure so page script cannot replace the cached
  // node collections. Reuse still requires the native exact-state key and
  // host-map coverage checks in detached_nodes_and_snapshots.js.
  let cachedHostCollections = null;
  // Canvas resource snapshots can contain base64 encoded image pixels. Keep
  // their decoded representations private to the installer and reuse them
  // while the immutable snapshot arrays are unchanged.
  let cachedCanvasLiveIndexSource = null;
  let cachedCanvasLiveIndexes = null;
  let cachedCanvasResourceSource = null;
  let cachedCanvasResources = null;
  let cachedCanvasImageResourceSource = null;
  let cachedCanvasImageResources = null;
  let releaseCurrentHostSnapshot = null;
  const nativeElementClassListCache = new WeakMap();
  const nativeElementStyleCache = new WeakMap();
  const nativeElementDatasetCache = new WeakMap();
  return (__glassHostViewJson, releasePreviousSnapshot = false) => {{
  // A changed host view can arrive while the prior cached snapshot is large.
  // Drop snapshot-only caches before parsing the replacement so the old and
  // new JSON object graphs do not overlap at peak QuickJS heap usage. Keep the
  // private node collections: they preserve wrapper identity across refreshes.
  if (releasePreviousSnapshot) {{
    if (releaseCurrentHostSnapshot) {{
      releaseCurrentHostSnapshot();
      releaseCurrentHostSnapshot = null;
    }}
    if (cachedHostState && typeof cachedHostState === "object") {{
      for (const key of ["elements", "nodes", "geometry", "canvasResources", "imageResources"]) {{
        const value = cachedHostState[key];
        if (Array.isArray(value)) value.length = 0;
      }}
      if (typeof cachedHostState.visibleText === "string") cachedHostState.visibleText = "";
    }}
    if (cachedHostCollections && Array.isArray(cachedHostCollections.elements)) {{
      for (const element of cachedHostCollections.elements) {{
        if (element && typeof element.__glassReleaseHostSnapshot === "function")
          element.__glassReleaseHostSnapshot();
      }}
    }}
    if (cachedGeometryMap instanceof Map) cachedGeometryMap.clear();
    cachedHostState = null;
    cachedGeometrySnapshot = null;
    cachedGeometryMap = null;
    cachedMutationShadow = null;
    cachedCanvasLiveIndexSource = null;
    cachedCanvasLiveIndexes = null;
    cachedCanvasResourceSource = null;
    cachedCanvasResources = null;
    cachedCanvasImageResourceSource = null;
    cachedCanvasImageResources = null;
  }}
  if (__glassHostViewJson === null || __glassHostViewJson === undefined) return;
  let host = JSON.parse(__glassHostViewJson);
  // Do not keep the large wire string alive while the object graph and host
  // wrappers are being materialized.
  __glassHostViewJson = "";
  if (host.reuse_state) {{
    if (cachedHostState === null) throw new Error("native host snapshot reuse has no prior state");
    host.state = cachedHostState;
  }}
  const hostStateDelta = Array.isArray(host.state_delta) ? host.state_delta : null;
  if (hostStateDelta) {{
    const validatedHostStateDelta = [];
    for (const operation of hostStateDelta) {{
      if (!Array.isArray(operation) || !Array.isArray(operation[1])
        || (operation[0] === 0 && operation.length !== 3)
        || (operation[0] === 1 && operation.length !== 2)
        || (operation[0] === 2 && operation.length !== 5)
        || (operation[0] !== 0 && operation[0] !== 1 && operation[0] !== 2))
        throw new TypeError("native host snapshot delta is invalid");
      const path = operation[1];
      if (path.length === 0) throw new TypeError("native host snapshot delta path is empty");
      let target = host.state;
      if (operation[0] === 2) {{
        for (const key of path) {{
          if ((typeof key !== "string" && !Number.isSafeInteger(key))
            || !target || typeof target !== "object"
            || !Object.prototype.hasOwnProperty.call(target, key))
            throw new TypeError("native host snapshot delta path is invalid");
          target = target[key];
        }}
        const index = operation[2];
        const deleteCount = operation[3];
        const items = operation[4];
        if (!Array.isArray(target) || !Number.isSafeInteger(index) || index < 0
          || !Number.isSafeInteger(deleteCount) || deleteCount < 0
          || index > target.length || deleteCount > target.length - index
          || !Array.isArray(items) || target.length - deleteCount + items.length > 16384)
          throw new TypeError("native host snapshot delta splice is invalid");
        validatedHostStateDelta.push([2, target, index, deleteCount, items]);
        continue;
      }}
      for (let index = 0; index < path.length - 1; index += 1) {{
        const key = path[index];
        if ((typeof key !== "string" && !Number.isSafeInteger(key))
          || !target || typeof target !== "object"
          || !Object.prototype.hasOwnProperty.call(target, key))
          throw new TypeError("native host snapshot delta path is invalid");
        target = target[key];
      }}
      if (!target || typeof target !== "object")
        throw new TypeError("native host snapshot delta target is invalid");
      const key = path[path.length - 1];
      if ((Array.isArray(target) && (!Number.isSafeInteger(key) || key < 0 || key >= target.length))
        || (!Array.isArray(target) && typeof key !== "string")
        || (operation[0] === 1
          && (!Object.prototype.hasOwnProperty.call(target, key) || Array.isArray(target))))
        throw new TypeError("native host snapshot delta key is invalid");
      validatedHostStateDelta.push(
        operation[0] === 0 ? [0, target, key, operation[2]] : [1, target, key],
      );
    }}
    for (const operation of validatedHostStateDelta) {{
      if (operation[0] === 2) {{
        operation[1].splice(operation[2], operation[3], ...operation[4]);
      }} else if (operation[0] === 0) {{
        const [, target, key, value] = operation;
        Object.defineProperty(target, key, {{
          configurable: true,
          enumerable: true,
          writable: true,
          value,
        }});
      }} else {{
        const [, target, key] = operation;
        delete target[key];
      }}
    }}
    delete host.state_delta;
  }}
  const bootstrapParams = host.__glassBootstrap || {{}};
  delete host.__glassBootstrap;
  const documentContextId = String(host.context_id || "native");
  const documentGeneration = Number(host.generation) || 0;
  const initialDocumentReferrerPolicy = String(
    host.document_referrer_policy || "strict-origin-when-cross-origin",
  );
  const documentReferrerPolicyStates = globalThis.__glassDocumentReferrerPolicyStates instanceof Map
    ? globalThis.__glassDocumentReferrerPolicyStates
    : (globalThis.__glassDocumentReferrerPolicyStates = new Map());
  let documentReferrerPolicy = documentReferrerPolicyStates.get(documentContextId);
  if (!documentReferrerPolicy || documentReferrerPolicy.generation !== documentGeneration) {{
    documentReferrerPolicy = {{ generation: documentGeneration, value: initialDocumentReferrerPolicy }};
    documentReferrerPolicyStates.set(documentContextId, documentReferrerPolicy);
  }} else {{
    documentReferrerPolicy.value = initialDocumentReferrerPolicy;
  }}
  let applyDynamicReferrerMetaPolicyToNode = null;
  let captureDynamicReferrerMetaPolicy = null;
  globalThis.__glassMessageRealmKey = "page:" + String(host.context_id || "native");
  const initialPageCryptoBytes = bootstrapParams.initialPageCryptoBytes;
  let state = host.state;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("json_state_parsed");
  const reuseCachedHostSnapshotState = Boolean(
    host.reuse_state && hostStateDelta === null
      && cachedHostState !== null && state === cachedHostState,
  );
  let snapshotComputedStyles = Array.isArray(state.computedStyles) ? state.computedStyles : [];
  if (!reuseCachedHostSnapshotState) {{
    for (const element of Array.isArray(state.elements) ? state.elements : []) {{
      if (!element || !Number.isSafeInteger(Number(element.cs))) continue;
      const styleIndex = Number(element.cs);
      element.computedStyle = snapshotComputedStyles[styleIndex] || {{}};
      delete element.cs;
    }}
  }}
  const createSnapshotTextContentResolver = (snapshots) => {{
    const nodes = new Map();
    if (Array.isArray(snapshots)) for (const entry of snapshots) {{
      if (entry && Number.isSafeInteger(Number(entry.nodeIndex)))
        nodes.set(Number(entry.nodeIndex), entry);
    }}
    const cached = new Map();
    const resolve = (nodeIndex, active) => {{
      const index = Number(nodeIndex);
      if (!Number.isSafeInteger(index)) return "";
      if (cached.has(index)) return cached.get(index);
      const entry = nodes.get(index);
      if (!entry || active.has(index)) return "";
      if (Number(entry.nodeType) === 3) {{
        const value = String(entry.nodeValue || "");
        cached.set(index, value);
        return value;
      }}
      if ([8, 10].includes(Number(entry.nodeType))) return "";
      active.add(index);
      const value = (Array.isArray(entry.children) ? entry.children : [])
        .map((childIndex) => resolve(childIndex, active)).join("");
      active.delete(index);
      cached.set(index, value);
      return value;
    }};
    return (nodeIndex) => resolve(nodeIndex, new Set());
  }};
  let snapshotGeometry = Array.isArray(state.geometry) ? state.geometry : [];
  const cachedGeometryStillMatchesSnapshot = Boolean(
    cachedGeometryMap instanceof Map
      && cachedGeometryMap.size === snapshotGeometry.filter((entry) => entry && entry.nodeIndex !== undefined).length
      && snapshotGeometry.every((entry) => !entry || entry.nodeIndex === undefined
        || cachedGeometryMap.get(Number(entry.nodeIndex)) === entry),
  );
  const refreshCachedGeometryInPlace = Boolean(
    host.reuse_host_tree === true
      && hostStateDelta !== null
      && cachedGeometryMap instanceof Map
      && globalThis.__glassHostGeometry === cachedGeometryMap,
  );
  const reuseCachedGeometry = Boolean(
    refreshCachedGeometryInPlace
      || (reuseCachedHostSnapshotState
        && cachedGeometrySnapshot === state.geometry
        && cachedGeometryStillMatchesSnapshot
        && globalThis.__glassHostGeometry === cachedGeometryMap),
  );
  const geometryByIndex = reuseCachedGeometry ? cachedGeometryMap : new Map();
  if (!reuseCachedGeometry || refreshCachedGeometryInPlace) {{
    for (const entry of snapshotGeometry) {{
      if (!entry || entry.nodeIndex === undefined) continue;
      geometryByIndex.set(Number(entry.nodeIndex), entry);
    }}
  }}
  globalThis.__glassHostGeometry = geometryByIndex;
  const zeroGeometry = () => ({{
    x: 0,
    y: 0,
    width: 0,
    height: 0,
    contentX: 0,
    contentY: 0,
    contentWidth: 0,
    contentHeight: 0,
    scrollX: 0,
    scrollY: 0,
    scrollWidth: 0,
    scrollHeight: 0,
    clientWidth: 0,
    clientHeight: 0,
  }});
  const geometryForNode = (node) => {{
    if (node && typeof node.__glassGeometrySource === "function") {{
      const projected = node.__glassGeometrySource();
      if (projected) return projected;
    }}
    if (node && node.__glassGeometry) return node.__glassGeometry;
    return node && typeof node.nodeIndex === "number"
      ? geometryByIndex.get(Number(node.nodeIndex)) || zeroGeometry()
      : zeroGeometry();
  }};
  const makeDomRect = (geometry) => {{
    const value = geometry || zeroGeometry();
    const x = Number(value.x) || 0;
    const y = Number(value.y) || 0;
    const width = Math.max(0, Number(value.width) || 0);
    const height = Math.max(0, Number(value.height) || 0);
    const Constructor = typeof globalThis.DOMRect === "function" ? globalThis.DOMRect : null;
    return Constructor
      ? new Constructor(x, y, width, height)
      : {{
          x, y, width, height,
          top: y,
          right: x + width,
          bottom: y + height,
          left: x,
          toJSON() {{ return {{ x, y, width, height, top: y, right: x + width, bottom: y + height, left: x }}; }},
        }};
  }};
  const resizeObservers = globalThis.__glassResizeObservers instanceof Set
    ? globalThis.__glassResizeObservers
    : new Set();
  globalThis.__glassResizeObservers = resizeObservers;
  let resizeDeliveryQueued = false;
  const resizeGeometryChanged = (previous, next) => !previous
    || previous.width !== next.width
    || previous.height !== next.height
    || previous.contentWidth !== next.contentWidth
    || previous.contentHeight !== next.contentHeight;
  const resizeEntry = (target, geometry) => {{
    const contentRect = makeDomRect({{
      x: geometry.contentX,
      y: geometry.contentY,
      width: geometry.contentWidth,
      height: geometry.contentHeight,
    }});
    const borderBoxSize = [{{ inlineSize: geometry.width, blockSize: geometry.height }}];
    const contentBoxSize = [{{ inlineSize: geometry.contentWidth, blockSize: geometry.contentHeight }}];
    const devicePixelContentBoxSize = [{{
      inlineSize: geometry.contentWidth * (Number(host.device_scale_factor_milli || 1000) / 1000),
      blockSize: geometry.contentHeight * (Number(host.device_scale_factor_milli || 1000) / 1000),
    }}];
    return {{
      target,
      contentRect,
      borderBoxSize,
      contentBoxSize,
      devicePixelContentBoxSize,
    }};
  }};
  const deliverResizeObservers = () => {{
    resizeDeliveryQueued = false;
    for (const observer of Array.from(resizeObservers)) {{
      if (!observer.__glassRecords || observer.__glassRecords.length === 0) continue;
      const records = observer.__glassRecords.splice(0, observer.__glassRecords.length);
      observer.__glassCallback(records, observer);
    }}
  }};
  const scheduleResizeObserverDelivery = () => {{
    if (resizeDeliveryQueued) return;
    resizeDeliveryQueued = true;
    Promise.resolve().then(deliverResizeObservers);
  }};
  const queueResizeObserverChanges = () => {{
    for (const observer of Array.from(resizeObservers)) {{
      for (const registration of observer.__glassRegistrations || []) {{
        const geometry = geometryForNode(registration.target);
        if (!resizeGeometryChanged(registration.last, geometry)) continue;
        registration.last = {{ ...geometry }};
        observer.__glassRecords.push(resizeEntry(registration.target, geometry));
        if (observer.__glassRecords.length > {max_commands}) {{
          observer.__glassRecords.splice(0, observer.__glassRecords.length - {max_commands});
        }}
      }}
    }}
    if (Array.from(resizeObservers).some((observer) => observer.__glassRecords.length > 0)) {{
      scheduleResizeObserverDelivery();
    }}
  }};
  const intersectionObservers = globalThis.__glassIntersectionObservers instanceof Set
    ? globalThis.__glassIntersectionObservers
    : new Set();
  globalThis.__glassIntersectionObservers = intersectionObservers;
  let intersectionDeliveryQueued = false;
  const intersectionViewport = () => ({{
    x: 0,
    y: 0,
    width: Math.max(0, Number(bootstrapParams.width) || 0),
    height: Math.max(0, Number(bootstrapParams.height) || 0),
  }});
  const intersectionRect = (geometry) => {{
    const value = geometry || zeroGeometry();
    return {{
      x: Number(value.x) || 0,
      y: Number(value.y) || 0,
      width: Math.max(0, Number(value.width) || 0),
      height: Math.max(0, Number(value.height) || 0),
    }};
  }};
  const intersectionWith = (left, right) => {{
    const x = Math.max(left.x, right.x);
    const y = Math.max(left.y, right.y);
    const rightEdge = Math.min(left.x + left.width, right.x + right.width);
    const bottomEdge = Math.min(left.y + left.height, right.y + right.height);
    return {{
      x,
      y,
      width: Math.max(0, rightEdge - x),
      height: Math.max(0, bottomEdge - y),
    }};
  }};
  const intersectionWithMargin = (rect, margin) => ({{
    x: rect.x - margin[3],
    y: rect.y - margin[0],
    width: Math.max(0, rect.width + margin[1] + margin[3]),
    height: Math.max(0, rect.height + margin[0] + margin[2]),
  }});
  const intersectionArea = (rect) => rect.width * rect.height;
  const intersectionState = (registration) => {{
    const viewport = intersectionViewport();
    const targetRect = intersectionRect(geometryForNode(registration.target));
    let rootRect = registration.root && Number(registration.root.nodeType) === 1
      ? intersectionRect(geometryForNode(registration.root))
      : viewport;
    rootRect = intersectionWithMargin(rootRect, registration.rootMargin);
    const rootIntersection = registration.root && Number(registration.root.nodeType) === 1
      ? intersectionWith(rootRect, viewport)
      : rootRect;
    const visible = intersectionWith(targetRect, rootIntersection);
    const targetArea = intersectionArea(targetRect);
    const visibleArea = intersectionArea(visible);
    const isIntersecting = visible.width > 0 && visible.height > 0;
    return {{
      rootBounds: rootIntersection,
      boundingClientRect: targetRect,
      intersectionRect: visible,
      isIntersecting,
      intersectionRatio: targetArea > 0 ? visibleArea / targetArea : (isIntersecting ? 1 : 0),
    }};
  }};
  const intersectionThresholdCrossed = (previous, next, thresholds) => {{
    if (!previous) return true;
    if (previous.isIntersecting !== next.isIntersecting) return true;
    return thresholds.some((threshold) =>
      (previous.intersectionRatio < threshold) !== (next.intersectionRatio < threshold)
    );
  }};
  const intersectionEntry = (target, observation) => new IntersectionObserverEntryNative(
    Number(host.now_ms) || 0,
    {{
      target,
      rootBounds: makeDomRect(observation.rootBounds),
      boundingClientRect: makeDomRect(observation.boundingClientRect),
      intersectionRect: makeDomRect(observation.intersectionRect),
      isIntersecting: observation.isIntersecting,
      intersectionRatio: observation.intersectionRatio,
      isVisible: observation.isIntersecting,
    }},
  );
  const deliverIntersectionObservers = () => {{
    intersectionDeliveryQueued = false;
    for (const observer of Array.from(intersectionObservers)) {{
      if (!observer.__glassRecords || observer.__glassRecords.length === 0) continue;
      const records = observer.__glassRecords.splice(0, observer.__glassRecords.length);
      observer.__glassCallback(records, observer);
    }}
  }};
  const scheduleIntersectionObserverDelivery = () => {{
    if (intersectionDeliveryQueued) return;
    intersectionDeliveryQueued = true;
    Promise.resolve().then(deliverIntersectionObservers);
  }};
  const queueIntersectionObserverChanges = () => {{
    for (const observer of Array.from(intersectionObservers)) {{
      for (const registration of observer.__glassRegistrations || []) {{
        const observation = intersectionState(registration);
        if (!intersectionThresholdCrossed(
          registration.last,
          observation,
          registration.thresholds,
        )) continue;
        registration.last = {{
          isIntersecting: observation.isIntersecting,
          intersectionRatio: observation.intersectionRatio,
        }};
        observer.__glassRecords.push(intersectionEntry(registration.target, observation));
        if (observer.__glassRecords.length > {max_commands}) {{
          observer.__glassRecords.splice(0, observer.__glassRecords.length - {max_commands});
        }}
      }}
    }}
    if (Array.from(intersectionObservers).some((observer) => observer.__glassRecords.length > 0)) {{
      scheduleIntersectionObserverDelivery();
    }}
  }};
  const commands = [];
  let suppressHostCommands = 0;
  const activeCommands = () => Array.isArray(globalThis.__glassHostCommandBuffer)
    ? globalThis.__glassHostCommandBuffer
    : commands;
  const pushCommand = (command) => {{
    if (suppressHostCommands > 0) return;
    if (typeof globalThis.__glassRouteTransferredNativeCommand === "function"
        && globalThis.__glassRouteTransferredNativeCommand(command)) {{
      if (typeof globalThis.__glassRecordMutationCommand === "function") {{
        globalThis.__glassRecordMutationCommand(command);
      }}
      return;
    }}
    const target = activeCommands();
    if (target.length >= {max_commands}) throw new RangeError("native host command limit exceeded");
    target.push(command);
    if (typeof globalThis.__glassRecordMutationCommand === "function") {{
      globalThis.__glassRecordMutationCommand(command);
    }}
  }};
  const pushCurrentDocumentCommand = (command) => {{
    const target = activeCommands();
    if (target.length >= {max_commands}) throw new RangeError("native host command limit exceeded");
    target.push(command);
    if (typeof globalThis.__glassRecordMutationCommand === "function") {{
      globalThis.__glassRecordMutationCommand(command);
    }}
  }};
  let nativeFormAssociatedDisabledStatesForFocus = null;
  const nativeFormAssociatedDisabledForFocus = (element) =>
    nativeFormAssociatedDisabledStatesForFocus instanceof WeakMap
      && nativeFormAssociatedDisabledStatesForFocus.get(element) === true;
  const nativeInertForFocus = (element) => {{
    let current = element;
    while (current) {{
      if (typeof current.hasAttribute === "function" && current.hasAttribute("inert")) {{
        return true;
      }}
      current = current.parentElement;
    }}
    return false;
  }};
  const timers = globalThis.__glassTimers instanceof Map
    ? globalThis.__glassTimers
    : new Map();
  const runningTimers = globalThis.__glassRunningTimers instanceof Map
    ? globalThis.__glassRunningTimers
    : new Map();
  const animationFrames = globalThis.__glassAnimationFrames instanceof Map
    ? globalThis.__glassAnimationFrames
    : new Map();
  const idleCallbacks = globalThis.__glassIdleCallbacks instanceof Map
    ? globalThis.__glassIdleCallbacks
    : new Map();
  const cpuThrottleRate = Number(host.cpu_throttling_rate) > 0
    ? Number(host.cpu_throttling_rate)
    : 1;
  let nextAnimationFrameId = Number.isSafeInteger(globalThis.__glassNextAnimationFrameId)
    ? globalThis.__glassNextAnimationFrameId
    : 1;
  let nextIdleCallbackId = Number.isSafeInteger(globalThis.__glassNextIdleCallbackId)
    ? globalThis.__glassNextIdleCallbackId
    : 1;
  let nextTimerId = Number.isSafeInteger(globalThis.__glassNextTimerId)
    ? globalThis.__glassNextTimerId
    : 1;
  const scheduleTimer = (callback, delay, args, repeating) => {{
    if (typeof callback !== "function") throw new TypeError("timer callback must be callable");
    if (timers.size >= {max_timers}) throw new RangeError("native timer limit exceeded");
    const id = nextTimerId;
    nextTimerId += 1;
    globalThis.__glassNextTimerId = nextTimerId;
    const numericDelay = Number(delay);
    const normalizedDelay = Number.isFinite(numericDelay)
      ? Math.max(0, Math.min(2147483647, numericDelay))
      : 0;
    timers.set(id, {{
      callback,
      args,
      dueAt: host.now_ms + (normalizedDelay * cpuThrottleRate),
      intervalMs: repeating ? Math.max(1, normalizedDelay * cpuThrottleRate) : 0,
      cancelled: false,
    }});
    return id;
  }};
  const setTimeoutNative = (callback, delay, ...args) =>
    scheduleTimer(callback, delay, args, false);
  const setIntervalNative = (callback, delay, ...args) =>
    scheduleTimer(callback, delay, args, true);
  const clearTimer = (id) => {{
    const timerId = Number(id);
    const timer = timers.get(timerId) || runningTimers.get(timerId);
    if (timer) timer.cancelled = true;
    timers.delete(timerId);
  }};
  const requestAnimationFrameNative = (callback) => {{
    if (typeof callback !== "function") throw new TypeError("animation frame callback must be callable");
    if (animationFrames.size >= {max_timers}) throw new RangeError("native animation frame limit exceeded");
    const id = nextAnimationFrameId;
    nextAnimationFrameId += 1;
    globalThis.__glassNextAnimationFrameId = nextAnimationFrameId;
    animationFrames.set(id, {{
      callback,
      dueAt: host.now_ms + (16 * cpuThrottleRate),
    }});
    return id;
  }};
  const cancelAnimationFrameNative = (id) => {{
    animationFrames.delete(Number(id));
  }};
  const requestIdleCallbackNative = (callback, options) => {{
    if (typeof callback !== "function") throw new TypeError("idle callback must be callable");
    if (options !== undefined && (options === null || typeof options !== "object"))
      throw new TypeError("idle callback options must be an object");
    if (idleCallbacks.size >= {max_timers}) throw new RangeError("native idle callback limit exceeded");
    const timeoutValue = options && options.timeout !== undefined ? Number(options.timeout) : null;
    const timeout = timeoutValue === null
      ? null
      : Number.isFinite(timeoutValue) ? Math.max(0, Math.min(2147483647, timeoutValue)) : null;
    const id = nextIdleCallbackId;
    nextIdleCallbackId += 1;
    globalThis.__glassNextIdleCallbackId = nextIdleCallbackId;
    idleCallbacks.set(id, {{
      callback,
      scheduledAt: host.now_ms,
      timeoutAt: timeout === null ? null : host.now_ms + timeout,
    }});
    return id;
  }};
  const cancelIdleCallbackNative = (id) => {{
    idleCallbacks.delete(Number(id));
  }};
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("module_runtime_timers");
