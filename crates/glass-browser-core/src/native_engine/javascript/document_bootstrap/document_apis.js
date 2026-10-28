  const body = elements.find((element) => element.tagName === "BODY") || null;
  const documentElement = elements.find((element) => element.tagName === "HTML") || null;
  const previousHostDocument = globalThis.__glassHostDocument || null;
  const rootSnapshot = snapshotNodes.find((entry) => entry && Number(entry.nodeIndex) === 0);
  const rootChildren = rootSnapshot && Array.isArray(rootSnapshot.children)
    ? rootSnapshot.children.map((index) => nodesByIndex.get(index)).filter(Boolean)
    : elements.filter((element) => element.parentIndex === null);
  const liveDocumentElements = () => {{
    const values = [];
    const seen = new Set();
    const visit = (node) => {{
      if (!node || seen.has(node)) return;
      seen.add(node);
      if (Number(node.nodeType) === 1) values.push(node);
      for (const child of node.__glassChildren || []) visit(child);
    }};
    for (const root of rootChildren) visit(root);
    return values;
  }};
  {font_face_script}
  let documentCookie = typeof host.cookie === "string" ? host.cookie : "";
  const pendingDocumentCookieWrites = [];
  Object.defineProperty(globalThis, "__glassApplySyncCookieProjection", {{
    value(value) {{
      if (typeof value === "string" && value.length <= storageValueLimit)
        documentCookie = value;
    }},
    enumerable: false,
    configurable: true,
    writable: false,
  }});
  Object.defineProperty(globalThis, "__glassTakePendingCookieWrites", {{
    value() {{ return pendingDocumentCookieWrites.splice(0); }},
    enumerable: false,
    configurable: true,
    writable: false,
  }});
  const previewCookieSet = (current, value) => {{
    const pair = String(value).split(";", 1)[0].trim();
    const separator = pair.indexOf("=");
    if (separator <= 0) return current;
    const name = pair.slice(0, separator).trim();
    const pairs = current ? current.split("; ").filter(Boolean) : [];
    const existing = pairs.findIndex(candidate => candidate.slice(0, candidate.indexOf("=")).trim() === name);
    if (existing >= 0) pairs[existing] = pair;
    else pairs.push(pair);
    return pairs.join("; ");
  }};
  let documentTitle = String(state.title || "");
  const document = {{
    get title() {{
      const titleElement = liveDocumentElements().find((element) => element.tagName === "TITLE");
      return titleElement ? titleElement.textContent : documentTitle;
    }},
    set title(value) {{
      const text = String(value);
      if (text.length > storageValueLimit) throw new RangeError("native document.title exceeds its limit");
      documentTitle = text;
      const titleElement = liveDocumentElements().find((element) => element.tagName === "TITLE");
      if (titleElement) {{
        titleElement.textContent = text;
        return;
      }}
      let parent = document.head;
      if (!parent && documentElement) {{
        parent = makeDetachedElement("head");
        documentElement.insertBefore(parent, documentElement.firstChild);
      }}
      if (parent) {{
        const created = makeDetachedElement("title");
        created.textContent = text;
        parent.appendChild(created);
        return;
      }}
      pushCommand({{ kind: "setDocumentTitle", value: text }});
    }},
    body,
    documentElement,
    fonts: fontFaceSet,
    nodeType: 9,
    nodeName: "#document",
    __glassNativeContextId: String(host.context_id || "native"),
    __glassNativeGeneration: Number(host.generation) || 0,
    __glassNativeOrigin: String(host.origin || "null"),
    __glassNativeScriptNodes: Array.isArray(state.scriptNodes) ? state.scriptNodes : [],
    __glassNativeFrameBinding: null,
    __glassGetTemplateContentsOwnerDocument() {{ return getTemplateContentsOwnerDocument(); }},
    URL: host.url,
    documentURI: host.url,
    get baseURI() {{ return effectiveDocumentBaseUrl(); }},
    compatMode: host.compat_mode === "BackCompat" ? "BackCompat" : "CSS1Compat",
    hidden: false,
    visibilityState: "visible",
    get defaultView() {{ return globalThis; }},
    get activeElement() {{
      const focused = liveDocumentElements().find((element) => element.focused) || null;
      if (!focused || focused.focusAnchorNodeIndex === null) return focused;
      return nodesByIndex.get(Number(focused.focusAnchorNodeIndex)) || focused;
    }},
    get head() {{ return liveDocumentElements().find((element) => element.tagName === "HEAD") || null; }},
    get doctype() {{ return document.__glassChildren.find((node) => Number(node.nodeType) === 10) || null; }},
    get forms() {{ return liveCollection(document, (element) => element.tagName === "FORM", "HTMLCollection", true); }},
    get links() {{ return liveCollection(document, (element) => ["A", "AREA"].includes(element.tagName) && element.getAttribute("href") !== null, "HTMLCollection", true); }},
    get scripts() {{ return liveCollection(document, (element) => element.tagName === "SCRIPT", "HTMLCollection", true); }},
    get images() {{ return liveCollection(document, (element) => element.tagName === "IMG", "HTMLCollection", true); }},
    get scrollingElement() {{ return documentElement; }},
    get cookie() {{ return documentCookie; }},
    set cookie(value) {{
      const text = String(value);
      if (text.length > storageValueLimit) throw new RangeError("native document.cookie value exceeds its limit");
      documentCookie = previewCookieSet(documentCookie, text);
      pushCommand({{ kind: "cookieSet", value: text }});
      pendingDocumentCookieWrites.push(text);
    }},
    readyState: bootstrapParams.readyState,
    addEventListener(type, callback, options) {{
      addListener("document", type, callback, options);
    }},
    removeEventListener(type, callback, options) {{
      removeListener("document", type, callback, options);
    }},
    dispatchEvent(event) {{
      return dispatchTarget(this, event);
    }},
    createElement(tagName, options) {{
      const createCustom = globalThis.__glassCreateNativeCustomElement;
      return typeof createCustom === "function"
        ? createCustom(document, tagName, options, HTML_NAMESPACE)
        : makeDetachedElement(tagName, HTML_NAMESPACE);
    }},
    createElementNS(namespace, qualifiedName, options) {{
      const namespaceURI = normalizeElementNamespace(namespace);
      const createCustom = globalThis.__glassCreateNativeCustomElement;
      return typeof createCustom === "function"
        ? createCustom(document, qualifiedName, options, namespaceURI)
        : makeDetachedElement(qualifiedName, namespaceURI);
    }},
    importNode(node, options = false) {{
      return importNativeNode(document, node, options);
    }},
    adoptNode(node) {{ return adoptNativeNode(document, node); }},
    createAttribute(name) {{ return makeAttributeNode(name, "", () => document); }},
    createAttributeNS(namespace, qualifiedName) {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      return makeAttributeNode(qualifiedName, "", () => document, namespaceURI);
    }},
    createTextNode(value) {{ return makeDetachedText(value); }},
    createComment(value) {{ return makeDetachedComment(value); }},
    appendChild(child) {{
      if (!child || Number(child.nodeType) !== 10) throw new TypeError("Document children must be a document type");
      if (document.__glassChildren.includes(child)) return child;
      if (child.__glassParent && child.__glassParent !== document) throw new DOMExceptionNative("The document type has another parent", "HierarchyRequestError");
      const existing = document.__glassChildren.find((candidate) => Number(candidate.nodeType) === 10);
      if (existing && existing !== child) throw new DOMExceptionNative("The document already has a document type", "HierarchyRequestError");
      document.__glassChildren = document.__glassChildren.filter((candidate) => candidate !== child);
      document.__glassChildren.push(child);
      child.__glassParent = document;
      child.parentIndex = 0;
      pushCommand({{ kind: "appendChild", parent_index: 0, child_index: child.nodeIndex }});
      return child;
    }},
    insertBefore(child, before) {{
      if (before == null) return document.appendChild(child);
      if (!before || !document.__glassChildren.includes(before)) throw new TypeError("reference node is not a document child");
      if (!child || Number(child.nodeType) !== 10) throw new TypeError("Document children must be a document type");
      if (child === before) return child;
      const existing = document.__glassChildren.find((candidate) => Number(candidate.nodeType) === 10);
      if (existing && existing !== child) throw new DOMExceptionNative("The document already has a document type", "HierarchyRequestError");
      document.__glassChildren = document.__glassChildren.filter((candidate) => candidate !== child);
      const index = document.__glassChildren.indexOf(before);
      document.__glassChildren.splice(index < 0 ? document.__glassChildren.length : index, 0, child);
      child.__glassParent = document;
      child.parentIndex = 0;
      pushCommand({{ kind: "insertBefore", parent_index: 0, child_index: child.nodeIndex, before_index: before.nodeIndex }});
      return child;
    }},
    createDocumentFragment() {{ return makeDocumentFragment(); }},
    implementation: {{
      createDocumentType(name, publicId = "", systemId = "") {{
        return makeDetachedDocumentType(name, publicId, systemId);
      }},
    }},
    getElementById(id) {{ return liveDocumentElements().find((element) => element.id === String(id)) || null; }},
    querySelector(selector) {{ return findAll(selector)[0] || null; }},
    querySelectorAll(selector) {{ return findAll(selector); }},
    getElementsByTagName(name) {{
      const value = String(name).toLowerCase();
      return liveCollection(document, (element) => value === "*" || element.tagName.toLowerCase() === value, "HTMLCollection", true);
    }},
    getElementsByClassName(name) {{
      const value = String(name);
      return liveCollection(document, (element) => element.className.split(/\s+/).includes(value), "HTMLCollection", true);
    }},
    getElementsByName(name) {{
      const value = String(name);
      return asNodeList(liveDocumentElements().filter((element) => element.getAttribute("name") === value));
    }},
  }};
  Object.defineProperty(document, "__glassEventOwner", {{
    enumerable: false,
    configurable: false,
    value: "document",
  }});
  Object.defineProperty(document, "__glassChildren", {{
    enumerable: false,
    configurable: false,
    writable: true,
    value: rootChildren,
  }});
  defineTreeAccessors(document);
  globalThis.__glassHostDocument = document;
  if (reuseExistingHostTree) {{
    // The serialized tree and its nodes are reused, but the Document wrapper
    // is recreated for every bootstrap. Refresh every link because text,
    // comment, and doctype wrappers can otherwise retain an ancestor wrapper
    // from the previous bootstrap.
    const pendingParents = [[document, rootChildren]];
    const linkedNodes = new Set();
    while (pendingParents.length > 0) {{
      const [parent, children] = pendingParents.pop();
      for (const child of children || []) {{
        if (!child || typeof child !== "object" || linkedNodes.has(child)) continue;
        linkedNodes.add(child);
        child.__glassParent = parent;
        if (Array.isArray(child.__glassChildren)) pendingParents.push([child, child.__glassChildren]);
        // Template contents are a separate fragment tree whose children keep
        // that fragment as their parent; its own parent remains null.
        if (child.content && Array.isArray(child.content.__glassChildren)) {{
          pendingParents.push([child.content, child.content.__glassChildren]);
        }}
      }}
    }}
  }} else {{
    // Snapshot node zero is represented by this Document wrapper rather than
    // by an entry in nodesByIndex, so link its direct children explicitly on a
    // fresh host tree as well.
    for (const child of rootChildren) {{
      if (child && typeof child === "object") child.__glassParent = document;
    }}
  }}
  if (reuseExistingHostTree) {{
    if (previousHostDocument && previousHostDocument !== document) {{
      for (const observer of mutationObservers) {{
        for (const registration of observer.__glassRegistrations || []) {{
          if (registration.target === previousHostDocument) registration.target = document;
        }}
      }}
    }}
  }}
  globalThis.__glassDispatchHostEvents = (events) => {{
    const results = events.map((descriptor) => {{
      const target = descriptor.node_index === 0
        ? document
        : descriptor.node_index === 4294967295
          ? globalThis
          : elements.find((element) => element.nodeIndex === descriptor.node_index) || null;
      if (!target) throw new TypeError("native event target is detached");
      const eventType = String(descriptor.type);
      const focusEvent = ["blur", "focusout", "focus", "focusin"].includes(eventType);
      const eventFactory = focusEvent
        ? createFocusEvent
        : eventType === "beforeunload"
          ? createBeforeUnloadEvent
          : createEvent;
      const event = eventFactory(eventType, {{
        bubbles: Boolean(descriptor.bubbles),
        cancelable: Boolean(descriptor.cancelable),
        persisted: Boolean(descriptor.persisted),
        view: globalThis,
        relatedTarget: descriptor.related_target_node_index == null
          ? null
          : elements.find((element) => element.nodeIndex === descriptor.related_target_node_index) || null,
        key: descriptor.key,
        code: descriptor.code,
        altKey: Boolean(descriptor.alt_key),
        ctrlKey: Boolean(descriptor.ctrl_key),
        metaKey: Boolean(descriptor.meta_key),
        shiftKey: Boolean(descriptor.shift_key),
        submitter: descriptor.submitter_node_index == null
          ? null
          : elements.find((element) => element.nodeIndex === descriptor.submitter_node_index) || null,
        oldURL: descriptor.old_url,
        newURL: descriptor.new_url,
      }});
      if (event.type === "popstate" && globalThis.history) event.state = globalThis.history.state;
      return dispatchTarget(target, event);
    }});
    globalThis.__glassLastHostEventResults = results;
    return results;
  }};
  globalThis.__glassDispatchScriptError = (descriptor) => {{
    if (!descriptor || typeof descriptor !== "object") throw new TypeError("native script error is invalid");
    const message = String(descriptor.message || "");
    const filename = String(descriptor.filename || "");
    const line = Number(descriptor.lineno) || 0;
    const column = Number(descriptor.colno) || 0;
    const error = Object.prototype.hasOwnProperty.call(descriptor, "error")
      ? descriptor.error
      : new Error(message);
    const createErrorEvent = () => {{
      const constructor = globalThis.__glassErrorEventConstructor || globalThis.ErrorEvent;
      if (typeof constructor === "function") return new constructor("error", {{
        message,
        filename,
        lineno: line,
        colno: column,
        error,
        cancelable: true,
      }});
      const event = createEvent("error", {{ bubbles: false, cancelable: true }});
      event.message = message;
      event.filename = filename;
      event.lineno = line;
      event.colno = column;
      event.error = error;
      return event;
    }};
    const results = [];
    const nodeIndex = descriptor.node_index;
    if (nodeIndex !== null && nodeIndex !== undefined) {{
      const target = elements.find((element) => element.nodeIndex === Number(nodeIndex)) || null;
      if (!target) throw new TypeError("native script error target is detached");
      results.push(dispatchTarget(target, createErrorEvent()));
    }}
    results.push(dispatchTarget(globalThis, createErrorEvent()));
    return results;
  }};
  globalThis.__glassDispatchPromiseRejections = (type, rejections, cancelable) => {{
    const eventType = String(type || "");
    if (eventType !== "unhandledrejection" && eventType !== "rejectionhandled")
      throw new TypeError("native Promise rejection event type is invalid");
    if (!Array.isArray(rejections)) throw new TypeError("native Promise rejections are invalid");
    return rejections.map((descriptor) => {{
      const reason = descriptor && typeof descriptor === "object" && descriptor.reason !== undefined
        ? descriptor.reason
        : null;
      const constructor = globalThis.__glassPromiseRejectionEventConstructor
        || globalThis.PromiseRejectionEvent;
      const event = typeof constructor === "function"
        ? new constructor(eventType, {{ reason, promise: null, cancelable: Boolean(cancelable) }})
        : createEvent(eventType, {{ bubbles: false, cancelable: Boolean(cancelable) }});
      if (event.reason === undefined) event.reason = reason;
      if (event.promise === undefined) event.promise = null;
      return dispatchTarget(globalThis, event);
    }});
  }};
  globalThis.__glassDispatchCspViolations = (violations) => {{
    if (!Array.isArray(violations)) throw new TypeError("native CSP violations are invalid");
    return violations.map((descriptor) => {{
      if (!descriptor || typeof descriptor !== "object") throw new TypeError("native CSP violation is invalid");
      const event = new SecurityPolicyViolationEventNative("securitypolicyviolation", {{
        documentURI: descriptor.document_uri,
        referrer: descriptor.referrer,
        blockedURI: descriptor.blocked_uri,
        effectiveDirective: descriptor.effective_directive,
        violatedDirective: descriptor.violated_directive,
        originalPolicy: descriptor.original_policy,
        sourceFile: descriptor.source_file,
        sample: descriptor.sample,
        disposition: descriptor.disposition,
        statusCode: descriptor.status_code,
        lineNumber: descriptor.line_number,
        columnNumber: descriptor.column_number,
        bubbles: true,
        cancelable: false,
      }});
      return dispatchTarget(document, event);
    }});
  }};
  globalThis.window = globalThis;
  const nativeDialogHostCall = globalThis.__glassNativeDialogCall;
  delete globalThis.__glassNativeDialogCall;
  const parseNativeDialogResult = JSON.parse.bind(JSON);
  const dialogText = (value, field) => {{
    const text = String(value).replace(/\r\n?/g, "\\n");
    if (text.length > {dialog_text_limit}) throw new RangeError("native dialog " + field + " exceeds its limit");
    return text;
  }};
  let dialogQueued = false;
  const callNativeDialog = (dialogType, message, defaultValue) => {{
    if (typeof nativeDialogHostCall !== "function") return null;
    const encoded = nativeDialogHostCall(dialogType, message, defaultValue);
    if (encoded === null || encoded === undefined) return null;
    const result = parseNativeDialogResult(encoded);
    if (!result || typeof result !== "object") throw new TypeError("native dialog response is invalid");
    if (typeof result.error === "string") throw new Error(result.error);
    if (typeof result.accepted !== "boolean") throw new TypeError("native dialog decision is invalid");
    return result;
  }};
  const queueDialog = (dialogType, message, defaultValue) => {{
    if (dialogQueued) throw new Error("native JavaScript dialog is already pending");
    dialogQueued = true;
    pushCommand({{
      kind: "dialog",
      dialog_type: dialogType,
      message: dialogText(message, "message"),
      default_value: defaultValue === undefined ? null : dialogText(defaultValue, "default value"),
    }});
  }};
  const openNativeDialog = (dialogType, message, defaultValue) => {{
    const normalizedMessage = dialogText(message, "message");
    const normalizedDefault = defaultValue === undefined
      ? undefined
      : dialogText(defaultValue, "default value");
    const response = callNativeDialog(dialogType, normalizedMessage, normalizedDefault);
    if (response !== null) return response;
    queueDialog(dialogType, normalizedMessage, normalizedDefault);
    return null;
  }};
  globalThis.alert = function alert(message) {{
    const text = arguments.length === 0 ? "" : dialogText(message, "message");
    openNativeDialog("alert", text, undefined);
    return undefined;
  }};
  globalThis.confirm = function confirm(message = "") {{
    const response = openNativeDialog("confirm", dialogText(message, "message"), undefined);
    return response === null ? false : response.accepted;
  }};
  globalThis.prompt = function prompt(message = "", defaultValue = "") {{
    const response = openNativeDialog(
      "prompt",
      dialogText(message, "message"),
      dialogText(defaultValue, "default value"),
    );
    if (response === null || !response.accepted) return null;
    if (typeof response.promptValue !== "string") throw new TypeError("native prompt response is invalid");
    return response.promptValue;
  }};
  globalThis.__glassHostCommands = commands;
  globalThis.__glassHostCommandBuffer = commands;
  globalThis.document = document;
  if (typeof globalThis.__glassRefreshFontFaces === "function") globalThis.__glassRefreshFontFaces();
  const locationUrl = new URLNative(host.url);
  const navigateLocation = (value, replaceHistory) => {{
    const next = new URLNative(value, locationUrl.href);
    const href = next.href;
    pushCommand({{ kind: "navigate", href, replace: Boolean(replaceHistory) }});
    locationUrl.href = href;
  }};
  const setLocationComponent = (name, value) => {{
    const next = new URLNative(locationUrl.href);
    next[name] = value;
    navigateLocation(next.href, false);
  }};
  const location = {{
    assign(value) {{ navigateLocation(value, false); }},
    replace(value) {{ navigateLocation(value, true); }},
    reload() {{ navigateLocation(locationUrl.href, false); }},
    toString() {{ return locationUrl.href; }},
  }};
  for (const name of ["href", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"]) {{
    Object.defineProperty(location, name, {{
      enumerable: true,
      get: () => locationUrl[name],
      set: value => name === "href"
        ? navigateLocation(value, false)
        : setLocationComponent(name, value),
    }});
  }}
  Object.defineProperty(location, "origin", {{
    enumerable: true,
    get: () => locationUrl.origin,
  }});
  if (typeof globalThis.Location !== "function") {{
    globalThis.Location = function Location() {{
      throw new TypeError("Illegal constructor");
    }};
  }}
  if (typeof globalThis.Window !== "function") {{
    globalThis.Window = function Window() {{
      throw new TypeError("Illegal constructor");
    }};
  }}
  try {{ Object.setPrototypeOf(location, globalThis.Location.prototype); }} catch (_error) {{}}
  Object.freeze(location);
  globalThis.location = location;
  let currentHistoryState = host.history_state === undefined ? null : host.history_state;
  let currentHistoryLength = Math.max(1, Number(host.history_length) || 1);
  let historyScrollRestoration = "auto";
  const cloneHistoryState = (value) => {{
    let encoded;
    try {{ encoded = JSON.stringify(value === undefined ? null : value); }} catch (_error) {{
      throw new DOMExceptionNative("history state could not be cloned", "DataCloneError");
    }}
    if (encoded === undefined) throw new DOMExceptionNative("history state could not be cloned", "DataCloneError");
    if (encoded.length > {history_state_bytes_limit}) throw new RangeError("native history state exceeds its limit");
    try {{ return JSON.parse(encoded); }} catch (_error) {{
      throw new DOMExceptionNative("history state could not be cloned", "DataCloneError");
    }}
  }};
  const historyTarget = (value) => {{
    if (value === undefined || value === null || String(value) === "") return locationUrl.href;
    const next = new URLNative(String(value), locationUrl.href);
    const documentOrigin = String(host.origin || "null");
    const sameOrigin = documentOrigin !== "null"
      ? next.origin === documentOrigin
      : next.protocol === locationUrl.protocol && next.host === locationUrl.host;
    if (!sameOrigin) throw new DOMExceptionNative("history URL must be same-origin", "SecurityError");
    return next.href;
  }};
  const history = {{
    back() {{ pushCommand({{ kind: "historyGo", delta: -1 }}); }},
    forward() {{ pushCommand({{ kind: "historyGo", delta: 1 }}); }},
    go(delta = 0) {{
      const numeric = Number(delta);
      if (!Number.isFinite(numeric)) return;
      const offset = Math.trunc(numeric);
      if (offset < -{history_length_limit} || offset > {history_length_limit}) throw new RangeError("native history delta exceeds its limit");
      pushCommand({{ kind: "historyGo", delta: offset }});
    }},
    pushState(state, _title, url) {{
      const cloned = cloneHistoryState(state);
      const href = historyTarget(url);
      currentHistoryState = cloned;
      currentHistoryLength = Math.min({history_length_limit}, currentHistoryLength + 1);
      locationUrl.href = href;
      pushCommand({{ kind: "historyPushState", href, state: cloned }});
    }},
    replaceState(state, _title, url) {{
      const cloned = cloneHistoryState(state);
      const href = historyTarget(url);
      currentHistoryState = cloned;
      locationUrl.href = href;
      pushCommand({{ kind: "historyReplaceState", href, state: cloned }});
    }},
  }};
  Object.defineProperties(history, {{
    length: {{ enumerable: true, get: () => currentHistoryLength }},
    state: {{ enumerable: true, get: () => cloneHistoryState(currentHistoryState) }},
    scrollRestoration: {{
      enumerable: true,
      get: () => historyScrollRestoration,
      set: value => {{
        const next = String(value);
        if (next !== "auto" && next !== "manual") throw new TypeError("invalid history scrollRestoration");
        historyScrollRestoration = next;
      }},
    }},
  }});
  if (typeof globalThis.History !== "function") {{
    globalThis.History = function History() {{ throw new TypeError("Illegal constructor"); }};
  }}
  try {{ Object.setPrototypeOf(history, globalThis.History.prototype); }} catch (_error) {{}}
  globalThis.history = history;
  const windowProxyCache = globalThis.__glassWindowProxyCache instanceof Map
    ? globalThis.__glassWindowProxyCache
    : new Map();
  globalThis.__glassWindowProxyCache = windowProxyCache;
  const windowProxyStates = globalThis.__glassWindowProxyStates instanceof Map
    ? globalThis.__glassWindowProxyStates
    : new Map();
  globalThis.__glassWindowProxyStates = windowProxyStates;
  const cloneMessageData = (value, options) => glassMessageStructuredClone(value, options);
  const windowMessageOptions = (targetOrigin, transfer) => {{
    let origin = targetOrigin;
    let options = transfer;
    if (targetOrigin && typeof targetOrigin === "object" && !Array.isArray(targetOrigin)) {{
      options = targetOrigin;
      origin = targetOrigin.targetOrigin;
    }}
    return {{
      targetOrigin: origin === undefined ? "/" : String(origin),
      transfer: glassMessageTransferList(options),
    }};
  }};
  const queueWindowMessage = (handle, targetContextId, message, targetOrigin, transfer) => {{
    const options = windowMessageOptions(targetOrigin, transfer);
    const envelope = glassMessageCloneWithTransfers(message, options.transfer);
    const origin = options.targetOrigin;
    if (origin.length === 0 || origin.length > {storage_key_limit}) throw new TypeError("invalid postMessage target origin");
    pushCommand({{
      kind: "postMessage",
      target: String(handle || ""),
      target_origin: origin,
      data: envelope.data,
      target_context_id: targetContextId || null,
      transfer_ports: envelope.transfer_ports,
    }});
  }};
  const currentDocumentOrigin = String(host.origin || "null");
  const originForUrl = (value) => {{
    try {{ return new URLNative(String(value)).origin; }} catch (_error) {{ return "null"; }}
  }};
  const sameOriginForUrl = (value) =>
    currentDocumentOrigin !== "null" && originForUrl(value) === currentDocumentOrigin;
  const crossOriginSecurityError = (property) => new DOMExceptionNative(
    "Permission denied to access property '" + String(property) + "'",
    "SecurityError",
  );
  const installCrossOriginGuards = (proxy, state) => {{
    for (const property of [
      "history", "localStorage", "sessionStorage", "indexedDB", "navigator",
      "performance", "screen", "crypto",
    ]) {{
      if (Object.prototype.hasOwnProperty.call(proxy, property)) continue;
      Object.defineProperty(proxy, property, {{
        configurable: false,
        enumerable: false,
        get() {{
          if (!state.sameOrigin) throw crossOriginSecurityError(property);
          return undefined;
        }},
      }});
    }}
  }};
  const makeWindowProxy = (
    handle,
    targetName,
    targetContextId,
    targetUrl,
    sameOriginOverride = undefined,
  ) => {{
    const cacheKey = String(targetContextId || "") + "\\u0000" + String(handle || "");
    const existing = windowProxyCache.get(cacheKey);
    if (existing) return existing;
    const state = {{
      closed: false,
      targetName: String(targetName || ""),
      targetContextId: String(targetContextId || ""),
      targetLocationHref: String(targetUrl || "about:blank"),
      sameOrigin: sameOriginOverride === undefined
        ? sameOriginForUrl(targetUrl)
        : Boolean(sameOriginOverride),
    }};
    windowProxyStates.set(cacheKey, state);
    const navigateTarget = (value, replaceHistory) => {{
      if (state.closed) return;
      const next = new URLNative(String(value), state.targetLocationHref || locationUrl.href).href;
      state.targetLocationHref = next;
      pushCommand({{
        kind: "navigateWindow",
        target: String(handle || ""),
        target_context_id: state.targetContextId || targetContextId || null,
        href: next,
        replace: Boolean(replaceHistory),
      }});
    }};
    const targetLocation = {{
      get href() {{ return state.targetLocationHref; }},
      set href(value) {{ navigateTarget(value, false); }},
      get protocol() {{ return new URLNative(state.targetLocationHref).protocol; }},
      get host() {{ return new URLNative(state.targetLocationHref).host; }},
      get hostname() {{ return new URLNative(state.targetLocationHref).hostname; }},
      get port() {{ return new URLNative(state.targetLocationHref).port; }},
      get pathname() {{ return new URLNative(state.targetLocationHref).pathname; }},
      get search() {{ return new URLNative(state.targetLocationHref).search; }},
      get hash() {{ return new URLNative(state.targetLocationHref).hash; }},
      get origin() {{ return new URLNative(state.targetLocationHref).origin; }},
      assign(value) {{ navigateTarget(value, false); }},
      replace(value) {{ navigateTarget(value, true); }},
      reload() {{ navigateTarget(state.targetLocationHref, false); }},
      toString() {{ return state.targetLocationHref; }},
    }};
    if (typeof globalThis.Location === "function" && globalThis.Location.prototype) {{
      try {{ Object.setPrototypeOf(targetLocation, globalThis.Location.prototype); }} catch (_error) {{}}
    }}
    Object.freeze(targetLocation);
    const eventOwner = "window:" + cacheKey;
    const proxy = {{
      get name() {{ return state.targetName; }},
      get closed() {{ return state.closed; }},
      get location() {{ return targetLocation; }},
      addEventListener(type, callback, options) {{
        addListener(eventOwner, type, callback, options);
      }},
      removeEventListener(type, callback, options) {{
        removeListener(eventOwner, type, callback, options);
      }},
      dispatchEvent(event) {{
        return dispatchTarget(this, event);
      }},
      close() {{
        if (state.closed) return;
        state.closed = true;
        pushCommand({{
          kind: "closeWindow",
          target: String(handle || ""),
          target_context_id: state.targetContextId || targetContextId || null,
        }});
      }},
      postMessage(message, targetOrigin = "/", transfer) {{
        queueWindowMessage(handle, targetContextId, message, targetOrigin, transfer);
      }},
      toJSON() {{
        return {{ name: this.name, closed: this.closed }};
      }},
    }};
    Object.defineProperty(proxy, "__glassEventOwner", {{
      enumerable: false,
      configurable: false,
      value: eventOwner,
    }});
    if (typeof globalThis.Window === "function" && globalThis.Window.prototype) {{
      try {{ Object.setPrototypeOf(proxy, globalThis.Window.prototype); }} catch (_error) {{}}
    }}
    installCrossOriginGuards(proxy, state);
    windowProxyCache.set(cacheKey, proxy);
    return proxy;
  }};
  globalThis.__glassSyncWindowProxies = (updates) => {{
    if (!Array.isArray(updates)) return;
    for (const update of updates) {{
      if (!update || typeof update !== "object") continue;
      const cacheKey = String(update.cache_key || "");
      const targetContextId = String(update.target_context_id || "");
      for (const [candidateKey, proxy] of windowProxyCache.entries()) {{
        const state = windowProxyStates.get(candidateKey);
        if (!state) continue;
        if (candidateKey !== cacheKey &&
            (!targetContextId || state.targetContextId !== targetContextId)) continue;
        state.targetContextId = targetContextId || state.targetContextId;
        if (typeof update.href === "string" && update.href.length > 0)
          state.targetLocationHref = update.href;
        if (typeof update.name === "string") state.targetName = update.name;
        state.sameOrigin = sameOriginForUrl(state.targetLocationHref);
        state.closed = Boolean(update.closed);
      }}
    }}
  }};
  let windowName = typeof globalThis.__glassWindowName === "string"
    ? globalThis.__glassWindowName
    : String(host.window_name || "");
  globalThis.__glassWindowName = windowName;
  const setWindowName = (value) => {{
    const next = String(value);
    if (next.length > {window_name_bytes_limit}) throw new RangeError("native window.name exceeds its limit");
    if ([...next].some((character) => {{
      const code = character.codePointAt(0);
      return code < 0x20 || code === 0x7f;
    }})) throw new TypeError("native window.name contains a control character");
    windowName = next;
    globalThis.__glassWindowName = next;
    pushCommand({{ kind: "setWindowName", value: next }});
  }};
  Object.defineProperty(globalThis, "name", {{
    configurable: true,
    enumerable: true,
    get: () => windowName,
    set: setWindowName,
  }});
  const openerProxy = host.opener_context_id
    ? makeWindowProxy("opener:" + String(host.opener_context_id), String(host.opener_window_name || ""), String(host.opener_context_id), String(host.opener_url || "about:blank"))
    : null;
  Object.defineProperty(globalThis, "opener", {{
    configurable: true,
    enumerable: true,
    get: () => openerProxy,
  }});
  globalThis.postMessage = (message, targetOrigin = "/", transfer) =>
    queueWindowMessage("", host.context_id, message, targetOrigin, transfer);
  globalThis.__glassDispatchMessage = (descriptor) => {{
    const event = createEvent("message", {{ bubbles: false, cancelable: false }});
    if (descriptor && Array.isArray(descriptor.object_urls)) {{
      globalThis.__glassInstallObjectUrlTransfers(descriptor.object_urls);
    }}
    const envelope = glassMessageDecodeEnvelope(descriptor || {{}});
    event.data = envelope.data;
    event.origin = String(descriptor && descriptor.source_origin || "null");
    event.source = descriptor && descriptor.source_context_id
      ? makeWindowProxy("source:" + String(descriptor.source_context_id), "", String(descriptor.source_context_id), "about:blank")
      : null;
    event.ports = envelope.ports;
    return dispatchTarget(globalThis, event);
  }};
  const workers = globalThis.__glassWorkers instanceof Map
    ? globalThis.__glassWorkers
    : new Map();
  const sharedWorkers = globalThis.__glassSharedWorkers instanceof Map
    ? globalThis.__glassSharedWorkers
    : new Map();
  let nextWorkerId = Number.isSafeInteger(globalThis.__glassNextWorkerId)
    ? globalThis.__glassNextWorkerId
    : 1;
  const workerDispatch = (worker, type, event) => {{
    const handler = worker["on" + type];
    if (typeof handler === "function") {{
      let result;
      try {{ result = handler.call(worker, event); }} catch (_error) {{}}
      if (type === "error" && event.cancelable === true && result === false)
        event.defaultPrevented = true;
    }}
    const listeners = worker.__glassWorkerListeners[type] || [];
    for (const listener of listeners.slice()) {{
      try {{ listener.call(worker, event); }} catch (_error) {{}}
    }}
  }};
  globalThis.__glassWorkers = workers;
  globalThis.__glassSharedWorkers = sharedWorkers;
  globalThis.__glassNextWorkerId = nextWorkerId;
  globalThis.__glassDispatchWorkerMessage = (workerId, payload) => {{
    const dedicatedWorker = workers.get(Number(workerId));
    const worker = dedicatedWorker || sharedWorkers.get(Number(workerId));
    if (!worker || worker.__glassTerminated || !payload || typeof payload !== "object") return null;
    if (payload.error !== undefined && payload.error !== null) {{
      const ErrorEventConstructor = globalThis.__glassErrorEventConstructor || globalThis.ErrorEvent;
      const errorEvent = typeof ErrorEventConstructor === "function"
        ? new ErrorEventConstructor("error", {{
            message: String(payload.error),
            filename: worker.url,
            lineno: 0,
            colno: 0,
            error: null,
            cancelable: true,
          }})
        : Object.assign(createEvent("error", {{ cancelable: true }}), {{
            message: String(payload.error), filename: worker.url,
            lineno: 0, colno: 0, error: null,
          }});
      errorEvent.target = worker;
      errorEvent.currentTarget = worker;
      errorEvent.eventPhase = 2;
      workerDispatch(worker, "error", errorEvent);
      errorEvent.currentTarget = null;
      errorEvent.eventPhase = 0;
    }} else {{
      let envelope;
      try {{
        envelope = glassMessageDecodeEnvelope(payload);
      }} catch (_error) {{
        if (!dedicatedWorker) throw _error;
        const MessageEventConstructor = globalThis.__glassWorkerMessageEventConstructor
          || globalThis.MessageEvent;
        const messageErrorEvent = new MessageEventConstructor("messageerror", {{
          data: null,
          ports: [],
          origin: String(payload.origin || ""),
          source: null,
          bubbles: false,
          cancelable: false,
        }});
        messageErrorEvent.target = worker;
        messageErrorEvent.currentTarget = worker;
        messageErrorEvent.eventPhase = 2;
        workerDispatch(worker, "messageerror", messageErrorEvent);
        messageErrorEvent.currentTarget = null;
        messageErrorEvent.eventPhase = 0;
        return null;
      }}
      if (Array.isArray(payload.object_urls))
        globalThis.__glassInstallObjectUrlTransfers(payload.object_urls);
      const MessageEventConstructor = globalThis.__glassWorkerMessageEventConstructor
        || globalThis.MessageEvent;
      const messageEvent = new MessageEventConstructor("message", {{
        data: envelope.data,
        ports: envelope.ports,
        origin: String(payload.origin || ""),
        source: null,
        bubbles: false,
        cancelable: false,
      }});
      messageEvent.target = worker;
      messageEvent.currentTarget = worker;
      messageEvent.eventPhase = 2;
      workerDispatch(worker, "message", messageEvent);
      messageEvent.currentTarget = null;
      messageEvent.eventPhase = 0;
    }}
    return null;
  }};
  const WorkerNative = function(input, options) {{
    if (!(this instanceof WorkerNative)) throw new TypeError("native Worker requires new");
    if (options !== undefined && (options === null || typeof options !== "object"))
      throw new TypeError("native Worker options must be an object");
    const workerType = options && options.type !== undefined
      ? String(options.type).toLowerCase()
      : "classic";
    if (workerType !== "classic" && workerType !== "module")
      throw new TypeError("native Worker type must be classic or module");
    const source = input && input.__glassUrl === true ? input.href : input;
    const resolved = new URLNative(String(source), locationUrl.href);
    const rootedFile = resolved.protocol === "file:" && locationUrl.protocol === "file:" && !resolved.host;
    if (!["http:", "https:", "fixture:", "file:"].includes(resolved.protocol)
        || resolved.username || resolved.password
        || (resolved.protocol === "file:" ? !rootedFile : !resolved.host))
      throw new SyntaxError("native Worker URL must use HTTP(S), a registered fixture, or the owning rooted file");
    const workerId = nextWorkerId;
    nextWorkerId += 1;
    globalThis.__glassNextWorkerId = nextWorkerId;
    this.url = resolved.href;
    this.onmessage = null;
    this.onmessageerror = null;
    this.onerror = null;
    this.__glassWorkerId = workerId;
    this.__glassTerminated = false;
    this.__glassWorkerListeners = {{ message: [], messageerror: [], error: [] }};
    workers.set(workerId, this);
    pushCommand({{ kind: "workerCreate", worker_id: workerId, href: resolved.href, worker_type: workerType, referrer_policy: documentReferrerPolicy.value }});
  }};
  WorkerNative.prototype.addEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassWorkerListeners[name] || typeof listener !== "function") return;
    if (!this.__glassWorkerListeners[name].includes(listener)) this.__glassWorkerListeners[name].push(listener);
  }};
  WorkerNative.prototype.removeEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassWorkerListeners[name]) return;
    this.__glassWorkerListeners[name] = this.__glassWorkerListeners[name].filter((candidate) => candidate !== listener);
  }};
  WorkerNative.prototype.postMessage = function(message, options) {{
    if (this.__glassTerminated) return;
    const envelope = glassMessageCloneWithTransfers(message, glassMessageTransferList(options));
    pushCommand({{ kind: "workerPostMessage", worker_id: this.__glassWorkerId, data: envelope.data, transfer_ports: envelope.transfer_ports }});
  }};
  WorkerNative.prototype.terminate = function() {{
    if (this.__glassTerminated) return;
    this.__glassTerminated = true;
    workers.delete(this.__glassWorkerId);
    pushCommand({{ kind: "workerTerminate", worker_id: this.__glassWorkerId }});
  }};
  WorkerNative.prototype.dispatchEvent = function(event) {{
    if (!event || !event.type) throw new TypeError("native Worker event is invalid");
    workerDispatch(this, String(event.type), event);
    return true;
  }};
  globalThis.Worker = WorkerNative;
  const SharedWorkerNative = function(input, options) {{
    if (!(this instanceof SharedWorkerNative)) throw new TypeError("native SharedWorker requires new");
    if (options !== undefined && options !== null
        && typeof options !== "object" && typeof options !== "string")
      throw new TypeError("native SharedWorker options must be an object or string");
    const workerType = typeof options === "object" && options && options.type !== undefined
      ? String(options.type)
      : "classic";
    if (workerType !== "classic" && workerType !== "module")
      throw new TypeError("native SharedWorker type must be classic or module");
    const credentials = typeof options === "object" && options && options.credentials !== undefined
      ? String(options.credentials)
      : "same-origin";
    if (!["omit", "same-origin", "include"].includes(credentials))
      throw new TypeError("native SharedWorker credentials must be omit, same-origin, or include");
    const extendedLifetime = typeof options === "object" && options && options.extendedLifetime !== undefined
      ? Boolean(options.extendedLifetime)
      : false;
    const workerName = typeof options === "string"
      ? options
      : options && options.name !== undefined ? String(options.name) : "";
    if (workerName.length > {storage_key_limit}
        || [...workerName].some(character => {{
          const code = character.codePointAt(0);
          return code < 0x20 || code === 0x7f;
        }}))
      throw new TypeError("native SharedWorker name is invalid");
    const source = input && input.__glassUrl === true ? input.href : input;
    const resolved = new URLNative(String(source), locationUrl.href);
    const rootedFile = resolved.protocol === "file:" && locationUrl.protocol === "file:" && !resolved.host;
    if (!["http:", "https:", "fixture:", "file:"].includes(resolved.protocol)
        || resolved.username || resolved.password
        || (resolved.protocol === "file:" ? !rootedFile : !resolved.host))
      throw new DOMExceptionNative("native SharedWorker URL must use a same-origin HTTP(S), fixture, or rooted file resource", "SecurityError");
    if (resolved.protocol !== "fixture:" && !rootedFile && resolved.origin !== locationUrl.origin)
      throw new DOMExceptionNative("native SharedWorker URL must be same-origin", "SecurityError");
    if (sharedWorkers.size >= {max_timers})
      throw new DOMExceptionNative("native SharedWorker connection limit exceeded", "QuotaExceededError");
    const connectionId = nextWorkerId;
    nextWorkerId += 1;
    globalThis.__glassNextWorkerId = nextWorkerId;
    const channel = new MessageChannel();
    const preparedTransfer = glassMessageTransferDescriptor(channel.port2);
    glassMessageCommitTransfer(preparedTransfer);
    this.url = resolved.href;
    this.name = workerName;
    this.onerror = null;
    this.__glassWorkerId = connectionId;
    this.__glassTerminated = false;
    this.__glassWorkerListeners = {{ message: [], error: [] }};
    Object.defineProperty(this, "port", {{
      configurable: false,
      enumerable: true,
      writable: false,
      value: channel.port1,
    }});
    sharedWorkers.set(connectionId, this);
    pushCommand({{
      kind: "sharedWorkerCreate",
      connection_id: connectionId,
      href: resolved.href,
      name: workerName,
      worker_type: workerType,
      credentials,
      extended_lifetime: extendedLifetime,
      referrer_policy: documentReferrerPolicy.value,
      transfer_port: preparedTransfer.descriptor,
    }});
  }};
  SharedWorkerNative.prototype.addEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassWorkerListeners[name] || typeof listener !== "function") return;
    if (!this.__glassWorkerListeners[name].includes(listener)) this.__glassWorkerListeners[name].push(listener);
  }};
  SharedWorkerNative.prototype.removeEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassWorkerListeners[name]) return;
    this.__glassWorkerListeners[name] = this.__glassWorkerListeners[name].filter((candidate) => candidate !== listener);
  }};
  SharedWorkerNative.prototype.dispatchEvent = function(event) {{
    if (!event || !event.type) throw new TypeError("native SharedWorker event is invalid");
    workerDispatch(this, String(event.type), event);
    return true;
  }};
  globalThis.SharedWorker = SharedWorkerNative;
  globalThis.open = function open(value, target) {{
    const rawTarget = target === undefined || target === null ? "_blank" : String(target);
    const normalizedTarget = rawTarget || "_blank";
    const lowerTarget = normalizedTarget.toLowerCase();
    const href = value === undefined || value === null || String(value) === ""
      ? "about:blank"
      : new URLNative(String(value), locationUrl.href).href;
    if (["_self", "_parent", "_top", "_unfencedtop"].includes(lowerTarget)) {{
      navigateLocation(href, false);
      return makeWindowProxy("", "", host.context_id, href);
    }}
    const windowHandles = globalThis.__glassWindowHandles instanceof Map
      ? globalThis.__glassWindowHandles
      : new Map();
    globalThis.__glassWindowHandles = windowHandles;
    let handle;
    if (normalizedTarget === "_blank") {{
      const next = Number.isSafeInteger(globalThis.__glassNextWindowHandle)
        ? globalThis.__glassNextWindowHandle
        : 1;
      handle = "glass-window-" + next;
      globalThis.__glassNextWindowHandle = next + 1;
    }} else {{
      handle = windowHandles.get(normalizedTarget);
      if (!handle) {{
        const next = Number.isSafeInteger(globalThis.__glassNextWindowHandle)
          ? globalThis.__glassNextWindowHandle
          : 1;
        handle = "glass-window-" + next;
        globalThis.__glassNextWindowHandle = next + 1;
        windowHandles.set(normalizedTarget, handle);
      }}
    }}
    pushCommand({{ kind: "openWindow", href, target: normalizedTarget, handle }});
    return makeWindowProxy(
      handle,
      normalizedTarget === "_blank" ? "" : normalizedTarget,
      null,
      href,
    );
  }};
  globalThis.innerWidth = bootstrapParams.width;
  globalThis.innerHeight = bootstrapParams.height;
  const nativeMediaDimension = (value) => {{
    const match = /^(-?\d+(?:\.\d+)?)px$/i.exec(String(value).trim());
    return match ? Number(match[1]) : null;
  }};
  const nativeMediaQueryMatches = (query) => {{
    let source = String(query).trim().toLowerCase();
    if (!source) return false;
    let negate = false;
    if (source.startsWith("not ")) {{
      negate = true;
      source = source.slice(4).trim();
    }}
    if (source.startsWith("only ")) source = source.slice(5).trim();
    const clauses = source.split(/\s+and\s+/).map((value) => value.trim()).filter(Boolean);
    let matches = true;
    for (const clause of clauses) {{
      if (clause === "all" || clause === "screen") continue;
      const feature = /^\(([-a-z]+)\s*:\s*([^)]*)\)$/.exec(clause);
      if (!feature) {{ matches = false; break; }}
      const name = feature[1];
      const value = feature[2].trim();
      if (name === "min-width") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerWidth >= dimension; }}
      else if (name === "max-width") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerWidth <= dimension; }}
      else if (name === "width") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerWidth === dimension; }}
      else if (name === "min-height") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerHeight >= dimension; }}
      else if (name === "max-height") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerHeight <= dimension; }}
      else if (name === "height") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerHeight === dimension; }}
      else if (name === "orientation") matches = matches && value === (innerWidth >= innerHeight ? "landscape" : "portrait");
      else if (name === "prefers-color-scheme") matches = false;
      else matches = false;
    }}
    return negate ? !matches : matches;
  }};
  globalThis.matchMedia = (query) => {{
    const media = String(query);
    const listeners = new Set();
    const result = {{
      media,
      get matches() {{ return nativeMediaQueryMatches(media); }},
      onchange: null,
      addListener(callback) {{ if (typeof callback === "function") listeners.add(callback); }},
      removeListener(callback) {{ listeners.delete(callback); }},
      addEventListener(type, callback) {{ if (type === "change" && typeof callback === "function") listeners.add(callback); }},
      removeEventListener(type, callback) {{ if (type === "change") listeners.delete(callback); }},
      dispatchEvent(event) {{
        for (const callback of listeners) callback.call(result, event);
        if (typeof result.onchange === "function") result.onchange.call(result, event);
        return true;
      }},
    }};
    return result;
  }};
  const performanceNative = globalThis.__glassPerformance instanceof Object
    ? globalThis.__glassPerformance
    : {{}};
  performanceNative.now = () => Number(host.now_ms) || 0;
  performanceNative.timeOrigin = Date.now() - (Number(host.now_ms) || 0);
  globalThis.__glassPerformance = performanceNative;
  globalThis.performance = performanceNative;
  globalThis.scrollX = Number(state.scrollX || 0);
  globalThis.scrollY = Number(state.scrollY || 0);
  globalThis.pageXOffset = globalThis.scrollX;
  globalThis.pageYOffset = globalThis.scrollY;
  const applyWindowScroll = (leftCandidate, topCandidate) => {{
    const leftValue = Number(leftCandidate);
    const topValue = Number(topCandidate);
    const left = leftCandidate === undefined
      ? (Number(globalThis.scrollX) || 0)
      : (Number.isFinite(leftValue) ? Math.trunc(leftValue) : 0);
    const top = topCandidate === undefined
      ? (Number(globalThis.scrollY) || 0)
      : (Number.isFinite(topValue) ? Math.trunc(topValue) : 0);
    const nextLeft = Math.max(0, Math.min(
      Math.max(0, (Number(state.scrollWidth) || globalThis.innerWidth) - globalThis.innerWidth),
      left,
    ));
    const nextTop = Math.max(0, Math.min(
      Math.max(0, (Number(state.scrollHeight) || globalThis.innerHeight) - globalThis.innerHeight),
      top,
    ));
    const changed = nextLeft !== globalThis.scrollX || nextTop !== globalThis.scrollY;
    globalThis.scrollX = nextLeft;
    globalThis.scrollY = nextTop;
    globalThis.pageXOffset = nextLeft;
    globalThis.pageYOffset = nextTop;
    state.scrollX = nextLeft;
    state.scrollY = nextTop;
    if (documentElement && typeof documentElement.__glassSetScrollState === "function") {{
      documentElement.__glassSetScrollState(nextLeft, nextTop);
    }}
    if (changed) pushCommand({{ kind: "scrollTo", node_index: 0, left: nextLeft, top: nextTop }});
  }};
  globalThis.scrollTo = (leftOrOptions = 0, top = 0) => {{
    if (leftOrOptions && typeof leftOrOptions === "object") {{
      applyWindowScroll(leftOrOptions.left, leftOrOptions.top);
    }} else {{
      applyWindowScroll(leftOrOptions, top);
    }}
  }};
  globalThis.scrollBy = (leftOrOptions = 0, top = 0) => {{
    if (leftOrOptions && typeof leftOrOptions === "object") {{
      applyWindowScroll(globalThis.scrollX + (Number(leftOrOptions.left) || 0), globalThis.scrollY + (Number(leftOrOptions.top) || 0));
    }} else {{
      applyWindowScroll(globalThis.scrollX + (Number(leftOrOptions) || 0), globalThis.scrollY + (Number(top) || 0));
    }}
  }};
  globalThis.scroll = globalThis.scrollTo;
  const nativeNavigator = globalThis.navigator && typeof globalThis.navigator === "object"
    ? globalThis.navigator
    : {{}};
  const navigatorLanguages = String(host.accept_language || "en-US")
    .split(",")
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  const setNavigatorProperty = (name, value) => {{
    try {{ nativeNavigator[name] = value; }} catch (_) {{
      try {{ Object.defineProperty(nativeNavigator, name, {{ value, configurable: true }}); }} catch (_) {{}}
    }}
  }};
  setNavigatorProperty("userAgent", String(host.user_agent || "GlassNative"));
  setNavigatorProperty("platform", String(host.platform || "GlassNative"));
  setNavigatorProperty("language", navigatorLanguages[0] || "en-US");
  setNavigatorProperty("languages", navigatorLanguages.length > 0 ? navigatorLanguages : ["en-US"]);
  const locationOverride = host.geolocation && typeof host.geolocation === "object"
    ? host.geolocation
    : null;
  const geolocation = {{
    getCurrentPosition(success, error) {{
      if (locationOverride && typeof success === "function") {{
        success({{ coords: {{
          latitude: Number(locationOverride.latitude),
          longitude: Number(locationOverride.longitude),
          accuracy: Number(locationOverride.accuracy),
          altitude: null, altitudeAccuracy: null, heading: null, speed: null,
        }}, timestamp: Date.now() }});
      }} else if (typeof error === "function") {{
        error({{ code: 1, message: "User denied Geolocation" }});
      }}
    }},
    watchPosition(success, error) {{
      this.getCurrentPosition(success, error);
      return 1;
    }},
    clearWatch(_) {{}},
  }};
  setNavigatorProperty("geolocation", geolocation);
  globalThis.__glassTimezoneId = host.timezone_id ? String(host.timezone_id) : "UTC";
  try {{ globalThis.navigator = nativeNavigator; }} catch (_) {{}}
  const pageCryptoPool = globalThis.__glassPageCryptoPool instanceof Array
    ? globalThis.__glassPageCryptoPool
    : [];
  const pageCryptoRandomSource = typeof globalThis.__glassNativeRandomBytes === "function"
    ? globalThis.__glassNativeRandomBytes
    : null;
  let pageCryptoOffset = Number.isSafeInteger(globalThis.__glassPageCryptoOffset)
    ? globalThis.__glassPageCryptoOffset
    : 0;
  if (pageCryptoOffset > 0) {{
    pageCryptoPool.splice(0, pageCryptoOffset);
    pageCryptoOffset = 0;
  }}
  pageCryptoPool.push(...initialPageCryptoBytes);
  if (pageCryptoPool.length > {page_crypto_pool_limit})
    pageCryptoPool.splice(0, pageCryptoPool.length - {page_crypto_pool_limit});
  const pageCryptoTake = (count) => {{
    if (!Number.isSafeInteger(count) || count < 0 || count > {page_crypto_values_limit})
      throw new RangeError("native page crypto value is outside the bounded range");
    if (pageCryptoOffset + count > pageCryptoPool.length) {{
      if (pageCryptoOffset > 0) {{
        pageCryptoPool.splice(0, pageCryptoOffset);
        pageCryptoOffset = 0;
      }}
      const needed = count - pageCryptoPool.length;
      if (needed > 0 && pageCryptoRandomSource) {{
        const refill = pageCryptoRandomSource(needed);
        if (!Array.isArray(refill) || refill.length !== needed) {{
          const error = new Error("native page crypto refill failed");
          error.name = "OperationError";
          throw error;
        }}
        for (const byte of refill) pageCryptoPool.push(byte);
      }}
    }}
    if (pageCryptoOffset + count > pageCryptoPool.length) {{
      const Constructor = globalThis.DOMException;
      if (typeof Constructor === "function")
        throw new Constructor("native page crypto pool exhausted", "QuotaExceededError");
      const error = new Error("native page crypto pool exhausted");
      error.name = "QuotaExceededError";
      throw error;
    }}
    const bytes = pageCryptoPool.slice(pageCryptoOffset, pageCryptoOffset + count);
    pageCryptoOffset += count;
    globalThis.__glassPageCryptoOffset = pageCryptoOffset;
    return bytes;
  }};
  const pageCryptoIntegerArray = (value) => {{
    const tag = Object.prototype.toString.call(value);
    return ["[object Int8Array]", "[object Uint8Array]", "[object Uint8ClampedArray]",
      "[object Int16Array]", "[object Uint16Array]", "[object Int32Array]",
      "[object Uint32Array]", "[object BigInt64Array]", "[object BigUint64Array]"].includes(tag);
  }};
  const pageCrypto = globalThis.__glassPageCryptoObject instanceof Object
    ? globalThis.__glassPageCryptoObject
    : (globalThis.crypto instanceof Object ? globalThis.crypto : {{}});
  pageCrypto.getRandomValues = (value) => {{
    if (!pageCryptoIntegerArray(value))
      throw new TypeError("native page crypto requires an integer typed array");
    if (value.byteLength > {page_crypto_values_limit}) {{
      const Constructor = globalThis.DOMException;
      if (typeof Constructor === "function")
        throw new Constructor("native page crypto value is too large", "QuotaExceededError");
      const error = new Error("native page crypto value is too large");
      error.name = "QuotaExceededError";
      throw error;
    }}
    new Uint8Array(value.buffer, value.byteOffset, value.byteLength).set(pageCryptoTake(value.byteLength));
    return value;
  }};
  pageCrypto.randomUUID = () => {{
    const bytes = pageCryptoTake(16);
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    const hex = bytes.map(byte => byte.toString(16).padStart(2, "0")).join("");
    return hex.slice(0, 8) + "-" + hex.slice(8, 12) + "-" + hex.slice(12, 16)
      + "-" + hex.slice(16, 20) + "-" + hex.slice(20);
  }};
  globalThis.__glassPageCryptoPool = pageCryptoPool;
  globalThis.__glassPageCryptoObject = pageCrypto;
  const pageCryptoDigestSource = typeof globalThis.__glassNativeCryptoDigest === "function"
    ? globalThis.__glassNativeCryptoDigest
    : null;
  const pageCryptoBufferInput = (input) => {{
    if (input instanceof ArrayBuffer) return Array.from(new Uint8Array(input));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(input))
      return Array.from(new Uint8Array(input.buffer, input.byteOffset, input.byteLength));
    throw new TypeError("native crypto digest data must be an ArrayBuffer or view");
  }};
  const pageCryptoDigestError = (message, name) => {{
    const Constructor = globalThis.DOMException;
    if (typeof Constructor === "function") return new Constructor(message, name);
    const error = new Error(message);
    error.name = name;
    return error;
  }};
  const pageSubtle = globalThis.__glassPageSubtleCrypto instanceof Object
    ? globalThis.__glassPageSubtleCrypto
    : {{}};
  let pageCryptoKeyStore = pageSubtle.__glassNativeKeyStore;
  if (!(pageCryptoKeyStore instanceof WeakMap)) {{
    pageCryptoKeyStore = new WeakMap();
    Object.defineProperty(pageSubtle, "__glassNativeKeyStore", {{
      configurable: false,
      enumerable: false,
      value: pageCryptoKeyStore,
    }});
  }}
  const PageCryptoKeyNative = typeof globalThis.__glassPageCryptoKeyConstructor === "function"
    ? globalThis.__glassPageCryptoKeyConstructor
    : function CryptoKey() {{ throw new TypeError("Illegal constructor"); }};
  PageCryptoKeyNative.prototype.constructor = PageCryptoKeyNative;
  globalThis.__glassPageCryptoKeyConstructor = PageCryptoKeyNative;
  globalThis.CryptoKey = PageCryptoKeyNative;
  const pageCryptoHashName = (value) => {{
    const candidate = value && typeof value === "object" ? value.name : value;
    const name = String(candidate || "").toUpperCase();
    if (!["SHA-1", "SHA-256", "SHA-384", "SHA-512"].includes(name))
      throw pageCryptoDigestError("native crypto hash is unsupported", "NotSupportedError");
    return name;
  }};
  const pageCryptoHmacHash = (algorithm) => {{
    const name = algorithm && typeof algorithm === "object" ? algorithm.name : algorithm;
    if (String(name || "").toUpperCase() !== "HMAC")
      throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
    if (!algorithm || typeof algorithm !== "object")
      throw pageCryptoDigestError("native HMAC hash is required", "DataError");
    return pageCryptoHashName(algorithm.hash);
  }};
  const pageCryptoUsages = (values) => {{
    if (!Array.isArray(values)) throw new TypeError("native crypto usages must be an array");
    const usages = [];
    for (const value of values) {{
      const usage = String(value);
      if (!["sign", "verify"].includes(usage) || usages.includes(usage))
        throw new SyntaxError("native crypto usage is invalid or duplicated");
      usages.push(usage);
    }}
    return usages;
  }};
  const pageCryptoKeyState = (key) => {{
    const state = pageCryptoKeyStore.get(key);
    if (!state) throw pageCryptoDigestError("native CryptoKey is not recognized", "InvalidAccessError");
    return state;
  }};
  const pageCryptoMakeKey = (bytes, hashName, extractable, usages) => {{
    const key = Object.create(PageCryptoKeyNative.prototype);
    const algorithm = Object.freeze({{
      name: "HMAC",
      hash: Object.freeze({{ name: hashName }}),
      length: bytes.length * 8,
    }});
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {{
      type: {{ configurable: false, enumerable: true, value: "secret" }},
      extractable: {{ configurable: false, enumerable: true, value: Boolean(extractable) }},
      algorithm: {{ configurable: false, enumerable: true, value: algorithm }},
      usages: {{ configurable: false, enumerable: true, value: normalizedUsages }},
    }});
    pageCryptoKeyStore.set(key, {{
      kind: "HMAC",
      bytes: bytes.slice(),
      extractable: Boolean(extractable),
      hashName,
      usages: normalizedUsages,
    }});
    return Object.freeze(key);
  }};
  const pageCryptoOperationHash = (algorithm, state) => {{
    const name = algorithm && typeof algorithm === "object" ? algorithm.name : algorithm;
    if (String(name || "").toUpperCase() !== "HMAC")
      throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
    if (algorithm && typeof algorithm === "object" && algorithm.hash !== undefined
        && pageCryptoHashName(algorithm.hash) !== state.hashName)
      throw pageCryptoDigestError("native CryptoKey hash does not match", "InvalidAccessError");
    return state.hashName;
  }};
  const pageCryptoHmacSource = typeof globalThis.__glassNativeCryptoHmac === "function"
    ? globalThis.__glassNativeCryptoHmac
    : null;
  const pageCryptoHmacBytes = (state, data) => {{
    if (!pageCryptoHmacSource)
      throw pageCryptoDigestError("native crypto HMAC is unavailable", "OperationError");
    const encoded = pageCryptoHmacSource(
      state.hashName,
      encodeBase64(state.bytes, {native_form_body_bytes}),
      encodeBase64(data, {native_form_body_bytes}),
    );
    return decodeBase64(encoded, {native_form_body_bytes});
  }};
  const pageCryptoEd25519Source = typeof globalThis.__glassNativeCryptoEd25519 === "function"
    ? globalThis.__glassNativeCryptoEd25519
    : null;
  const pageCryptoEd25519Algorithm = (algorithm) => {{
    const name = algorithm && typeof algorithm === "object" ? algorithm.name : algorithm;
    if (String(name || "").toUpperCase() !== "ED25519")
      throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
  }};
  const pageCryptoEd25519Usages = (values) => {{
    if (!Array.isArray(values)) throw new TypeError("native Ed25519 usages must be an array");
    const usages = [];
    for (const value of values) {{
      const usage = String(value);
      if (!["sign", "verify"].includes(usage) || usages.includes(usage))
        throw new SyntaxError("native Ed25519 usage is invalid or duplicated");
      usages.push(usage);
    }}
    return usages;
  }};
  const pageCryptoEd25519PublicKey = (privateBytes) => {{
    if (!pageCryptoEd25519Source)
      throw pageCryptoDigestError("native Ed25519 is unavailable", "OperationError");
    const encoded = pageCryptoEd25519Source(
      "public",
      encodeBase64(privateBytes, {native_form_body_bytes}),
      "",
      "",
    );
    const bytes = decodeBase64(encoded, {native_form_body_bytes});
    if (bytes.length !== 32)
      throw pageCryptoDigestError("native Ed25519 public key is invalid", "OperationError");
    return bytes;
  }};
  const pageCryptoEd25519Bytes = (operation, state, data, signature = []) => {{
    if (!pageCryptoEd25519Source)
      throw pageCryptoDigestError("native Ed25519 is unavailable", "OperationError");
    const encoded = pageCryptoEd25519Source(
      operation,
      encodeBase64(state.bytes, {native_form_body_bytes}),
      encodeBase64(data, {native_form_body_bytes}),
      encodeBase64(signature, {native_form_body_bytes}),
    );
    if (operation === "verify") return encoded === "1";
    return decodeBase64(encoded, {native_form_body_bytes});
  }};
  const pageCryptoMakeEd25519PublicKey = (bytes, extractable, usages) => {{
    if (!pageCryptoEd25519Source)
      throw pageCryptoDigestError("native Ed25519 is unavailable", "OperationError");
    try {{
      if (pageCryptoEd25519Source(
        "validate",
        encodeBase64(bytes, {native_form_body_bytes}),
        "",
        "",
      ) !== "1") throw new Error("invalid public key");
    }} catch (_error) {{
      throw pageCryptoDigestError("native Ed25519 public key is invalid", "DataError");
    }}
    const key = Object.create(PageCryptoKeyNative.prototype);
    const algorithm = Object.freeze({{ name: "Ed25519" }});
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {{
      type: {{ configurable: false, enumerable: true, value: "public" }},
      extractable: {{ configurable: false, enumerable: true, value: Boolean(extractable) }},
      algorithm: {{ configurable: false, enumerable: true, value: algorithm }},
      usages: {{ configurable: false, enumerable: true, value: normalizedUsages }},
    }});
    pageCryptoKeyStore.set(key, {{
      kind: "ED25519-PUBLIC",
      bytes: bytes.slice(),
      extractable: Boolean(extractable),
      usages: normalizedUsages,
    }});
    return Object.freeze(key);
  }};
  const pageCryptoMakeEd25519PrivateKey = (privateBytes, publicBytes, extractable, usages) => {{
    const key = Object.create(PageCryptoKeyNative.prototype);
    const algorithm = Object.freeze({{ name: "Ed25519" }});
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {{
      type: {{ configurable: false, enumerable: true, value: "private" }},
      extractable: {{ configurable: false, enumerable: true, value: Boolean(extractable) }},
      algorithm: {{ configurable: false, enumerable: true, value: algorithm }},
      usages: {{ configurable: false, enumerable: true, value: normalizedUsages }},
    }});
    pageCryptoKeyStore.set(key, {{
      kind: "ED25519-PRIVATE",
      bytes: privateBytes.slice(),
      publicBytes: publicBytes.slice(),
      extractable: Boolean(extractable),
      usages: normalizedUsages,
    }});
    return Object.freeze(key);
  }};
  const pageCryptoDeriveSource = typeof globalThis.__glassNativeCryptoDerive === "function"
    ? globalThis.__glassNativeCryptoDerive
    : null;
  const pageCryptoAesSource = typeof globalThis.__glassNativeCryptoAesGcm === "function"
    ? globalThis.__glassNativeCryptoAesGcm
    : null;
  const pageCryptoAesCbcSource = typeof globalThis.__glassNativeCryptoAesCbc === "function"
    ? globalThis.__glassNativeCryptoAesCbc
    : null;
  const pageCryptoAesCtrSource = typeof globalThis.__glassNativeCryptoAesCtr === "function"
    ? globalThis.__glassNativeCryptoAesCtr
    : null;
  const pageCryptoAesUsages = (values) => {{
    if (!Array.isArray(values)) throw new TypeError("native crypto usages must be an array");
    const usages = [];
    for (const value of values) {{
      const usage = String(value);
      if (!["encrypt", "decrypt"].includes(usage) || usages.includes(usage))
        throw new SyntaxError("native AES usage is invalid or duplicated");
      usages.push(usage);
    }}
    return usages;
  }};
  const pageCryptoAesAlgorithm = (algorithm) => {{
    if (!algorithm || typeof algorithm !== "object")
      throw pageCryptoDigestError("native AES-GCM algorithm is required", "TypeError");
    if (String(algorithm.name || "").toUpperCase() !== "AES-GCM")
      throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
    const iv = pageCryptoBufferInput(algorithm.iv);
    if (iv.length !== 12)
      throw pageCryptoDigestError("native AES-GCM IV must be 12 bytes", "DataError");
    const additionalData = algorithm.additionalData === undefined
      ? []
      : pageCryptoBufferInput(algorithm.additionalData);
    if (additionalData.length > {native_form_body_bytes})
      throw pageCryptoDigestError("native AES-GCM additional data is too large", "DataError");
    const tagLength = algorithm.tagLength === undefined ? 128 : Number(algorithm.tagLength);
    if (tagLength !== 128)
      throw pageCryptoDigestError("native AES-GCM tag length is unsupported", "NotSupportedError");
    return {{ iv, additionalData, tagLength }};
  }};
  const pageCryptoAesCbcAlgorithm = (algorithm) => {{
    if (!algorithm || typeof algorithm !== "object")
      throw pageCryptoDigestError("native AES-CBC algorithm is required", "TypeError");
    if (String(algorithm.name || "").toUpperCase() !== "AES-CBC")
      throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
    const iv = pageCryptoBufferInput(algorithm.iv);
    if (iv.length !== 16)
      throw pageCryptoDigestError("native AES-CBC IV must be 16 bytes", "DataError");
    return {{ iv }};
  }};
  const pageCryptoAesCtrAlgorithm = (algorithm) => {{
    if (!algorithm || typeof algorithm !== "object")
      throw pageCryptoDigestError("native AES-CTR algorithm is required", "TypeError");
    if (String(algorithm.name || "").toUpperCase() !== "AES-CTR")
      throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
    const counter = pageCryptoBufferInput(algorithm.counter);
    if (counter.length !== 16)
      throw pageCryptoDigestError("native AES-CTR counter must be 16 bytes", "DataError");
    const length = Number(algorithm.length);
    if (!Number.isSafeInteger(length) || length < 1 || length > 128)
      throw pageCryptoDigestError("native AES-CTR counter length is invalid", "DataError");
    return {{ counter, length }};
  }};
  const pageCryptoMakeAesKey = (bytes, extractable, usages, algorithmName = "AES-GCM") => {{
    const key = Object.create(PageCryptoKeyNative.prototype);
    const algorithm = Object.freeze({{ name: algorithmName, length: bytes.length * 8 }});
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {{
      type: {{ configurable: false, enumerable: true, value: "secret" }},
      extractable: {{ configurable: false, enumerable: true, value: Boolean(extractable) }},
      algorithm: {{ configurable: false, enumerable: true, value: algorithm }},
      usages: {{ configurable: false, enumerable: true, value: normalizedUsages }},
    }});
    pageCryptoKeyStore.set(key, {{
      kind: algorithmName,
      bytes: bytes.slice(),
      extractable: Boolean(extractable),
      length: bytes.length * 8,
      usages: normalizedUsages,
    }});
    return Object.freeze(key);
  }};
  const pageCryptoDeriveUsages = (values) => {{
    if (!Array.isArray(values)) throw new TypeError("native crypto usages must be an array");
    const usages = [];
    for (const value of values) {{
      const usage = String(value);
      if (!["deriveBits", "deriveKey"].includes(usage) || usages.includes(usage))
        throw new SyntaxError("native crypto derivation usage is invalid or duplicated");
      usages.push(usage);
    }}
    return usages;
  }};
  const pageCryptoMakeDeriveKey = (kind, bytes, usages) => {{
    const key = Object.create(PageCryptoKeyNative.prototype);
    const algorithm = Object.freeze({{ name: kind }});
    const normalizedUsages = Object.freeze(usages.slice());
    Object.defineProperties(key, {{
      type: {{ configurable: false, enumerable: true, value: "secret" }},
      extractable: {{ configurable: false, enumerable: true, value: false }},
      algorithm: {{ configurable: false, enumerable: true, value: algorithm }},
      usages: {{ configurable: false, enumerable: true, value: normalizedUsages }},
    }});
    pageCryptoKeyStore.set(key, {{
      kind,
      bytes: bytes.slice(),
      extractable: false,
      usages: normalizedUsages,
    }});
    return Object.freeze(key);
  }};
  const pageCryptoHmacDefaultLength = (hashName) =>
    ["SHA-384", "SHA-512"].includes(hashName) ? 1024 : 512;
  const pageCryptoGenerateKey = (algorithm, extractable, keyUsages) => {{
    if (!algorithm || typeof algorithm !== "object")
      throw pageCryptoDigestError("native crypto key algorithm is required", "TypeError");
    const algorithmName = String(algorithm.name || "").toUpperCase();
    if (algorithmName === "ED25519") {{
      const usages = pageCryptoEd25519Usages(keyUsages);
      const privateBytes = pageCryptoTake(32);
      const publicBytes = pageCryptoEd25519PublicKey(privateBytes);
      return Object.freeze({{
        privateKey: pageCryptoMakeEd25519PrivateKey(
          privateBytes,
          publicBytes,
          extractable,
          usages.filter(usage => usage === "sign"),
        ),
        publicKey: pageCryptoMakeEd25519PublicKey(
          publicBytes,
          true,
          usages.filter(usage => usage === "verify"),
        ),
      }});
    }}
    if (algorithmName === "HMAC") {{
      const hashName = pageCryptoHashName(algorithm.hash);
      const length = algorithm.length === undefined
        ? pageCryptoHmacDefaultLength(hashName)
        : Number(algorithm.length);
      if (!Number.isSafeInteger(length) || length <= 0 || length % 8 !== 0
          || length / 8 > {page_crypto_values_limit})
        throw pageCryptoDigestError("native HMAC key length is invalid", "DataError");
      return pageCryptoMakeKey(
        pageCryptoTake(length / 8),
        hashName,
        extractable,
        pageCryptoUsages(keyUsages),
      );
    }}
    if (["AES-GCM", "AES-CBC", "AES-CTR"].includes(algorithmName)) {{
      const length = Number(algorithm.length);
      if (![128, 192, 256].includes(length))
        throw pageCryptoDigestError("native AES key length is invalid", "DataError");
      return pageCryptoMakeAesKey(
        pageCryptoTake(length / 8),
        extractable,
        pageCryptoAesUsages(keyUsages),
        algorithmName,
      );
    }}
    throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
  }};
  const pageCryptoDeriveAlgorithm = (algorithm, state) => {{
    if (!algorithm || typeof algorithm !== "object")
      throw pageCryptoDigestError("native crypto derivation algorithm is required", "TypeError");
    const name = String(algorithm.name || "").toUpperCase();
    if (name !== state.kind)
      throw pageCryptoDigestError("native crypto derivation algorithm does not match the key", "InvalidAccessError");
    const hashName = pageCryptoHashName(algorithm.hash);
    const salt = pageCryptoBufferInput(algorithm.salt);
    if (salt.length > {native_form_body_bytes})
      throw pageCryptoDigestError("native crypto derivation salt is too large", "DataError");
    if (name === "HKDF") {{
      const info = pageCryptoBufferInput(algorithm.info);
      if (info.length > {native_form_body_bytes})
        throw pageCryptoDigestError("native HKDF info is too large", "DataError");
      return {{ hashName, salt, info, iterations: 0 }};
    }}
    const iterations = Number(algorithm.iterations);
    if (!Number.isSafeInteger(iterations) || iterations < 1 || iterations > 100000)
      throw pageCryptoDigestError("native PBKDF2 iterations are invalid", "DataError");
    return {{ hashName, salt, info: [], iterations }};
  }};
  const pageCryptoDeriveHashLength = (hashName) =>
    ({{ "SHA-1": 20, "SHA-256": 32, "SHA-384": 48, "SHA-512": 64 }})[hashName] || 0;
  const pageCryptoCheckDeriveWork = (state, algorithm, length) => {{
    const hashLength = pageCryptoDeriveHashLength(algorithm.hashName);
    const blockCount = Math.ceil(length / hashLength);
    if (state.kind === "HKDF" && length > hashLength * 255)
      throw pageCryptoDigestError("native HKDF output is too large", "DataError");
    const workPerBlock = state.kind === "HKDF"
      ? algorithm.info.length + hashLength + 1
      : algorithm.salt.length + 4 + algorithm.iterations * hashLength;
    if (blockCount > 0 && workPerBlock > Math.floor({native_crypto_derive_work} / blockCount))
      throw pageCryptoDigestError("native crypto derivation work is too large", "DataError");
  }};
  const pageCryptoDeriveBytes = (state, algorithm, length) => {{
    if (!pageCryptoDeriveSource)
      throw pageCryptoDigestError("native crypto derivation is unavailable", "OperationError");
    if (!Number.isSafeInteger(length) || length < 0 || length > {native_form_body_bytes})
      throw pageCryptoDigestError("native crypto derivation output is too large", "DataError");
    pageCryptoCheckDeriveWork(state, algorithm, length);
    const encoded = pageCryptoDeriveSource(
      state.kind,
      algorithm.hashName,
      encodeBase64(state.bytes, {native_form_body_bytes}),
      encodeBase64(algorithm.salt, {native_form_body_bytes}),
      encodeBase64(algorithm.info, {native_form_body_bytes}),
      algorithm.iterations,
      length * 8,
    );
    return decodeBase64(encoded, {native_form_body_bytes});
  }};
  const pageCryptoDerivedKey = (state, algorithm, targetAlgorithm, extractable, keyUsages) => {{
    if (!targetAlgorithm || typeof targetAlgorithm !== "object")
      throw pageCryptoDigestError("native derived key algorithm is required", "TypeError");
    const name = String(targetAlgorithm.name || "").toUpperCase();
    if (name === "HMAC") {{
      const hashName = pageCryptoHashName(targetAlgorithm.hash);
      const length = targetAlgorithm.length === undefined
        ? pageCryptoHmacDefaultLength(hashName)
        : Number(targetAlgorithm.length);
      if (!Number.isSafeInteger(length) || length <= 0 || length % 8 !== 0
          || length / 8 > {page_crypto_values_limit})
        throw pageCryptoDigestError("native derived HMAC key length is invalid", "DataError");
      return pageCryptoMakeKey(
        pageCryptoDeriveBytes(state, algorithm, length / 8),
        hashName,
        extractable,
        pageCryptoUsages(keyUsages),
      );
    }}
    if (["AES-GCM", "AES-CBC", "AES-CTR"].includes(name)) {{
      const length = Number(targetAlgorithm.length);
      if (![128, 192, 256].includes(length))
        throw pageCryptoDigestError("native derived AES key length is invalid", "DataError");
      return pageCryptoMakeAesKey(
        pageCryptoDeriveBytes(state, algorithm, length / 8),
        extractable,
        pageCryptoAesUsages(keyUsages),
        name,
      );
    }}
    throw pageCryptoDigestError("native derived key algorithm is unsupported", "NotSupportedError");
  }};
  const pageCryptoJwkDecode = (value) => {{
    if (typeof value !== "string" || value.length > Math.ceil({native_form_body_bytes} / 3) * 4
        || !/^[A-Za-z0-9_-]*$/.test(value) || value.length % 4 === 1)
      throw pageCryptoDigestError("native JWK key material is invalid", "DataError");
    const remainder = value.length % 4;
    const standard = value.replace(/-/g, "+").replace(/_/g, "/")
      + (remainder === 0 ? "" : "=".repeat(4 - remainder));
    return decodeBase64(standard, {native_form_body_bytes});
  }};
  const pageCryptoJwkEncode = (bytes) =>
    encodeBase64(bytes, {native_form_body_bytes})
      .replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
  const pageCryptoJwkUsages = (jwk, usages) => {{
    if (jwk.key_ops === undefined) return usages;
    if (!Array.isArray(jwk.key_ops))
      throw pageCryptoDigestError("native JWK key_ops is invalid", "DataError");
    const operations = [];
    for (const value of jwk.key_ops) {{
      const operation = String(value);
      if (!["sign", "verify", "encrypt", "decrypt"].includes(operation)
          || operations.includes(operation))
        throw pageCryptoDigestError("native JWK key_ops is invalid", "DataError");
      operations.push(operation);
    }}
    if (usages.some(usage => !operations.includes(usage)))
      throw pageCryptoDigestError("native JWK key_ops does not permit usage", "DataError");
    return usages;
  }};
  const pageCryptoImportJwk = (jwk, algorithm, extractable, keyUsages) => {{
    if (!jwk || typeof jwk !== "object" || Array.isArray(jwk))
      throw pageCryptoDigestError("native JWK is invalid", "DataError");
    if (jwk.ext !== undefined && typeof jwk.ext !== "boolean")
      throw pageCryptoDigestError("native JWK ext is invalid", "DataError");
    if (jwk.ext === false && extractable)
      throw pageCryptoDigestError("native JWK is not extractable", "DataError");
    const algorithmName = algorithm && typeof algorithm === "object"
      ? String(algorithm.name || "").toUpperCase()
      : "";
    if (algorithmName === "ED25519") {{
      if (jwk.kty !== "OKP" || jwk.crv !== "Ed25519" || typeof jwk.x !== "string"
          || (jwk.alg !== undefined && jwk.alg !== "EdDSA"))
        throw pageCryptoDigestError("native Ed25519 JWK is invalid", "DataError");
      const publicBytes = pageCryptoJwkDecode(jwk.x);
      if (publicBytes.length !== 32)
        throw pageCryptoDigestError("native Ed25519 JWK public key is invalid", "DataError");
      const usages = pageCryptoJwkUsages(jwk, pageCryptoEd25519Usages(keyUsages));
      if (jwk.d !== undefined) {{
        if (typeof jwk.d !== "string" || usages.some(usage => usage !== "sign"))
          throw pageCryptoDigestError("native Ed25519 private JWK usage is invalid", "DataError");
        const privateBytes = pageCryptoJwkDecode(jwk.d);
        if (privateBytes.length !== 32)
          throw pageCryptoDigestError("native Ed25519 JWK private key is invalid", "DataError");
        const derivedPublicBytes = pageCryptoEd25519PublicKey(privateBytes);
        if (derivedPublicBytes.length !== publicBytes.length
            || derivedPublicBytes.some((value, index) => value !== publicBytes[index]))
          throw pageCryptoDigestError("native Ed25519 JWK key pair does not match", "DataError");
        return pageCryptoMakeEd25519PrivateKey(
          privateBytes,
          publicBytes,
          extractable,
          usages,
        );
      }}
      if (usages.some(usage => usage !== "verify"))
        throw pageCryptoDigestError("native Ed25519 public JWK usage is invalid", "DataError");
      return pageCryptoMakeEd25519PublicKey(publicBytes, extractable, usages);
    }}
    if (jwk.kty !== "oct" || typeof jwk.k !== "string")
      throw pageCryptoDigestError("native JWK is invalid", "DataError");
    const bytes = pageCryptoJwkDecode(jwk.k);
    if (algorithmName === "HMAC") {{
      if (bytes.length === 0)
        throw pageCryptoDigestError("native HMAC JWK key is empty", "DataError");
      const hashName = pageCryptoHashName(algorithm.hash);
      const expectedAlgorithm = ({{ "SHA-1": "HS1", "SHA-256": "HS256", "SHA-384": "HS384", "SHA-512": "HS512" }})[hashName];
      if (jwk.alg !== undefined && jwk.alg !== expectedAlgorithm)
        throw pageCryptoDigestError("native HMAC JWK algorithm does not match", "DataError");
      return pageCryptoMakeKey(
        bytes,
        hashName,
        extractable,
        pageCryptoJwkUsages(jwk, pageCryptoUsages(keyUsages)),
      );
    }}
    if (algorithmName === "AES-GCM") {{
      const expectedAlgorithm = ({{ 16: "A128GCM", 24: "A192GCM", 32: "A256GCM" }})[bytes.length];
      if (!expectedAlgorithm || (jwk.alg !== undefined && jwk.alg !== expectedAlgorithm))
        throw pageCryptoDigestError("native AES-GCM JWK key length or algorithm is invalid", "DataError");
      return pageCryptoMakeAesKey(
        bytes,
        extractable,
        pageCryptoJwkUsages(jwk, pageCryptoAesUsages(keyUsages)),
      );
    }}
    throw pageCryptoDigestError("native JWK algorithm is unsupported", "NotSupportedError");
  }};
  const pageCryptoExportJwk = (state) => {{
    if (state.kind === "ED25519-PUBLIC") {{
      return {{ kty: "OKP", crv: "Ed25519", x: pageCryptoJwkEncode(state.bytes), alg: "EdDSA", ext: Boolean(state.extractable), key_ops: Array.from(state.usages) }};
    }}
    if (state.kind === "ED25519-PRIVATE") {{
      return {{ kty: "OKP", crv: "Ed25519", x: pageCryptoJwkEncode(state.publicBytes), d: pageCryptoJwkEncode(state.bytes), alg: "EdDSA", ext: Boolean(state.extractable), key_ops: Array.from(state.usages) }};
    }}
    if (state.kind === "HMAC") {{
      const alg = ({{ "SHA-1": "HS1", "SHA-256": "HS256", "SHA-384": "HS384", "SHA-512": "HS512" }})[state.hashName];
      return {{ kty: "oct", k: pageCryptoJwkEncode(state.bytes), alg, ext: Boolean(state.extractable), key_ops: Array.from(state.usages) }};
    }}
    if (state.kind === "AES-GCM") {{
      const alg = ({{ 16: "A128GCM", 24: "A192GCM", 32: "A256GCM" }})[state.bytes.length];
      return {{ kty: "oct", k: pageCryptoJwkEncode(state.bytes), alg, ext: Boolean(state.extractable), key_ops: Array.from(state.usages) }};
    }}
    throw pageCryptoDigestError("native JWK key kind is unsupported", "NotSupportedError");
  }};
  const pageCryptoAesBytes = (operation, state, algorithm, data) => {{
    if (!pageCryptoAesSource)
      throw pageCryptoDigestError("native AES-GCM is unavailable", "OperationError");
    const encoded = pageCryptoAesSource(
      operation,
      encodeBase64(state.bytes, {native_form_body_bytes}),
      encodeBase64(algorithm.iv, {native_form_body_bytes}),
      encodeBase64(algorithm.additionalData, {native_form_body_bytes}),
      algorithm.tagLength,
      encodeBase64(data, {native_form_body_bytes}),
    );
    return decodeBase64(encoded, {native_form_body_bytes});
  }};
  const pageCryptoAesCbcBytes = (operation, state, algorithm, data) => {{
    if (!pageCryptoAesCbcSource)
      throw pageCryptoDigestError("native AES-CBC is unavailable", "OperationError");
    const encoded = pageCryptoAesCbcSource(
      operation,
      encodeBase64(state.bytes, {native_form_body_bytes}),
      encodeBase64(algorithm.iv, {native_form_body_bytes}),
      encodeBase64(data, {native_form_body_bytes}),
    );
    return decodeBase64(encoded, {native_form_body_bytes});
  }};
  const pageCryptoAesCtrBytes = (state, algorithm, data) => {{
    if (!pageCryptoAesCtrSource)
      throw pageCryptoDigestError("native AES-CTR is unavailable", "OperationError");
    const encoded = pageCryptoAesCtrSource(
      encodeBase64(state.bytes, {native_form_body_bytes}),
      encodeBase64(algorithm.counter, {native_form_body_bytes}),
      algorithm.length,
      encodeBase64(data, {native_form_body_bytes}),
    );
    return decodeBase64(encoded, {native_form_body_bytes});
  }};
  pageSubtle.digest = (algorithm, data) => {{
    const name = algorithm && typeof algorithm === "object"
      ? String(algorithm.name || "")
      : String(algorithm || "");
    const normalizedName = name.toUpperCase();
    if (!["SHA-1", "SHA-256", "SHA-384", "SHA-512"].includes(normalizedName))
      return Promise.reject(pageCryptoDigestError("native crypto digest algorithm is unsupported", "NotSupportedError"));
    let bytes;
    try {{ bytes = pageCryptoBufferInput(data); }} catch (error) {{ return Promise.reject(error); }}
    if (bytes.length > {native_form_body_bytes})
      return Promise.reject(pageCryptoDigestError("native crypto digest data is too large", "DataError"));
    if (!pageCryptoDigestSource)
      return Promise.reject(pageCryptoDigestError("native crypto digest is unavailable", "OperationError"));
    try {{
      const encoded = encodeBase64(bytes, {native_form_body_bytes});
      const digest = pageCryptoDigestSource(normalizedName, encoded);
      return Promise.resolve(new Uint8Array(decodeBase64(digest, {native_form_body_bytes})).buffer);
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.generateKey = (algorithm, extractable = false, keyUsages = []) => {{
    try {{
      return Promise.resolve(pageCryptoGenerateKey(algorithm, extractable, keyUsages));
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.deriveBits = (algorithm, baseKey, length) => {{
    try {{
      const state = pageCryptoKeyState(baseKey);
      if (!["HKDF", "PBKDF2"].includes(state.kind))
        throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
      if (!state.usages.includes("deriveBits"))
        throw pageCryptoDigestError("native CryptoKey cannot derive bits", "InvalidAccessError");
      if (!Number.isSafeInteger(length) || length < 0 || length % 8 !== 0
          || length / 8 > {native_form_body_bytes})
        throw pageCryptoDigestError("native crypto derivation length is invalid", "DataError");
      const normalized = pageCryptoDeriveAlgorithm(algorithm, state);
      return Promise.resolve(new Uint8Array(
        pageCryptoDeriveBytes(state, normalized, length / 8)
      ).buffer);
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.deriveKey = (algorithm, baseKey, derivedKeyAlgorithm, extractable = false, keyUsages = []) => {{
    try {{
      const state = pageCryptoKeyState(baseKey);
      if (!["HKDF", "PBKDF2"].includes(state.kind))
        throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
      if (!state.usages.includes("deriveKey"))
        throw pageCryptoDigestError("native CryptoKey cannot derive a key", "InvalidAccessError");
      const normalized = pageCryptoDeriveAlgorithm(algorithm, state);
      return Promise.resolve(pageCryptoDerivedKey(
        state, normalized, derivedKeyAlgorithm, extractable, keyUsages
      ));
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.importKey = (format, keyData, algorithm, extractable = false, keyUsages = []) => {{
    try {{
      const normalizedFormat = String(format).toLowerCase();
      if (normalizedFormat === "jwk")
        return Promise.resolve(pageCryptoImportJwk(keyData, algorithm, extractable, keyUsages));
      if (normalizedFormat !== "raw")
        throw pageCryptoDigestError("native crypto key format is unsupported", "NotSupportedError");
      const bytes = pageCryptoBufferInput(keyData);
      if (bytes.length > {native_form_body_bytes})
        throw pageCryptoDigestError("native crypto key data is invalid", "DataError");
      const algorithmName = algorithm && typeof algorithm === "object"
        ? String(algorithm.name || "").toUpperCase()
        : "";
      if (["HKDF", "PBKDF2"].includes(algorithmName)) {{
        if (extractable)
          throw pageCryptoDigestError("native derivation keys are not extractable", "SyntaxError");
        return Promise.resolve(pageCryptoMakeDeriveKey(
          algorithmName, bytes, pageCryptoDeriveUsages(keyUsages)
        ));
      }}
      if (algorithmName === "ED25519") {{
        if (bytes.length !== 32)
          throw pageCryptoDigestError("native Ed25519 public key length is invalid", "DataError");
        const usages = pageCryptoEd25519Usages(keyUsages);
        if (usages.some(usage => usage !== "verify"))
          throw pageCryptoDigestError("native Ed25519 public key usage is invalid", "SyntaxError");
        return Promise.resolve(pageCryptoMakeEd25519PublicKey(bytes, extractable, usages));
      }}
      if (bytes.length === 0)
        throw pageCryptoDigestError("native crypto key data is invalid", "DataError");
      if (["AES-GCM", "AES-CBC", "AES-CTR"].includes(algorithmName)) {{
        if (![16, 24, 32].includes(bytes.length))
          throw pageCryptoDigestError("native AES key length is invalid", "DataError");
        return Promise.resolve(pageCryptoMakeAesKey(
          bytes, extractable, pageCryptoAesUsages(keyUsages), algorithmName
        ));
      }}
      const hashName = pageCryptoHmacHash(algorithm);
      return Promise.resolve(pageCryptoMakeKey(bytes, hashName, extractable, pageCryptoUsages(keyUsages)));
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.exportKey = (format, key) => {{
    try {{
      const normalizedFormat = String(format).toLowerCase();
      const state = pageCryptoKeyState(key);
      if (normalizedFormat === "jwk") {{
        if (!state.extractable)
          throw pageCryptoDigestError("native CryptoKey is not extractable", "InvalidAccessError");
        return Promise.resolve(pageCryptoExportJwk(state));
      }}
      if (normalizedFormat !== "raw")
        throw pageCryptoDigestError("native crypto key format is unsupported", "NotSupportedError");
      if (!state.extractable)
        throw pageCryptoDigestError("native CryptoKey is not extractable", "InvalidAccessError");
      if (state.kind !== "HMAC" && !["AES-GCM", "AES-CBC", "AES-CTR"].includes(state.kind)
          && state.kind !== "ED25519-PUBLIC")
        throw pageCryptoDigestError("native crypto raw export is unsupported", "NotSupportedError");
      return Promise.resolve(new Uint8Array(state.bytes.slice()).buffer);
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.sign = (algorithm, key, data) => {{
    try {{
      const state = pageCryptoKeyState(key);
      if (state.kind === "ED25519-PRIVATE") {{
        pageCryptoEd25519Algorithm(algorithm);
        if (!state.usages.includes("sign"))
          throw pageCryptoDigestError("native Ed25519 private key cannot sign", "InvalidAccessError");
        const bytes = pageCryptoBufferInput(data);
        if (bytes.length > {native_form_body_bytes})
          throw pageCryptoDigestError("native Ed25519 data is too large", "DataError");
        return Promise.resolve(new Uint8Array(
          pageCryptoEd25519Bytes("sign", state, bytes)
        ).buffer);
      }}
      if (state.kind !== "HMAC")
        throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
      pageCryptoOperationHash(algorithm, state);
      if (!state.usages.includes("sign"))
        throw pageCryptoDigestError("native CryptoKey cannot sign", "InvalidAccessError");
      return Promise.resolve(new Uint8Array(pageCryptoHmacBytes(state, pageCryptoBufferInput(data))).buffer);
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.verify = (algorithm, key, signature, data) => {{
    try {{
      const state = pageCryptoKeyState(key);
      if (state.kind === "ED25519-PUBLIC") {{
        pageCryptoEd25519Algorithm(algorithm);
        if (!state.usages.includes("verify"))
          throw pageCryptoDigestError("native Ed25519 public key cannot verify", "InvalidAccessError");
        const expected = pageCryptoBufferInput(signature);
        const bytes = pageCryptoBufferInput(data);
        if (expected.length !== 64 || bytes.length > {native_form_body_bytes})
          throw pageCryptoDigestError("native Ed25519 signature or data is invalid", "DataError");
        return Promise.resolve(pageCryptoEd25519Bytes("verify", state, bytes, expected));
      }}
      if (state.kind !== "HMAC")
        throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
      pageCryptoOperationHash(algorithm, state);
      if (!state.usages.includes("verify"))
        throw pageCryptoDigestError("native CryptoKey cannot verify", "InvalidAccessError");
      const expected = pageCryptoBufferInput(signature);
      const actual = pageCryptoHmacBytes(state, pageCryptoBufferInput(data));
      let difference = actual.length === expected.length ? 0 : 1;
      const length = Math.max(actual.length, expected.length);
      for (let index = 0; index < length; index += 1)
        difference |= (actual[index] || 0) ^ (expected[index] || 0);
      return Promise.resolve(difference === 0);
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.encrypt = (algorithm, key, data) => {{
    try {{
      const state = pageCryptoKeyState(key);
      if (!["AES-GCM", "AES-CBC", "AES-CTR"].includes(state.kind))
        throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
      if (!state.usages.includes("encrypt"))
        throw pageCryptoDigestError("native CryptoKey cannot encrypt", "InvalidAccessError");
      const bytes = pageCryptoBufferInput(data);
      if (state.kind === "AES-GCM") {{
        const normalized = pageCryptoAesAlgorithm(algorithm);
        if (bytes.length > {native_form_body_bytes} - 16)
          throw pageCryptoDigestError("native AES-GCM data is too large", "DataError");
        return Promise.resolve(new Uint8Array(pageCryptoAesBytes("encrypt", state, normalized, bytes)).buffer);
      }}
      if (state.kind === "AES-CBC") {{
        const normalized = pageCryptoAesCbcAlgorithm(algorithm);
        if (bytes.length > {native_form_body_bytes} - 16)
          throw pageCryptoDigestError("native AES-CBC data is too large", "DataError");
        try {{
          return Promise.resolve(new Uint8Array(
            pageCryptoAesCbcBytes("encrypt", state, normalized, bytes)
          ).buffer);
        }} catch (_error) {{
          throw pageCryptoDigestError("native AES-CBC operation failed", "OperationError");
        }}
      }}
      const normalized = pageCryptoAesCtrAlgorithm(algorithm);
      if (bytes.length > {native_form_body_bytes})
        throw pageCryptoDigestError("native AES-CTR data is too large", "DataError");
      try {{
        return Promise.resolve(new Uint8Array(
          pageCryptoAesCtrBytes(state, normalized, bytes)
        ).buffer);
      }} catch (_error) {{
        throw pageCryptoDigestError("native AES-CTR operation failed", "OperationError");
      }}
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.decrypt = (algorithm, key, data) => {{
    try {{
      const state = pageCryptoKeyState(key);
      if (!["AES-GCM", "AES-CBC", "AES-CTR"].includes(state.kind))
        throw pageCryptoDigestError("native crypto algorithm is unsupported", "NotSupportedError");
      if (!state.usages.includes("decrypt"))
        throw pageCryptoDigestError("native CryptoKey cannot decrypt", "InvalidAccessError");
      const bytes = pageCryptoBufferInput(data);
      if (state.kind === "AES-GCM") {{
        const normalized = pageCryptoAesAlgorithm(algorithm);
        if (bytes.length < 16 || bytes.length > {native_form_body_bytes})
          throw pageCryptoDigestError("native AES-GCM ciphertext is invalid", "DataError");
        try {{
          return Promise.resolve(new Uint8Array(pageCryptoAesBytes("decrypt", state, normalized, bytes)).buffer);
        }} catch (_error) {{
          return Promise.reject(pageCryptoDigestError("native AES-GCM authentication failed", "OperationError"));
        }}
      }}
      if (state.kind === "AES-CBC") {{
        const normalized = pageCryptoAesCbcAlgorithm(algorithm);
        if (bytes.length === 0 || bytes.length % 16 !== 0 || bytes.length > {native_form_body_bytes})
          throw pageCryptoDigestError("native AES-CBC ciphertext is invalid", "DataError");
        try {{
          return Promise.resolve(new Uint8Array(
            pageCryptoAesCbcBytes("decrypt", state, normalized, bytes)
          ).buffer);
        }} catch (_error) {{
          return Promise.reject(pageCryptoDigestError("native AES-CBC padding is invalid", "OperationError"));
        }}
      }}
      const normalized = pageCryptoAesCtrAlgorithm(algorithm);
      if (bytes.length > {native_form_body_bytes})
        throw pageCryptoDigestError("native AES-CTR ciphertext is too large", "DataError");
      try {{
        return Promise.resolve(new Uint8Array(
          pageCryptoAesCtrBytes(state, normalized, bytes)
        ).buffer);
      }} catch (_error) {{
        return Promise.reject(pageCryptoDigestError("native AES-CTR operation failed", "OperationError"));
      }}
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.wrapKey = (format, key, wrappingKey, wrapAlgorithm) => {{
    try {{
      if (String(format).toLowerCase() !== "raw")
        throw pageCryptoDigestError("native crypto key format is unsupported", "NotSupportedError");
      const source = pageCryptoKeyState(key);
      if (!source.extractable)
        throw pageCryptoDigestError("native CryptoKey is not extractable", "InvalidAccessError");
      if (!["HMAC", "AES-GCM"].includes(source.kind))
        throw pageCryptoDigestError("native crypto key kind is unsupported", "NotSupportedError");
      const wrapping = pageCryptoKeyState(wrappingKey);
      if (wrapping.kind !== "AES-GCM")
        throw pageCryptoDigestError("native wrapping key is unsupported", "NotSupportedError");
      if (!wrapping.usages.includes("encrypt"))
        throw pageCryptoDigestError("native wrapping key cannot encrypt", "InvalidAccessError");
      const normalized = pageCryptoAesAlgorithm(wrapAlgorithm);
      if (source.bytes.length > {native_form_body_bytes} - 16)
        throw pageCryptoDigestError("native wrapped key is too large", "DataError");
      return Promise.resolve(new Uint8Array(
        pageCryptoAesBytes("encrypt", wrapping, normalized, source.bytes.slice())
      ).buffer);
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  pageSubtle.unwrapKey = (format, wrappedKey, unwrappedKeyAlgorithm, unwrappingKey, unwrapAlgorithm, extractable = false, keyUsages = []) => {{
    try {{
      if (String(format).toLowerCase() !== "raw")
        throw pageCryptoDigestError("native crypto key format is unsupported", "NotSupportedError");
      const unwrapping = pageCryptoKeyState(unwrappingKey);
      if (unwrapping.kind !== "AES-GCM")
        throw pageCryptoDigestError("native unwrapping key is unsupported", "NotSupportedError");
      if (!unwrapping.usages.includes("decrypt"))
        throw pageCryptoDigestError("native unwrapping key cannot decrypt", "InvalidAccessError");
      const normalized = pageCryptoAesAlgorithm(unwrapAlgorithm);
      const wrapped = pageCryptoBufferInput(wrappedKey);
      if (wrapped.length < 16 || wrapped.length > {native_form_body_bytes})
        throw pageCryptoDigestError("native wrapped key is invalid", "DataError");
      let bytes;
      try {{
        bytes = pageCryptoAesBytes("decrypt", unwrapping, normalized, wrapped);
      }} catch (_error) {{
        throw pageCryptoDigestError("native wrapped key authentication failed", "OperationError");
      }}
      const algorithmName = unwrappedKeyAlgorithm && typeof unwrappedKeyAlgorithm === "object"
        ? String(unwrappedKeyAlgorithm.name || "").toUpperCase()
        : "";
      if (algorithmName === "HMAC") {{
        if (bytes.length === 0)
          throw pageCryptoDigestError("native unwrapped HMAC key is invalid", "DataError");
        const hashName = pageCryptoHmacHash(unwrappedKeyAlgorithm);
        return Promise.resolve(pageCryptoMakeKey(
          bytes, hashName, extractable, pageCryptoUsages(keyUsages)
        ));
      }}
      if (algorithmName === "AES-GCM") {{
        const length = Number(unwrappedKeyAlgorithm.length);
        if (![128, 192, 256].includes(length) || bytes.length !== length / 8)
          throw pageCryptoDigestError("native unwrapped AES-GCM key length is invalid", "DataError");
        return Promise.resolve(pageCryptoMakeAesKey(
          bytes, extractable, pageCryptoAesUsages(keyUsages)
        ));
      }}
      throw pageCryptoDigestError("native unwrapped key algorithm is unsupported", "NotSupportedError");
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  globalThis.__glassPageSubtleCrypto = pageSubtle;
  pageCrypto.subtle = pageSubtle;
  globalThis.crypto = pageCrypto;
  try {{ delete globalThis.__glassNativeRandomBytes; }} catch (_error) {{}}
  try {{ delete globalThis.__glassNativeCryptoDigest; }} catch (_error) {{}}
  try {{ delete globalThis.__glassNativeCryptoHmac; }} catch (_error) {{}}
  try {{ delete globalThis.__glassNativeCryptoEd25519; }} catch (_error) {{}}
  try {{ delete globalThis.__glassNativeCryptoAesGcm; }} catch (_error) {{}}
  try {{ delete globalThis.__glassNativeCryptoAesCbc; }} catch (_error) {{}}
  try {{ delete globalThis.__glassNativeCryptoAesCtr; }} catch (_error) {{}}
  try {{ delete globalThis.__glassNativeCryptoDerive; }} catch (_error) {{}}
  const nativeStorageUsage = () => {{
    const encoded = (value) => {{
      try {{ return JSON.stringify(value); }} catch (_error) {{ return ""; }}
    }};
    const mapBytes = (value) => {{
      const text = value instanceof Map ? encoded(Array.from(value.entries())) : encoded(value);
      return text ? text.length : 0;
    }};
    const usage = mapBytes(globalThis.__glassLocalStorageValues)
      + mapBytes(globalThis.__glassSessionStorageValues)
      + encoded(globalThis.__glassIndexedDbState || {{ databases: {{}} }}).length;
    return Math.min({storage_quota}, usage);
  }};
  const nativeStorageManager = globalThis.__glassNativeStorageManager instanceof Object
    ? globalThis.__glassNativeStorageManager
    : {{}};
  nativeStorageManager.estimate = () => Promise.resolve({{ usage: nativeStorageUsage(), quota: {storage_quota} }});
  nativeStorageManager.persist = () => Promise.resolve(false);
  nativeStorageManager.persisted = () => Promise.resolve(false);
  globalThis.__glassNativeStorageManager = nativeStorageManager;
  globalThis.navigator.storage = nativeStorageManager;
  const ensureNativeConstructor = (name, parent) => {{
    let constructor = globalThis[name];
    if (typeof constructor !== "function") {{
      constructor = function NativeWebIdlConstructor() {{
        const htmlElementBase = name === "HTMLElement"
          || (name.startsWith("HTML")
            && parent
            && globalThis.HTMLElement
            && (parent === globalThis.HTMLElement
              || globalThis.HTMLElement.prototype.isPrototypeOf(parent.prototype)));
        if (htmlElementBase
            && typeof globalThis.__glassConstructNativeCustomElement === "function") {{
          return globalThis.__glassConstructNativeCustomElement(new.target, name);
        }}
        throw new TypeError("Illegal constructor");
      }};
      globalThis[name] = constructor;
    }}
    if (constructor.prototype && parent && parent.prototype
        && Object.getPrototypeOf(constructor.prototype) !== parent.prototype) {{
      try {{ Object.setPrototypeOf(constructor.prototype, parent.prototype); }} catch (_error) {{}}
    }}
    return constructor;
  }};
  const defineDomRectValues = (target, x, y, width, height, writable) => {{
    const values = {{
      x: Number(x) || 0,
      y: Number(y) || 0,
      width: Number(width) || 0,
      height: Number(height) || 0,
    }};
    Object.defineProperties(target, {{
      x: {{ configurable: true, enumerable: true, writable, value: values.x }},
      y: {{ configurable: true, enumerable: true, writable, value: values.y }},
      width: {{ configurable: true, enumerable: true, writable, value: values.width }},
      height: {{ configurable: true, enumerable: true, writable, value: values.height }},
      top: {{ configurable: true, enumerable: true, get() {{ return this.y; }} }},
      right: {{ configurable: true, enumerable: true, get() {{ return this.x + this.width; }} }},
      bottom: {{ configurable: true, enumerable: true, get() {{ return this.y + this.height; }} }},
      left: {{ configurable: true, enumerable: true, get() {{ return this.x; }} }},
    }});
  }};
  const DOMRectReadOnlyNative = globalThis.__glassDOMRectReadOnlyConstructor || function DOMRectReadOnly(
    x = 0,
    y = 0,
    width = 0,
    height = 0,
  ) {{
    if (!(this instanceof DOMRectReadOnlyNative)) throw new TypeError("DOMRectReadOnly requires new");
    defineDomRectValues(this, x, y, width, height, false);
  }};
  if (!DOMRectReadOnlyNative.prototype.toJSON) {{
    Object.defineProperty(DOMRectReadOnlyNative.prototype, "toJSON", {{
      configurable: true,
      value() {{
        return {{
          x: this.x,
          y: this.y,
          width: this.width,
          height: this.height,
          top: this.top,
          right: this.right,
          bottom: this.bottom,
          left: this.left,
        }};
      }},
    }});
  }}
  const DOMRectNative = globalThis.__glassDOMRectConstructor || function DOMRect(
    x = 0,
    y = 0,
    width = 0,
    height = 0,
  ) {{
    if (!(this instanceof DOMRectNative)) throw new TypeError("DOMRect requires new");
    defineDomRectValues(this, x, y, width, height, true);
  }};
  try {{ Object.setPrototypeOf(DOMRectNative.prototype, DOMRectReadOnlyNative.prototype); }} catch (_error) {{}}
  globalThis.__glassDOMRectReadOnlyConstructor = DOMRectReadOnlyNative;
  globalThis.__glassDOMRectConstructor = DOMRectNative;
  globalThis.DOMRectReadOnly = DOMRectReadOnlyNative;
  globalThis.DOMRect = DOMRectNative;
  const ResizeObserverNative = globalThis.__glassResizeObserverConstructor || function ResizeObserver(callback) {{
    if (!(this instanceof ResizeObserverNative)) throw new TypeError("ResizeObserver requires new");
    if (typeof callback !== "function") throw new TypeError("ResizeObserver callback must be callable");
    Object.defineProperties(this, {{
      __glassCallback: {{ configurable: false, enumerable: false, value: callback }},
      __glassRegistrations: {{ configurable: false, enumerable: false, writable: true, value: [] }},
      __glassRecords: {{ configurable: false, enumerable: false, writable: true, value: [] }},
    }});
    resizeObservers.add(this);
  }};
  if (!ResizeObserverNative.prototype.observe) {{
    Object.defineProperties(ResizeObserverNative.prototype, {{
      observe: {{
        configurable: true,
        value(target, options = {{}}) {{
          if (!target || typeof target.nodeIndex !== "number") throw new TypeError("ResizeObserver target must be an Element");
          if (!options || typeof options !== "object") throw new TypeError("ResizeObserver options must be an object");
          const box = options.box === undefined ? "content-box" : String(options.box);
          if (!["content-box", "border-box", "device-pixel-content-box"].includes(box)) {{
            throw new TypeError("ResizeObserver box is unsupported");
          }}
          const registrations = this.__glassRegistrations;
          const existing = registrations.find((registration) => registration.target === target);
          if (existing) {{
            existing.box = box;
            return;
          }}
          registrations.push({{ target, box, last: null }});
        }},
      }},
      unobserve: {{
        configurable: true,
        value(target) {{
          this.__glassRegistrations = this.__glassRegistrations.filter(
            (registration) => registration.target !== target,
          );
        }},
      }},
      disconnect: {{
        configurable: true,
        value() {{
          this.__glassRegistrations = [];
          this.__glassRecords = [];
        }},
      }},
      takeRecords: {{
        configurable: true,
        value() {{
          const records = this.__glassRecords.slice();
          this.__glassRecords = [];
          return records;
        }},
      }},
    }});
  }}
  globalThis.__glassResizeObserverConstructor = ResizeObserverNative;
  globalThis.ResizeObserver = ResizeObserverNative;
  const normalizeIntersectionThresholds = (value) => {{
    const values = Array.isArray(value) ? value.slice() : [value === undefined ? 0 : value];
    if (values.length === 0 || values.length > 16) throw new RangeError("IntersectionObserver threshold limit exceeded");
    const normalized = values.map((entry) => Number(entry));
    if (normalized.some((entry) => !Number.isFinite(entry) || entry < 0 || entry > 1)) {{
      throw new RangeError("IntersectionObserver threshold must be between 0 and 1");
    }}
    return Array.from(new Set(normalized)).sort((left, right) => left - right);
  }};
  const normalizeIntersectionRootMargin = (value) => {{
    const raw = String(value === undefined ? "0px" : value).trim();
    const tokens = raw ? raw.split(/\s+/) : ["0px"];
    if (tokens.length < 1 || tokens.length > 4) throw new TypeError("IntersectionObserver rootMargin is invalid");
    const values = tokens.map((token) => {{
      if (!/^-?(?:\d+\.?\d*|\.\d+)px$/.test(token)) throw new TypeError("IntersectionObserver rootMargin only supports px lengths");
      const number = Number(token.slice(0, -2));
      if (!Number.isFinite(number)) throw new TypeError("IntersectionObserver rootMargin is invalid");
      return number;
    }});
    const expanded = values.length === 1
      ? [values[0], values[0], values[0], values[0]]
      : values.length === 2
        ? [values[0], values[1], values[0], values[1]]
        : values.length === 3
          ? [values[0], values[1], values[2], values[1]]
          : values;
    return {{
      values: expanded,
      text: expanded.map((entry) => String(entry) + "px").join(" "),
    }};
  }};
  const IntersectionObserverEntryNative = globalThis.__glassIntersectionObserverEntryConstructor || function IntersectionObserverEntry(time, init) {{
    if (!(this instanceof IntersectionObserverEntryNative)) throw new TypeError("IntersectionObserverEntry requires new");
    Object.defineProperties(this, {{
      time: {{ configurable: true, enumerable: true, value: Number(time) || 0 }},
      target: {{ configurable: true, enumerable: true, value: init.target || null }},
      rootBounds: {{ configurable: true, enumerable: true, value: init.rootBounds || null }},
      boundingClientRect: {{ configurable: true, enumerable: true, value: init.boundingClientRect || makeDomRect(null) }},
      intersectionRect: {{ configurable: true, enumerable: true, value: init.intersectionRect || makeDomRect(null) }},
      isIntersecting: {{ configurable: true, enumerable: true, value: Boolean(init.isIntersecting) }},
      intersectionRatio: {{ configurable: true, enumerable: true, value: Number(init.intersectionRatio) || 0 }},
      isVisible: {{ configurable: true, enumerable: true, value: Boolean(init.isVisible) }},
    }});
  }};
  globalThis.__glassIntersectionObserverEntryConstructor = IntersectionObserverEntryNative;
  globalThis.IntersectionObserverEntry = IntersectionObserverEntryNative;
  const IntersectionObserverNative = globalThis.__glassIntersectionObserverConstructor || function IntersectionObserver(callback, options = {{}}) {{
    if (!(this instanceof IntersectionObserverNative)) throw new TypeError("IntersectionObserver requires new");
    if (typeof callback !== "function") throw new TypeError("IntersectionObserver callback must be callable");
    if (!options || typeof options !== "object") throw new TypeError("IntersectionObserver options must be an object");
    const root = options.root === undefined ? null : options.root;
    if (root !== null && ![1, 9].includes(Number(root && root.nodeType))) throw new TypeError("IntersectionObserver root must be an Element or Document");
    const margin = normalizeIntersectionRootMargin(options.rootMargin);
    const thresholds = normalizeIntersectionThresholds(options.threshold);
    Object.defineProperties(this, {{
      __glassCallback: {{ configurable: false, enumerable: false, value: callback }},
      __glassRegistrations: {{ configurable: false, enumerable: false, writable: true, value: [] }},
      __glassRecords: {{ configurable: false, enumerable: false, writable: true, value: [] }},
      root: {{ configurable: false, enumerable: true, value: root }},
      rootMargin: {{ configurable: false, enumerable: true, value: margin.text }},
      thresholds: {{ configurable: false, enumerable: true, value: Object.freeze(thresholds.slice()) }},
    }});
    intersectionObservers.add(this);
  }};
  if (!IntersectionObserverNative.prototype.observe) {{
    Object.defineProperties(IntersectionObserverNative.prototype, {{
      observe: {{
        configurable: true,
        value(target) {{
          if (!target || Number(target.nodeType) !== 1) throw new TypeError("IntersectionObserver target must be an Element");
          const registrations = this.__glassRegistrations;
          const existing = registrations.find((registration) => registration.target === target);
          if (existing) {{
            existing.last = null;
            return;
          }}
          registrations.push({{
            target,
            root: this.root,
            rootMargin: normalizeIntersectionRootMargin(this.rootMargin).values,
            thresholds: this.thresholds.slice(),
            last: null,
          }});
        }},
      }},
      unobserve: {{
        configurable: true,
        value(target) {{
          this.__glassRegistrations = this.__glassRegistrations.filter(
            (registration) => registration.target !== target,
          );
        }},
      }},
      disconnect: {{
        configurable: true,
        value() {{
          this.__glassRegistrations = [];
          this.__glassRecords = [];
        }},
      }},
      takeRecords: {{
        configurable: true,
        value() {{
          const records = this.__glassRecords.slice();
          this.__glassRecords = [];
          return records;
        }},
      }},
    }});
  }}
  globalThis.__glassIntersectionObserverConstructor = IntersectionObserverNative;
  globalThis.IntersectionObserver = IntersectionObserverNative;
  const DOMExceptionNative = globalThis.__glassDOMExceptionConstructor || (() => {{
    const constructor = function DOMException(message = "", name = "Error") {{
      if (!(this instanceof constructor)) throw new TypeError("DOMException requires new");
      const normalizedName = String(name || "Error");
      const normalizedMessage = String(message || "");
      const codes = {{
        IndexSizeError: 1,
        DOMStringSizeError: 2,
        HierarchyRequestError: 3,
        WrongDocumentError: 4,
        InvalidCharacterError: 5,
        NoModificationAllowedError: 7,
        NotFoundError: 8,
        NotSupportedError: 9,
        InvalidStateError: 11,
        SyntaxError: 12,
        InvalidModificationError: 13,
        NamespaceError: 14,
        InvalidAccessError: 15,
        TypeMismatchError: 17,
        SecurityError: 18,
        NetworkError: 19,
        AbortError: 20,
        URLMismatchError: 21,
        QuotaExceededError: 22,
        TimeoutError: 23,
        InvalidNodeTypeError: 24,
        DataCloneError: 25,
      }};
      Object.defineProperties(this, {{
        name: {{ configurable: false, enumerable: true, value: normalizedName }},
        message: {{ configurable: false, enumerable: true, value: normalizedMessage }},
        code: {{ configurable: false, enumerable: true, value: codes[normalizedName] || 0 }},
      }});
    }};
    constructor.prototype = Object.create(Error.prototype);
    Object.defineProperty(constructor.prototype, "constructor", {{
      configurable: true,
      value: constructor,
    }});
    Object.defineProperty(constructor.prototype, "toString", {{
      configurable: true,
      value() {{ return this.name + ": " + this.message; }},
    }});
    return constructor;
  }})();
  globalThis.__glassDOMExceptionConstructor = DOMExceptionNative;
  globalThis.DOMException = DOMExceptionNative;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("document_apis_ready");
