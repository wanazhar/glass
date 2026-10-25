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
  const nativeElementClassListCache = new WeakMap();
  const nativeElementStyleCache = new WeakMap();
  const nativeElementDatasetCache = new WeakMap();
  return (__glassHostViewJson) => {{
  const host = JSON.parse(__glassHostViewJson);
  // Do not keep the large wire string alive while the object graph and host
  // wrappers are being materialized.
  __glassHostViewJson = "";
  if (host.reuse_state) {{
    if (cachedHostState === null) throw new Error("native host snapshot reuse has no prior state");
    host.state = cachedHostState;
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
  const state = host.state;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("json_state_parsed");
  const reuseCachedHostSnapshotState = Boolean(
    host.reuse_state && cachedHostState !== null && state === cachedHostState,
  );
  const snapshotComputedStyles = Array.isArray(state.computedStyles) ? state.computedStyles : [];
  if (!reuseCachedHostSnapshotState) {{
    for (const element of Array.isArray(state.elements) ? state.elements : []) {{
      if (!element || !Number.isSafeInteger(Number(element.computedStyleIndex))) continue;
      const styleIndex = Number(element.computedStyleIndex);
      element.computedStyle = snapshotComputedStyles[styleIndex] || {{}};
      delete element.computedStyleIndex;
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
  const snapshotGeometry = Array.isArray(state.geometry) ? state.geometry : [];
  const cachedGeometryStillMatchesSnapshot = Boolean(
    cachedGeometryMap instanceof Map
      && cachedGeometryMap.size === snapshotGeometry.filter((entry) => entry && entry.nodeIndex !== undefined).length
      && snapshotGeometry.every((entry) => !entry || entry.nodeIndex === undefined
        || cachedGeometryMap.get(Number(entry.nodeIndex)) === entry),
  );
  const reuseCachedGeometry = Boolean(
    reuseCachedHostSnapshotState
      && cachedGeometrySnapshot === state.geometry
      && cachedGeometryStillMatchesSnapshot
      && globalThis.__glassHostGeometry === cachedGeometryMap,
  );
  const geometryByIndex = reuseCachedGeometry ? cachedGeometryMap : new Map();
  if (!reuseCachedGeometry) {{
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
