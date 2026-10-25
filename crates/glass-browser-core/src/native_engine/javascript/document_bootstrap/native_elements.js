  const namespaceUriForEntry = (entry) => {{
    if (!Object.prototype.hasOwnProperty.call(entry, "namespaceUri")) return HTML_NAMESPACE;
    if (entry.namespaceUri === null || entry.namespaceUri === "") return null;
    return String(entry.namespaceUri);
  }};
  const snapshotElementTextContent = (entry, exactTextForNode) => {{
    const isHtmlScript = String(entry && entry.tagName || "").toLowerCase() === "script"
      && namespaceUriForEntry(entry) === HTML_NAMESPACE;
    return isHtmlScript && typeof exactTextForNode === "function"
      ? exactTextForNode(entry.nodeIndex)
      : String(entry && entry.text || "");
  }};
  const tagNameForEntry = (entry) => {{
    const tagName = String(entry.tagName || "");
    return namespaceUriForEntry(entry) === HTML_NAMESPACE ? tagName.toLowerCase().toUpperCase() : tagName;
  }};
  const normalizeElementNamespace = (namespace) => {{
    if (namespace === null || namespace === "" || namespace === undefined) return null;
    const value = String(namespace);
    if (![HTML_NAMESPACE, SVG_NAMESPACE, MATHML_NAMESPACE].includes(value)) {{
      throw new DOMExceptionNative("The element namespace is unsupported", "NamespaceError");
    }}
    return value;
  }};
  const normalizeAttributeNamespace = (namespace) => {{
    if (namespace === null || namespace === "" || namespace === undefined) return null;
    const value = String(namespace);
    if (![HTML_NAMESPACE, SVG_NAMESPACE, MATHML_NAMESPACE, XML_NAMESPACE, XMLNS_NAMESPACE, XLINK_NAMESPACE].includes(value)) {{
      throw new DOMExceptionNative("The attribute namespace is unsupported", "NamespaceError");
    }}
    return value;
  }};
  const normalizedAttributeName = (name) => normalizeAttributeNodeName(name);
  const attributeLocalName = (name) => {{
    const qualifiedName = String(name);
    const separator = qualifiedName.indexOf(":");
    return separator < 0 ? qualifiedName : qualifiedName.slice(separator + 1);
  }};
  const storedAttributeLocalName = (name, namespaceURI) =>
    namespaceURI === null ? String(name) : attributeLocalName(name);
  const qualifiedAttributeName = (name, namespace) => {{
    const normalized = normalizedAttributeName(name);
    const separator = normalized.indexOf(":");
    const prefix = separator < 0 ? null : normalized.slice(0, separator);
    if (separator >= 0 && (separator === 0 || separator === normalized.length - 1
        || normalized.indexOf(":", separator + 1) >= 0)) {{
      throw new DOMExceptionNative("The qualified attribute name is invalid", "NamespaceError");
    }}
    if (prefix === "xml" && namespace !== XML_NAMESPACE) {{
      throw new DOMExceptionNative("The xml prefix requires the XML namespace", "NamespaceError");
    }}
    if ((prefix === "xmlns" || normalized === "xmlns") && namespace !== XMLNS_NAMESPACE) {{
      throw new DOMExceptionNative("The xmlns prefix requires the XMLNS namespace", "NamespaceError");
    }}
    if (namespace === XML_NAMESPACE && prefix !== "xml") {{
      throw new DOMExceptionNative("The XML namespace requires the xml prefix", "NamespaceError");
    }}
    if (namespace === XMLNS_NAMESPACE && prefix !== "xmlns" && normalized !== "xmlns") {{
      throw new DOMExceptionNative("The XMLNS namespace requires the xmlns prefix", "NamespaceError");
    }}
    if (namespace === XLINK_NAMESPACE && prefix !== "xlink") {{
      throw new DOMExceptionNative("The XLink namespace requires the xlink prefix", "NamespaceError");
    }}
    return normalized;
  }};
  const attributeNamespaceForEntry = (entry, name) => {{
    const key = normalizedAttributeName(name);
    const namespaces = entry && entry.attributeNamespaces && typeof entry.attributeNamespaces === "object"
      ? entry.attributeNamespaces
      : {{}};
    const value = namespaces[key];
    return value === undefined || value === "" ? null : String(value);
  }};
  const namespaceForChildElement = (parent, localName, forceHtmlRules = false) => {{
    const parentNamespace = parent && parent.namespaceURI;
    const parentName = String(parent && parent.localName || "").toLowerCase();
    const parentIsMathmlAnnotationXml = parentNamespace === MATHML_NAMESPACE
      && parentName === "annotation-xml";
    const encoding = parentIsMathmlAnnotationXml && typeof parent.getAttribute === "function"
      ? String(parent.getAttribute("encoding") || "").toLowerCase()
      : "";
    const usesHtmlRules = forceHtmlRules || !parentNamespace || parentNamespace === HTML_NAMESPACE
      || (parentNamespace === SVG_NAMESPACE
        && ["foreignobject", "desc", "title"].includes(parentName))
      || (parentNamespace === MATHML_NAMESPACE
        && (["mi", "mo", "mn", "ms", "mtext"].includes(parentName)
          && !["mglyph", "malignmark"].includes(localName)
          || parentIsMathmlAnnotationXml
            && (["text/html", "application/xhtml+xml"].includes(encoding) || localName === "svg")));
    if (!usesHtmlRules) return parentNamespace;
    if (localName === "svg") return SVG_NAMESPACE;
    if (localName === "math") return MATHML_NAMESPACE;
    return HTML_NAMESPACE;
  }};
  let captureDynamicCspMetaPolicy = () => {{}};
  let nativeElementBuildPhase = "idle";
  const applyNativeElementScroll = (element, leftCandidate, topCandidate) => {{
    const entry = element.__glassEntry;
    if (!entry) return undefined;
    const isRoot = entry.tagName.toLowerCase() === "html";
    const geometry = geometryForNode(element);
    const clientWidth = isRoot
      ? Number(globalThis.innerWidth) || Number(geometry.clientWidth) || Number(geometry.width) || 0
      : Number(geometry.clientWidth) || Number(geometry.width) || 0;
    const clientHeight = isRoot
      ? Number(globalThis.innerHeight) || Number(geometry.clientHeight) || Number(geometry.height) || 0
      : Number(geometry.clientHeight) || Number(geometry.height) || 0;
    const scrollWidth = isRoot ? Number(state.scrollWidth) || clientWidth : Number(geometry.scrollWidth) || clientWidth;
    const scrollHeight = isRoot ? Number(state.scrollHeight) || clientHeight : Number(geometry.scrollHeight) || clientHeight;
    const currentLeft = isRoot ? Number(state.scrollX) || 0 : Number(element.__glassScrollLeft) || 0;
    const currentTop = isRoot ? Number(state.scrollY) || 0 : Number(element.__glassScrollTop) || 0;
    const coordinate = (candidate, current) => {{
      const numeric = Number(candidate === undefined ? current : candidate);
      return Number.isFinite(numeric) ? Math.trunc(numeric) : 0;
    }};
    const left = Math.max(0, Math.min(Math.max(0, scrollWidth - clientWidth), coordinate(leftCandidate, currentLeft)));
    const top = Math.max(0, Math.min(Math.max(0, scrollHeight - clientHeight), coordinate(topCandidate, currentTop)));
    if (isRoot) {{
      state.scrollX = left;
      state.scrollY = top;
      if (typeof globalThis.scrollX === "number") globalThis.scrollX = left;
      if (typeof globalThis.scrollY === "number") globalThis.scrollY = top;
      if (typeof globalThis.pageXOffset === "number") globalThis.pageXOffset = left;
      if (typeof globalThis.pageYOffset === "number") globalThis.pageYOffset = top;
    }} else {{
      element.__glassScrollLeft = left;
      element.__glassScrollTop = top;
    }}
    if (left !== currentLeft || top !== currentTop) {{
      pushCommand({{ kind: "scrollTo", node_index: isRoot ? 0 : entry.nodeIndex, left, top }});
    }}
    return undefined;
  }};
  const makeElement = (initialEntry, exactTextForNode = null) => {{
    nativeElementBuildPhase = "initialize_entry";
    let entry = initialEntry;
    let textContent = snapshotElementTextContent(entry, exactTextForNode);
    let innerHtml = String(entry.innerHtml || "");
    let templateContent = null;
    let value = entry.value == null
      ? (entry.tagName.toLowerCase() === "option"
        ? (entry.attributes.value === undefined ? String(entry.text || "") : entry.attributes.value)
        : "")
      : entry.value;
    let files = Array.isArray(entry.files) && entry.files.length > 0
      ? makeNativeFileList(entry.files)
      : null;
    let disabled = Boolean(entry.disabled);
    let hidden = Boolean(entry.hidden);
    let multiple = Object.prototype.hasOwnProperty.call(entry.attributes, "multiple");
    let imageComplete = entry.imageComplete === undefined
      ? !Object.prototype.hasOwnProperty.call(entry.attributes, "src")
      : Boolean(entry.imageComplete);
    let imageNaturalWidth = Number(entry.imageNaturalWidth) || 0;
    let imageNaturalHeight = Number(entry.imageNaturalHeight) || 0;
    let imageCurrentSrc = String(entry.imageCurrentSrc || "");
    let mediaReadyState = Number(entry.mediaReadyState) || 0;
    let mediaNetworkState = Number(entry.mediaNetworkState) || 0;
    let mediaCurrentSrc = String(entry.mediaCurrentSrc || "");
    let mediaDurationMillis = entry.mediaDurationMillis === null || entry.mediaDurationMillis === undefined
      ? null
      : Number(entry.mediaDurationMillis);
    let mediaError = entry.mediaError === null || entry.mediaError === undefined
      ? null
      : Number(entry.mediaError);
    let inlineStyleAllowed = entry.inlineStyleAllowed !== false;
    let inlineAttributeHandlers = null;
    let computedStyle = entry.computedStyle && typeof entry.computedStyle === "object"
      ? entry.computedStyle
      : {{}};
    const resetImageState = (complete = false) => {{
      imageComplete = complete;
      imageNaturalWidth = 0;
      imageNaturalHeight = 0;
      imageCurrentSrc = "";
    }};
    const resetMediaState = () => {{
      mediaReadyState = 0;
      mediaNetworkState = Object.prototype.hasOwnProperty.call(entry.attributes, "src") ? 2 : 0;
      mediaCurrentSrc = "";
      mediaDurationMillis = null;
      mediaError = null;
    }};
    if (!entry.attributeNamespaces || typeof entry.attributeNamespaces !== "object") entry.attributeNamespaces = {{}};
    const reconcileNativeFormAssociatedAttribute = (name, namespace) => {{
      if (namespace !== null && namespace !== undefined) return;
      const attributeName = String(name).toLowerCase();
      if (!["form", "id", "disabled"].includes(attributeName)) return;
      const reconcile = globalThis.__glassReconcileNativeFormAssociatedLifecycle;
      if (typeof reconcile === "function") {{
        let root = element;
        while (root && root.__glassParent) root = root.__glassParent;
        reconcile(root || element);
      }}
    }};
    const setNamespacedAttribute = (namespace, name, nextValue) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const qualified = qualifiedAttributeName(name, namespaceURI);
      const value = String(nextValue);
      if (value.length > {storage_value_limit}) throw new RangeError("native attribute value exceeds its limit");
      const oldValue = Object.prototype.hasOwnProperty.call(entry.attributes, qualified)
        ? String(entry.attributes[qualified])
        : null;
      entry.attributes[qualified] = value;
      if (namespaceURI === null) delete entry.attributeNamespaces[qualified];
      else entry.attributeNamespaces[qualified] = namespaceURI;
      element.__glassSyncContent();
      if (typeof element.__glassSyncAttributeNodes === "function") element.__glassSyncAttributeNodes();
      pushCommand({{
        kind: "setAttribute",
        node_index: entry.nodeIndex,
        name: qualified,
        value,
        namespace_uri: namespaceURI === null ? "" : namespaceURI,
      }});
      if (namespaceURI === null) {{
        captureDynamicCspMetaPolicy(element);
        if (applyDynamicReferrerMetaPolicyToNode) applyDynamicReferrerMetaPolicyToNode(element);
      }}
      if (oldValue !== value && typeof globalThis.__glassQueueNativeCustomElementReaction === "function") {{
        globalThis.__glassQueueNativeCustomElementReaction(
          element,
          "attributeChangedCallback",
          [storedAttributeLocalName(qualified, namespaceURI), oldValue, value, namespaceURI],
          true,
        );
      }}
      reconcileNativeFormAssociatedAttribute(storedAttributeLocalName(qualified, namespaceURI), namespaceURI);
    }};
    const namespacedAttributeValue = (namespace, name) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const localName = String(name);
      for (const key of Object.keys(entry.attributes)) {{
        const keyNamespace = attributeNamespaceForEntry(entry, key);
        if (storedAttributeLocalName(key, keyNamespace) === localName
            && keyNamespace === namespaceURI) return entry.attributes[key];
      }}
      return null;
    }};
    const removeNamespacedAttribute = (namespace, name) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const localName = String(name);
      const key = Object.keys(entry.attributes).find((candidate) => {{
        const candidateNamespace = attributeNamespaceForEntry(entry, candidate);
        return storedAttributeLocalName(candidate, candidateNamespace) === localName
          && candidateNamespace === namespaceURI;
      }});
      if (key === undefined) return;
      const oldValue = String(entry.attributes[key]);
      delete entry.attributes[key];
      delete entry.attributeNamespaces[key];
      element.__glassSyncContent();
      if (typeof element.__glassSyncAttributeNodes === "function") element.__glassSyncAttributeNodes();
      pushCommand({{
        kind: "removeAttribute",
        node_index: entry.nodeIndex,
        name: key,
        namespace_uri: namespaceURI === null ? "" : namespaceURI,
      }});
      if (typeof globalThis.__glassQueueNativeCustomElementReaction === "function") {{
        globalThis.__glassQueueNativeCustomElementReaction(
          element,
          "attributeChangedCallback",
          [localName, oldValue, null, namespaceURI],
          true,
        );
      }}
      reconcileNativeFormAssociatedAttribute(localName, namespaceURI);
    }};
    let selectionStart = entry.selectionStart ?? null;
    let selectionEnd = entry.selectionEnd ?? null;
    let selectionDirection = entry.selectionDirection || "none";
    const hasTextSelection = ["input", "textarea"].includes(String(entry.tagName).toLowerCase())
      && selectionStart != null && selectionEnd != null;
    let selectionLength = null;
    let applySelection = null;
    if (hasTextSelection) {{
      selectionLength = () => Array.from(String(value)).length;
      const selectionIndex = (candidate) => {{
        const numeric = Number(candidate);
        if (!Number.isFinite(numeric) || Math.trunc(numeric) !== numeric) {{
          throw new TypeError("selection offset must be a finite integer");
        }}
        return Math.max(0, Math.min(selectionLength(), numeric));
      }};
      applySelection = (start, end, direction) => {{
        if (selectionStart == null || selectionEnd == null) {{
          throw new TypeError("selection is unavailable on this element");
        }}
        const nextStart = selectionIndex(start);
        const nextEnd = selectionIndex(end);
        const normalizedDirection = String(direction).toLowerCase();
        if (!["none", "forward", "backward"].includes(normalizedDirection)) {{
          throw new TypeError("invalid selection direction");
        }}
        selectionStart = Math.min(nextStart, nextEnd);
        selectionEnd = Math.max(nextStart, nextEnd);
        selectionDirection = normalizedDirection;
        pushCommand({{ kind: "setSelection", node_index: entry.nodeIndex, start: selectionStart, end: selectionEnd, direction: selectionDirection }});
      }};
    }}
    nativeElementBuildPhase = "create_element_object";
    const element = {{
      nodeIndex: entry.nodeIndex,
      parentIndex: entry.parentIndex,
      formOwnerIndex: entry.formOwnerIndex,
      tagName: tagNameForEntry(entry),
      nodeType: 1,
      nodeName: tagNameForEntry(entry),
      localName: namespaceUriForEntry(entry) === HTML_NAMESPACE
        ? String(entry.tagName || "").toLowerCase()
        : String(entry.tagName || ""),
      namespaceURI: namespaceUriForEntry(entry),
      value: entry.value == null
        ? (entry.tagName.toLowerCase() === "option"
          ? (entry.attributes.value === undefined ? String(entry.text || "") : entry.attributes.value)
          : "")
        : entry.value,
      get files() {{ return files || (files = makeNativeFileList([])); }},
      checked: Boolean(entry.checked),
      indeterminate: Boolean(entry.indeterminate),
      selected: Boolean(entry.selected),
      multiple,
      disabled,
      hidden,
      focused: Boolean(entry.focused),
      focusAnchorNodeIndex: entry.focusAnchorNodeIndex ?? null,
      hovered: entry.hovered === true,
      userInteracted: entry.userInteracted === true,
      selectionEnd: entry.selectionEnd ?? null,
      selectionDirection: entry.selectionDirection,
      click() {{
        if (this.disabled || this.hidden) return;
        this.focus();
        const event = createEvent("click", {{ bubbles: true, cancelable: true }});
        if (!dispatchTarget(this, event)) return;
        if (this.tagName === "INPUT") {{
          const type = String(entry.attributes.type || "text").toLowerCase();
          if (type === "checkbox") checked = !checked;
          if (type === "radio") checked = true;
        }}
        pushCommand({{ kind: "click", node_index: entry.nodeIndex }});
        nativeActivateResetButton(this, pushCommand);
      }},
      setAttribute(name, value) {{
        const key = String(name).toLowerCase();
        const stringValue = String(value);
        const oldValue = Object.prototype.hasOwnProperty.call(entry.attributes, key)
          ? String(entry.attributes[key])
          : null;
        entry.attributes[key] = stringValue;
        delete entry.attributeNamespaces[key];
        installInlineAttributeHandler(key, stringValue);
        if (["src", "srcset", "sizes"].includes(key) && element.tagName === "IMG") resetImageState(stringValue === "" && key === "src");
        if ((element.tagName === "AUDIO" || element.tagName === "VIDEO") && key === "src") resetMediaState();
        if (element.tagName === "SOURCE" && ["src", "type"].includes(key)
            && element.__glassParent && typeof element.__glassParent.__glassMediaReset === "function") element.__glassParent.__glassMediaReset();
        if (key === "disabled") disabled = true;
        if (key === "hidden") hidden = true;
        if (key === "multiple") multiple = true;
        element.__glassSyncContent();
        pushCommand({{ kind: "setAttribute", node_index: entry.nodeIndex, name: key, value: stringValue, namespace_uri: "" }});
        captureDynamicCspMetaPolicy(element);
        if (applyDynamicReferrerMetaPolicyToNode) applyDynamicReferrerMetaPolicyToNode(element);
        if (oldValue !== stringValue && typeof globalThis.__glassQueueNativeCustomElementReaction === "function") {{
          globalThis.__glassQueueNativeCustomElementReaction(
            element,
            "attributeChangedCallback",
            [key, oldValue, stringValue, null],
            true,
          );
        }}
        reconcileNativeFormAssociatedAttribute(key, null);
      }},
      __glassSetParsedAttribute(name, value, namespace) {{
        const qualifiedName = String(name);
        const namespaceURI = namespace === null || namespace === undefined ? null : String(namespace);
        const stringValue = String(value);
        if (stringValue.length > {storage_value_limit}) throw new RangeError("native parsed attribute value exceeds its limit");
        const oldValue = Object.prototype.hasOwnProperty.call(entry.attributes, qualifiedName)
          ? String(entry.attributes[qualifiedName])
          : null;
        entry.attributes[qualifiedName] = stringValue;
        if (namespaceURI === null) delete entry.attributeNamespaces[qualifiedName];
        else entry.attributeNamespaces[qualifiedName] = namespaceURI;
        const key = qualifiedName.toLowerCase();
        installInlineAttributeHandler(key, stringValue);
        if (["src", "srcset", "sizes"].includes(key) && element.tagName === "IMG") {{
          resetImageState(stringValue === "" && key === "src");
        }}
        if ((element.tagName === "AUDIO" || element.tagName === "VIDEO") && key === "src") resetMediaState();
        if (element.tagName === "SOURCE" && ["src", "type"].includes(key)
            && element.__glassParent && typeof element.__glassParent.__glassMediaReset === "function") {{
          element.__glassParent.__glassMediaReset();
        }}
        if (key === "disabled") disabled = true;
        if (key === "hidden") hidden = true;
        if (key === "multiple") multiple = true;
        element.__glassSyncContent();
        pushCommand({{
          kind: "setAttribute",
          node_index: entry.nodeIndex,
          name: qualifiedName,
          value: stringValue,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
        if (oldValue !== stringValue
            && typeof globalThis.__glassQueueNativeCustomElementReaction === "function") {{
          globalThis.__glassQueueNativeCustomElementReaction(
            element,
            "attributeChangedCallback",
            [storedAttributeLocalName(qualifiedName, namespaceURI), oldValue, stringValue, namespaceURI],
            true,
          );
        }}
        reconcileNativeFormAssociatedAttribute(
          storedAttributeLocalName(qualifiedName, namespaceURI), namespaceURI,
        );
      }},
      setAttributeNS(namespace, qualifiedName, value) {{
        setNamespacedAttribute(namespace, qualifiedName, value);
      }},
      removeAttribute(name) {{
        const key = String(name).toLowerCase();
        const oldValue = Object.prototype.hasOwnProperty.call(entry.attributes, key)
          ? String(entry.attributes[key])
          : null;
        delete entry.attributes[key];
        delete entry.attributeNamespaces[key];
        removeInlineAttributeHandler(key, oldValue !== null);
        if (["src", "srcset", "sizes"].includes(key) && element.tagName === "IMG") resetImageState(key === "src");
        if ((element.tagName === "AUDIO" || element.tagName === "VIDEO") && key === "src") resetMediaState();
        if (element.tagName === "SOURCE" && ["src", "type"].includes(key)
            && element.__glassParent && typeof element.__glassParent.__glassMediaReset === "function") element.__glassParent.__glassMediaReset();
        if (key === "disabled") disabled = false;
        if (key === "hidden") hidden = false;
        if (key === "multiple") multiple = false;
        element.__glassSyncContent();
        pushCommand({{ kind: "removeAttribute", node_index: entry.nodeIndex, name: key, namespace_uri: "" }});
        if (oldValue !== null && typeof globalThis.__glassQueueNativeCustomElementReaction === "function") {{
          globalThis.__glassQueueNativeCustomElementReaction(
            element,
            "attributeChangedCallback",
            [key, oldValue, null, null],
            true,
          );
        }}
        reconcileNativeFormAssociatedAttribute(key, null);
      }},
      removeAttributeNS(namespace, name) {{
        removeNamespacedAttribute(namespace, name);
      }},
    }};
    nativeElementBuildPhase = "bind_entry_and_handlers";
    Object.defineProperty(element, "__glassEntry", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: entry,
    }});
    Object.defineProperty(element, "__glassScrollLeft", {{
      enumerable: false, configurable: false, writable: true, value: Number(entry.scrollX) || 0,
    }});
    Object.defineProperty(element, "__glassScrollTop", {{
      enumerable: false, configurable: false, writable: true, value: Number(entry.scrollY) || 0,
    }});
    Object.defineProperty(element, "__glassCustomElementIs", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: Object.prototype.hasOwnProperty.call(entry.attributes || {{}}, "is")
        ? String(entry.attributes.is)
        : null,
    }});
    Object.defineProperty(element, "__glassSetNativeIndex", {{
      enumerable: false,
      configurable: false,
      value(nextIndex) {{
        const index = Number(nextIndex);
        if (!Number.isSafeInteger(index) || index < 0) throw new TypeError("native node index is invalid");
        entry = {{ ...entry, nodeIndex: index }};
        element.nodeIndex = index;
      }},
    }});
    nativeElementBuildPhase = "install_inline_event_helpers";
    const inlineEventType = (name) => {{
      const value = String(name).toLowerCase();
      if (!value.startsWith("on") || value.length <= 2 || !/^[a-z][a-z0-9]*$/.test(value.slice(2))) return null;
      try {{ return normalizeEventType(value.slice(2)); }} catch (_error) {{ return null; }}
    }};
    const inlineAttributeOwner = () => ownerFor(element);
    const inlineAttributeWindowTarget = (name, type) => {{
      if (element.namespaceURI !== HTML_NAMESPACE) return null;
      const attributeName = String(name).toLowerCase();
      for (const [qualifiedName, namespace] of Object.entries(entry.attributeNamespaces || {{}})) {{
        if (qualifiedName.toLowerCase() === attributeName && namespace !== null && namespace !== undefined) return null;
      }}
      const target = nativeEventHandlerTargetFor(element, type);
      return target && target !== element ? target : null;
    }};
    const removeInlineAttributeHandler = (name, deactivateShared = true) => {{
      const type = inlineEventType(name);
      if (!type) return;
      const record = inlineAttributeHandlers && inlineAttributeHandlers.get(type);
      const owner = inlineAttributeOwner();
      const stableKey = listenerKey(owner, type);
      const persisted = inlineAttributeListeners.get(stableKey);
      if (record && record.shared) {{
        if (deactivateShared) setEventHandlerValue(record.target, type, null, false);
      }} else if (record) {{
        removeListener(owner, type, record.registered, false);
      }} else if (persisted) {{
        removeListener(persisted.owner, type, persisted.registered, false);
      }} else {{
        const target = inlineAttributeWindowTarget(name, type);
        if (deactivateShared && target) setEventHandlerValue(target, type, null, false);
      }}
      inlineAttributeListeners.delete(stableKey);
      if (inlineAttributeHandlers) inlineAttributeHandlers.delete(type);
    }};
    const installInlineAttributeHandler = (name, source) => {{
      const type = inlineEventType(name);
      if (!type) return;
      const value = String(source === null || source === undefined ? "" : source);
      const target = inlineAttributeWindowTarget(name, type);
      const previous = inlineAttributeHandlers && inlineAttributeHandlers.get(type);
      if (target) {{
        let allowed = true;
        if (typeof globalThis.__glassAllowsInlineEventHandler === "function") {{
          try {{ allowed = Boolean(globalThis.__glassAllowsInlineEventHandler(value)); }} catch (_error) {{ allowed = false; }}
        }}
        if (!allowed) {{
          if (previous && !previous.shared) {{
            const owner = inlineAttributeOwner();
            removeListener(owner, type, previous.registered, false);
            inlineAttributeListeners.delete(listenerKey(owner, type));
          }}
          if (previous && previous.shared)
            setEventHandlerValue(previous.target, type, null, false);
          else if (!previous && eventHandlerStateFor(target, type).state?.contentAttribute)
            setEventHandlerValue(target, type, null, false);
          if (!inlineAttributeHandlers) inlineAttributeHandlers = new Map();
          inlineAttributeHandlers.set(type, {{
            source: value,
            shared: true,
            target,
            handler: previous && previous.shared && previous.target === target ? previous.handler : null,
          }});
          return;
        }}
        let handler;
        try {{
          handler = type === "error"
            ? Function("event", "source", "lineno", "colno", "error", value)
            : Function("event", value);
        }} catch (_error) {{
          setEventHandlerValue(target, type, null, false);
          if (!inlineAttributeHandlers) inlineAttributeHandlers = new Map();
          inlineAttributeHandlers.set(type, {{ source: value, shared: true, target, handler: null }});
          return;
        }}
        if (previous && !previous.shared) {{
          const owner = inlineAttributeOwner();
          removeListener(owner, type, previous.registered, false);
          inlineAttributeListeners.delete(listenerKey(owner, type));
        }}
        if (previous && previous.shared && previous.target !== target)
          setEventHandlerValue(previous.target, type, null, false);
        setEventHandlerValue(target, type, handler, true);
        if (!inlineAttributeHandlers) inlineAttributeHandlers = new Map();
        inlineAttributeHandlers.set(type, {{ source: value, shared: true, target, handler }});
        return;
      }}
      removeInlineAttributeHandler(name);
      if (!value) return;
      let allowed = true;
      if (typeof globalThis.__glassAllowsInlineEventHandler === "function") {{
        try {{ allowed = Boolean(globalThis.__glassAllowsInlineEventHandler(value)); }} catch (_error) {{ allowed = false; }}
      }}
      if (!allowed) return;
      let handler;
      try {{ handler = Function("event", value); }} catch (_error) {{ return; }}
      const registered = (event) => {{
        try {{
          if (handler.call(element, event) !== false || !event) return;
          if (type === "beforeunload") {{
            if (typeof event.preventDefault === "function") event.preventDefault();
          }} else {{
            event.defaultPrevented = true;
          }}
        }} catch (_error) {{}}
      }};
      const owner = inlineAttributeOwner();
      if (!inlineAttributeHandlers) inlineAttributeHandlers = new Map();
      inlineAttributeHandlers.set(type, {{ source: value, shared: false, owner, registered }});
      inlineAttributeListeners.set(listenerKey(owner, type), {{ source: value, owner, registered }});
      addListener(owner, type, registered, false);
    }};
    const clearInlineAttributeHandlers = () => {{
      for (const [type, record] of inlineAttributeHandlers || []) {{
        if (!record.shared) {{
          const owner = inlineAttributeOwner();
          removeListener(owner, type, record.registered, false);
          inlineAttributeListeners.delete(listenerKey(owner, type));
        }}
      }}
      const owner = inlineAttributeOwner();
      for (const [key, record] of Array.from(inlineAttributeListeners.entries())) {{
        if (record.owner !== owner) continue;
        removeListener(record.owner, key.slice(owner.length + 1), record.registered, false);
        inlineAttributeListeners.delete(key);
      }}
      if (inlineAttributeHandlers) inlineAttributeHandlers.clear();
      inlineAttributeHandlers = null;
    }};
    const refreshInlineAttributeHandlers = () => {{
      const attributes = entry.attributes || {{}};
      if (inlineAttributeHandlers) for (const type of inlineAttributeHandlers.keys()) {{
        let found = false;
        for (const name in attributes) {{
          if (!Object.prototype.hasOwnProperty.call(attributes, name)) continue;
          if (inlineEventType(name) === type) {{ found = true; break; }}
        }}
        if (!found) removeInlineAttributeHandler("on" + type);
      }}
      for (const name in attributes) {{
        if (!Object.prototype.hasOwnProperty.call(attributes, name)) continue;
        const value = attributes[name];
        const type = inlineEventType(name);
        if (!type) continue;
        const attributeValue = String(value);
        const target = inlineAttributeWindowTarget(name, type);
        const record = inlineAttributeHandlers && inlineAttributeHandlers.get(type);
        if (record
            && record.source === attributeValue
            && record.shared === Boolean(target)
            && (!target || record.target === target)) continue;
        installInlineAttributeHandler(name, attributeValue);
      }}
    }};
    Object.defineProperty(element, "__glassChildren", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: [],
    }});
    Object.defineProperty(element, "__glassParent", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: null,
    }});
    Object.defineProperty(element, "__glassCreated", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: false,
    }});
    Object.defineProperty(element, "__glassSyncContent", {{
      enumerable: false,
      configurable: false,
      value(clearEmpty = false) {{
        if (element.__glassChildren.length > 0) {{
          innerHtml = element.__glassChildren.map(child => child.__glassMarkup).join("");
          textContent = element.__glassChildren.map(child => child.__glassTextValue).join("");
        }} else if (clearEmpty) {{
          innerHtml = "";
          textContent = "";
        }}
        if (element.__glassParent && typeof element.__glassParent.__glassSyncContent === "function") element.__glassParent.__glassSyncContent(clearEmpty);
      }},
    }});
    Object.defineProperty(element, "__glassTextValue", {{
      enumerable: false,
      configurable: false,
      get() {{ return textContent; }},
    }});
    Object.defineProperty(element, "__glassMarkup", {{
      enumerable: false,
      configurable: false,
      get() {{
        const names = Object.keys(entry.attributes);
        if (element.__glassCustomElementIs !== null && !names.includes("is")) names.push("is");
        const attributes = names
          .sort()
          .map(name => {{
            const value = Object.prototype.hasOwnProperty.call(entry.attributes, name)
              ? entry.attributes[name]
              : element.__glassCustomElementIs;
            return " " + name + "=\"" + escapeHtmlText(value).replace(/\"/g, "&quot;") + "\"";
          }})
          .join("");
        const opening = "<" + element.localName + attributes + ">";
        if (["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"].includes(element.localName)) return opening;
        const content = element.localName === "template"
          ? element.content.innerHTML
          : element.__glassChildren.length > 0
          ? element.__glassChildren.map(child => child.__glassMarkup).join("")
          : innerHtml;
        return opening + content + "</" + element.localName + ">";
      }},
    }});
    Object.defineProperty(element, "outerHTML", {{
      enumerable: true,
      configurable: false,
      get() {{ return element.__glassMarkup; }},
      set(next) {{
        const value = String(next);
        if (value.length > storageValueLimit) throw new RangeError("native element outerHTML exceeds its limit");
        const owner = element.__glassParent || null;
        if (!owner || typeof owner.insertBefore !== "function") throw new TypeError("outerHTML requires an attached element");
        const fragment = document.createDocumentFragment();
        populateDetachedFragment(fragment, value, makeDetachedElement, makeDetachedText, makeDetachedComment, owner);
        for (const child of fragment.__glassChildren.slice()) owner.insertBefore(child, element);
        element.remove();
      }},
    }});
    installAttributeNodeSurface(element, () => globalThis.document || null);
    installCommonAttributeProperties(element, {{
      disabled: () => disabled,
      hidden: () => hidden,
      multiple: () => multiple,
      imageComplete: () => imageComplete,
      imageNaturalWidth: () => imageNaturalWidth,
      imageNaturalHeight: () => imageNaturalHeight,
      imageCurrentSrc: () => imageCurrentSrc,
      imageReset: resetImageState,
      mediaReadyState: () => mediaReadyState,
      mediaNetworkState: () => mediaNetworkState,
      mediaCurrentSrc: () => mediaCurrentSrc,
      mediaDurationMillis: () => mediaDurationMillis,
      mediaError: () => mediaError,
      mediaReset: resetMediaState,
      mediaLoad: () => pushCommand({{ kind: "mediaLoad", node_index: entry.nodeIndex }}),
    }});
    refreshInlineAttributeHandlers();
    nativeCanvasInstallElement(element, entry);
    Object.defineProperty(element, "__glassAttributeSource", {{
      enumerable: false,
      configurable: false,
      value: () => entry.attributes,
    }});
    Object.defineProperty(element, "__glassAttributeNamespace", {{
      enumerable: false,
      configurable: false,
      value: (name) => attributeNamespaceForEntry(entry, name),
    }});
    for (const property of ["textContent", "innerText"]) {{
      Object.defineProperty(element, property, {{
        enumerable: true,
        configurable: false,
        get() {{ return textContent; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native element textContent exceeds its limit");
          const oldChildren = element.__glassChildren.slice();
          const wasConnected = nodeIsConnected(element);
          for (const child of element.__glassChildren) {{
            child.__glassParent = null;
            child.parentIndex = null;
          }}
          element.__glassChildren = [];
          innerHtml = "";
          textContent = "";
          suppressHostCommands += 1;
          try {{
            if (value) element.appendChild(makeDetachedText(value));
          }} finally {{
            suppressHostCommands -= 1;
          }}
          element.__glassSyncContent(true);
          pushCommand({{ kind: "setTextContent", node_index: entry.nodeIndex, value }});
          if (wasConnected && typeof globalThis.__glassRunNativeCustomElementRemovalReactions === "function") {{
            for (const child of oldChildren) globalThis.__glassRunNativeCustomElementRemovalReactions(child);
          }}
        }},
      }});
    }}
    if (element.namespaceURI === HTML_NAMESPACE && element.localName === "template") {{
      Object.defineProperty(element, "content", {{
        enumerable: true,
        configurable: false,
        get() {{
          const contentIndex = entry.templateContentIndex;
          const current = globalThis.__glassHostNodes;
          const cache = globalThis.__glassTemplateContents instanceof Map
            ? globalThis.__glassTemplateContents
            : (globalThis.__glassTemplateContents = new Map());
          if (!templateContent && contentIndex !== null && contentIndex !== undefined
              && current instanceof Map && current.has(Number(contentIndex))) {{
            templateContent = current.get(Number(contentIndex));
          }}
          if (!templateContent && cache.has(Number(entry.nodeIndex))) {{
            templateContent = cache.get(Number(entry.nodeIndex));
          }}
          if (!templateContent) templateContent = makeDocumentFragment(null, element);
          templateContent.__glassTemplateHost = element;
          cache.set(Number(entry.nodeIndex), templateContent);
          return templateContent;
        }},
      }});
    }}
    Object.defineProperty(element, "innerHTML", {{
      enumerable: true,
      configurable: false,
      get() {{ return element.localName === "template" ? element.content.innerHTML : innerHtml; }},
      set(next) {{
        const value = String(next);
        if (value.length > storageValueLimit) throw new RangeError("native element innerHTML exceeds its limit");
        if (element.localName === "template") {{
          const content = element.content;
          for (const child of content.__glassChildren) {{
            child.__glassParent = null;
            child.parentIndex = null;
          }}
          content.__glassChildren = [];
          innerHtml = "";
          textContent = "";
          suppressHostCommands += 1;
          try {{
            const populate = () => populateDetachedFragment(
              content,
              value,
              makeDetachedElement,
              makeDetachedText,
              makeDetachedComment,
            );
            const suppressReactions = globalThis.__glassWithNativeCustomElementReactionsSuppressed;
            if (typeof suppressReactions === "function") suppressReactions(populate);
            else populate();
          }} finally {{
            suppressHostCommands -= 1;
          }}
          innerHtml = value;
          element.__glassSyncContent(true);
          pushCommand({{ kind: "setInnerHtml", node_index: entry.nodeIndex, value }});
          const nativeInnerHtmlNodeIndexes = globalThis.__glassNativeInnerHtmlNodeIndexes;
          if (typeof nativeInnerHtmlNodeIndexes === "function") {{
            const temporaryNodeIndexes = nativeInnerHtmlNodeIndexes(element);
            if (temporaryNodeIndexes.length > 0) {{
              pushCommand({{
                kind: "bindInnerHtmlNodeIndexes",
                root_index: entry.nodeIndex,
                temporary_node_indexes: temporaryNodeIndexes,
              }});
            }}
          }}
          return;
        }}
        const oldChildren = element.__glassChildren.slice();
        const wasConnected = nodeIsConnected(element);
        for (const child of element.__glassChildren) {{
          child.__glassParent = null;
          child.parentIndex = null;
        }}
        element.__glassChildren = [];
        innerHtml = "";
        textContent = "";
        suppressHostCommands += 1;
        try {{
          const populate = () => populateDetachedFragment(
            element,
            value,
            makeDetachedElement,
            makeDetachedText,
            makeDetachedComment,
          );
          const suppressReactions = globalThis.__glassWithNativeCustomElementReactionsSuppressed;
          if (typeof suppressReactions === "function") suppressReactions(populate);
          else populate();
        }} finally {{
          suppressHostCommands -= 1;
        }}
        element.__glassSyncContent(true);
        pushCommand({{ kind: "setInnerHtml", node_index: entry.nodeIndex, value }});
        const nativeInnerHtmlNodeIndexes = globalThis.__glassNativeInnerHtmlNodeIndexes;
        if (typeof nativeInnerHtmlNodeIndexes === "function") {{
          const temporaryNodeIndexes = nativeInnerHtmlNodeIndexes(element);
          if (temporaryNodeIndexes.length > 0) {{
            pushCommand({{
              kind: "bindInnerHtmlNodeIndexes",
              root_index: entry.nodeIndex,
              temporary_node_indexes: temporaryNodeIndexes,
            }});
          }}
        }}
        if (wasConnected && typeof globalThis.__glassRunNativeCustomElementRemovalReactions === "function") {{
          for (const child of oldChildren) globalThis.__glassRunNativeCustomElementRemovalReactions(child);
        }}
        if (element.__glassChildren.length > 0
            && typeof globalThis.__glassRunNativeCustomElementInsertionReactions === "function") {{
          for (const child of element.__glassChildren) {{
            globalThis.__glassRunNativeCustomElementInsertionReactions(
              child,
              false,
              nodeIsConnected(element),
            );
          }}
        }}
      }},
    }});
    value = element.value;
    Object.defineProperty(element, "value", {{
      enumerable: true,
      configurable: false,
      get() {{ return value; }},
      set(next) {{
        const nextValue = String(next);
        const inputType = entry.tagName.toLowerCase() === "input"
          ? String(entry.attributes.type || "text").toLowerCase()
          : "";
        if (inputType === "file") {{
          if (nextValue !== "") throw new DOMExceptionNative("This input element accepts a filename only when selected by the user", "InvalidStateError");
          value = "";
          files = makeNativeFileList([]);
          pushCommand({{ kind: "clearFileInput", node_index: entry.nodeIndex }});
          return;
        }}
        value = nextValue;
        if (selectionStart !== null) {{
          selectionStart = selectionLength();
          selectionEnd = selectionStart;
          selectionDirection = "none";
        }}
        pushCommand({{ kind: "setValue", node_index: entry.nodeIndex, value }});
      }}
    }});
    if (hasTextSelection) {{
      Object.defineProperty(element, "__glassApplySelection", {{
        enumerable: false,
        configurable: false,
        value: applySelection,
      }});
      Object.defineProperty(element, "selectionStart", {{
        enumerable: true,
        configurable: false,
        get() {{ return selectionStart ?? entry.selectionStart ?? null; }},
        set(next) {{ applySelection(next, selectionEnd, selectionDirection); }},
      }});
      Object.defineProperty(element, "selectionEnd", {{
        enumerable: true,
        configurable: false,
        get() {{ return selectionEnd ?? entry.selectionEnd ?? null; }},
        set(next) {{ applySelection(selectionStart, next, selectionDirection); }},
      }});
      Object.defineProperty(element, "selectionDirection", {{
        enumerable: true,
        configurable: false,
        get() {{ return selectionDirection ?? entry.selectionDirection ?? "none"; }},
        set(next) {{ applySelection(selectionStart, selectionEnd, next); }},
      }});
    }}
    let checked = element.checked;
    Object.defineProperty(element, "checked", {{
      enumerable: true,
      configurable: false,
      get() {{ return checked; }},
      set(next) {{
        checked = Boolean(next);
        pushCommand({{ kind: "setChecked", node_index: entry.nodeIndex, checked }});
      }}
    }});
    let indeterminate = Boolean(element.indeterminate);
    Object.defineProperty(element, "indeterminate", {{
      enumerable: true,
      configurable: false,
      get() {{ return indeterminate; }},
      set(next) {{
        if (entry.tagName.toLowerCase() !== "input") throw new TypeError("indeterminate requires an input element");
        indeterminate = Boolean(next);
        pushCommand({{ kind: "setIndeterminate", node_index: entry.nodeIndex, indeterminate }});
      }}
    }});
    let selected = element.selected;
    Object.defineProperty(element, "selected", {{
      enumerable: true,
      configurable: false,
      get() {{ return selected; }},
      set(next) {{
        selected = Boolean(next);
        pushCommand({{ kind: "setSelected", node_index: entry.nodeIndex, selected }});
      }}
    }});
    Object.defineProperty(element, "__glassSetFormResetState", {{
      enumerable: false,
      configurable: false,
      value(next) {{
        if (Object.prototype.hasOwnProperty.call(next, "value")) value = String(next.value);
        if (Object.prototype.hasOwnProperty.call(next, "checked")) checked = Boolean(next.checked);
        if (Object.prototype.hasOwnProperty.call(next, "selected")) selected = Boolean(next.selected);
        if (next.clearFiles) files = makeNativeFileList([]);
        if (selectionStart !== null) selectionStart = 0;
        if (selectionEnd !== null) selectionEnd = 0;
        if (selectionDirection !== null) selectionDirection = "none";
      }},
    }});
    nativeElementBuildPhase = "install_refresh_method";
    Object.defineProperty(element, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry, nextExactTextForNode = exactTextForNode) {{
        entry = nextEntry;
        element.__glassEntry = nextEntry;
        computedStyle = nextEntry.computedStyle && typeof nextEntry.computedStyle === "object"
          ? nextEntry.computedStyle
          : {{}};
        inlineStyleAllowed = nextEntry.inlineStyleAllowed !== false;
        if (!entry.attributeNamespaces || typeof entry.attributeNamespaces !== "object") entry.attributeNamespaces = {{}};
        element.nodeIndex = nextEntry.nodeIndex;
        element.parentIndex = nextEntry.parentIndex;
        element.tagName = tagNameForEntry(nextEntry);
        element.nodeName = tagNameForEntry(nextEntry);
        element.localName = nextEntry.tagName.toLowerCase();
        textContent = snapshotElementTextContent(nextEntry, nextExactTextForNode);
        innerHtml = String(nextEntry.innerHtml || "");
        disabled = Boolean(nextEntry.disabled);
        hidden = Boolean(nextEntry.hidden);
        element.focused = nextEntry.focused;
        element.focusAnchorNodeIndex = nextEntry.focusAnchorNodeIndex ?? null;
        element.hovered = nextEntry.hovered === true;
        element.userInteracted = nextEntry.userInteracted === true;
        value = nextEntry.value === null
          ? (nextEntry.tagName.toLowerCase() === "option"
            ? (nextEntry.attributes.value === undefined ? nextEntry.text : nextEntry.attributes.value)
            : "")
          : nextEntry.value;
        selectionStart = nextEntry.selectionStart;
        selectionEnd = nextEntry.selectionEnd;
        selectionDirection = nextEntry.selectionDirection || "none";
        checked = nextEntry.checked;
        indeterminate = Boolean(nextEntry.indeterminate);
        selected = nextEntry.selected;
        files = Array.isArray(nextEntry.files) && nextEntry.files.length > 0
          ? makeNativeFileList(nextEntry.files)
          : files && files.length === 0 ? files : null;
        imageComplete = nextEntry.imageComplete === undefined
          ? !Object.prototype.hasOwnProperty.call(nextEntry.attributes, "src")
          : Boolean(nextEntry.imageComplete);
        imageNaturalWidth = Number(nextEntry.imageNaturalWidth) || 0;
        imageNaturalHeight = Number(nextEntry.imageNaturalHeight) || 0;
        imageCurrentSrc = String(nextEntry.imageCurrentSrc || "");
        mediaReadyState = Number(nextEntry.mediaReadyState) || 0;
        mediaNetworkState = Number(nextEntry.mediaNetworkState) || 0;
        mediaCurrentSrc = String(nextEntry.mediaCurrentSrc || "");
        mediaDurationMillis = nextEntry.mediaDurationMillis === null || nextEntry.mediaDurationMillis === undefined
          ? null
          : Number(nextEntry.mediaDurationMillis);
        mediaError = nextEntry.mediaError === null || nextEntry.mediaError === undefined
          ? null
          : Number(nextEntry.mediaError);
        element.__glassScrollLeft = Number(nextEntry.scrollX) || 0;
        element.__glassScrollTop = Number(nextEntry.scrollY) || 0;
        refreshInlineAttributeHandlers();
        if (typeof element.__glassSyncAttributeNodes === "function") element.__glassSyncAttributeNodes();
      }}
    }});
    nativeElementBuildPhase = "complete";
    return element;
  }};
  const nativeElementSharedMethodProperties = {{
    getAttribute: {{ configurable: true, writable: true, value(name) {{
      const entry = this.__glassEntry;
      const htmlElement = this.namespaceURI === HTML_NAMESPACE;
      const key = htmlElement ? String(name).toLowerCase() : String(name);
      for (const attr of Object.keys(entry.attributes || {{}})) {{
        if (htmlElement ? attr.toLowerCase() === key : attr === key) return entry.attributes[attr];
      }}
      return null;
    }} }},
    getAttributeNS: {{ configurable: true, writable: true, value(namespace, name) {{
      const entry = this.__glassEntry;
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const localName = String(name);
      for (const key of Object.keys(entry.attributes || {{}})) {{
        const keyNamespace = attributeNamespaceForEntry(entry, key);
        if (storedAttributeLocalName(key, keyNamespace) === localName
            && keyNamespace === namespaceURI) return entry.attributes[key];
      }}
      return null;
    }} }},
    getAttributeNames: {{ configurable: true, writable: true, value() {{
      return Object.keys(this.__glassEntry.attributes || {{}});
    }} }},
    hasAttribute: {{ configurable: true, writable: true, value(name) {{
      return this.getAttribute(name) !== null;
    }} }},
    hasAttributeNS: {{ configurable: true, writable: true, value(namespace, name) {{
      return this.getAttributeNS(namespace, name) !== null;
    }} }},
    matches: {{ configurable: true, writable: true, value(selector) {{
      return matchesSelector(this, selector);
    }} }},
    closest: {{ configurable: true, writable: true, value(selector) {{
      let current = this;
      while (current) {{
        if (matchesSelector(current, selector)) return current;
        current = current.parentElement;
      }}
      return null;
    }} }},
    querySelector: {{ configurable: true, writable: true, value(selector) {{
      return descendantsInTree(this, (candidate) => matchesSelector(candidate, selector, this))[0] || null;
    }} }},
    querySelectorAll: {{ configurable: true, writable: true, value(selector) {{
      return asNodeList(descendantsInTree(this, (candidate) => matchesSelector(candidate, selector, this)));
    }} }},
    getElementsByTagName: {{ configurable: true, writable: true, value(name) {{
      const value = String(name).toLowerCase();
      return asHtmlCollection(descendantsInTree(this, (candidate) =>
        value === "*" || candidate.tagName.toLowerCase() === value));
    }} }},
    getElementsByClassName: {{ configurable: true, writable: true, value(name) {{
      const value = String(name).trim();
      if (!value) return asHtmlCollection([]);
      const tokens = value.split(/\s+/);
      return asHtmlCollection(descendantsInTree(this, (candidate) =>
        tokens.every((token) => String(candidate.className).split(/\s+/).includes(token))));
    }} }},
    setSelectionRange: {{ configurable: true, writable: true, value(start, end, direction = "none") {{
      if (typeof this.__glassApplySelection !== "function")
        throw new TypeError("selection is unavailable on this element");
      return this.__glassApplySelection(start, end, direction);
    }} }},
    select: {{ configurable: true, writable: true, value() {{
      if (typeof this.__glassApplySelection !== "function")
        throw new TypeError("selection is unavailable on this element");
      return this.__glassApplySelection(0, Array.from(String(this.value || "")).length, "forward");
    }} }},
    getBoundingClientRect: {{ configurable: true, writable: true, value() {{
      return makeDomRect(geometryForNode(this));
    }} }},
    getClientRects: {{ configurable: true, writable: true, value() {{
      const geometry = geometryForNode(this);
      return geometry.width > 0 && geometry.height > 0
        ? asNodeList([makeDomRect(geometry)])
        : asNodeList([]);
    }} }},
    clientWidth: {{ configurable: true, get() {{
      const geometry = geometryForNode(this);
      return Number(geometry.clientWidth) || Number(geometry.width) || 0;
    }} }},
    clientHeight: {{ configurable: true, get() {{
      const geometry = geometryForNode(this);
      return Number(geometry.clientHeight) || Number(geometry.height) || 0;
    }} }},
    offsetWidth: {{ configurable: true, get() {{ return Number(geometryForNode(this).width) || 0; }} }},
    offsetHeight: {{ configurable: true, get() {{ return Number(geometryForNode(this).height) || 0; }} }},
    scrollWidth: {{ configurable: true, get() {{
      const geometry = geometryForNode(this);
      return Number(geometry.scrollWidth) || Number(geometry.width) || 0;
    }} }},
    scrollHeight: {{ configurable: true, get() {{
      const geometry = geometryForNode(this);
      return Number(geometry.scrollHeight) || Number(geometry.height) || 0;
    }} }},
    addEventListener: {{ configurable: true, writable: true, value(type, callback, options) {{
      addListener(ownerFor(this), type, callback, options);
    }} }},
    removeEventListener: {{ configurable: true, writable: true, value(type, callback, options) {{
      removeListener(ownerFor(this), type, callback, options);
    }} }},
    dispatchEvent: {{ configurable: true, writable: true, value(event) {{
      return dispatchTarget(this, event);
    }} }},
    focus: {{ configurable: true, writable: true, value() {{
      if (this.disabled || this.hidden || nativeFormAssociatedDisabledForFocus(this)
          || nativeInertForFocus(this)) return;
      setLocalFocus(this);
    }} }},
    blur: {{ configurable: true, writable: true, value() {{
      if (!this.focused) return;
      this.focused = false;
      pushCommand({{ kind: "blur", node_index: this.__glassEntry.nodeIndex }});
      dispatchTarget(this, createFocusEvent("blur", {{ relatedTarget: null }}));
      dispatchTarget(this, createFocusEvent("focusout", {{ bubbles: true, relatedTarget: null }}));
    }} }},
    submit: {{ configurable: true, writable: true, value() {{
      if (this.tagName !== "FORM") throw new TypeError("submit requires a form");
      pushCommand({{ kind: "submitForm", node_index: this.__glassEntry.nodeIndex }});
    }} }},
    requestSubmit: {{ configurable: true, writable: true, value() {{
      if (this.tagName !== "FORM") throw new TypeError("requestSubmit requires a form");
      const submitter = arguments.length === 0 ? null : arguments[0];
      if (submitter !== null && (typeof submitter !== "object" || typeof submitter.nodeIndex !== "number"))
        throw new TypeError("requestSubmit submitter must be an element");
      pushCommand({{ kind: "requestSubmitForm", node_index: this.__glassEntry.nodeIndex, submitter_index: submitter === null ? null : submitter.nodeIndex }});
    }} }},
    reset: {{ configurable: true, writable: true, value() {{ return nativeFormReset(this, pushCommand); }} }},
    checkValidity: {{ configurable: true, writable: true, value() {{
      const valid = validityFlags(this.__glassEntry).valid;
      if (!valid) pushCommand({{ kind: "checkValidity", node_index: this.__glassEntry.nodeIndex }});
      return valid;
    }} }},
    reportValidity: {{ configurable: true, writable: true, value() {{
      const valid = validityFlags(this.__glassEntry).valid;
      if (!valid) pushCommand({{ kind: "reportValidity", node_index: this.__glassEntry.nodeIndex }});
      return valid;
    }} }},
    setCustomValidity: {{ configurable: true, writable: true, value(message) {{
      const entry = this.__glassEntry;
      const value = String(message);
      entry.customValidity = value;
      if (!entry.validity) entry.validity = {{}};
      entry.validity.customError = value.length > 0;
      entry.validity.valid = validityFlags(entry).valid;
      pushCommand({{ kind: "setCustomValidity", node_index: entry.nodeIndex, message: value }});
    }} }},
    appendChild: {{ configurable: true, writable: true, value(child) {{
      if (child && child.__glassFragment === true) {{
        const children = child.__glassChildren.slice();
        for (const fragmentChild of children) this.appendChild(fragmentChild);
        child.__glassChildren = [];
        return child;
      }}
      if (!child || typeof child.nodeIndex !== "number") throw new TypeError("child must be a native element");
      if (child === this) throw new TypeError("a node cannot contain itself");
      const wasConnected = nodeIsConnected(child);
      let ancestor = this;
      while (ancestor) {{
        if (ancestor === child) throw new TypeError("a node cannot contain one of its ancestors");
        ancestor = ancestor.__glassParent || null;
      }}
      const oldParent = child.__glassParent || null;
      if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
        queueFragmentChildRemoval(oldParent, child);
        oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
        if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
      }}
      this.__glassChildren = this.__glassChildren.filter(candidate => candidate !== child);
      this.__glassChildren.push(child);
      child.__glassParent = this;
      child.parentIndex = this.nodeIndex;
      this.__glassSyncContent();
      if (child.localName === "source" && (this.tagName === "AUDIO" || this.tagName === "VIDEO")
          && typeof this.__glassMediaReset === "function") this.__glassMediaReset();
      pushCommand({{ kind: "appendChild", parent_index: this.__glassEntry.nodeIndex, child_index: child.nodeIndex }});
      if (typeof globalThis.__glassRunNativeCustomElementInsertionReactions === "function")
        globalThis.__glassRunNativeCustomElementInsertionReactions(child, wasConnected, nodeIsConnected(this));
      if (nodeIsConnected(this)) executeInsertedScripts(child);
      return child;
    }} }},
    insertBefore: {{ configurable: true, writable: true, value(child, before) {{
      if (child && child.__glassFragment === true) {{
        const children = child.__glassChildren.slice();
        for (const fragmentChild of children) this.insertBefore(fragmentChild, before);
        child.__glassChildren = [];
        return child;
      }}
      if (before == null) return this.appendChild(child);
      if (!child || typeof child.nodeIndex !== "number" || typeof before.nodeIndex !== "number")
        throw new TypeError("insertBefore requires native elements");
      if (before.__glassParent !== this) throw new TypeError("reference node is not a child");
      if (child === before) return child;
      const wasConnected = nodeIsConnected(child);
      let ancestor = this;
      while (ancestor) {{
        if (ancestor === child) throw new TypeError("a node cannot contain one of its ancestors");
        ancestor = ancestor.__glassParent || null;
      }}
      const oldParent = child.__glassParent || null;
      if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
        queueFragmentChildRemoval(oldParent, child);
        oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
        if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
      }}
      this.__glassChildren = this.__glassChildren.filter(candidate => candidate !== child);
      const index = this.__glassChildren.indexOf(before);
      this.__glassChildren.splice(index < 0 ? this.__glassChildren.length : index, 0, child);
      child.__glassParent = this;
      child.parentIndex = this.nodeIndex;
      this.__glassSyncContent();
      if (child.localName === "source" && (this.tagName === "AUDIO" || this.tagName === "VIDEO")
          && typeof this.__glassMediaReset === "function") this.__glassMediaReset();
      pushCommand({{ kind: "insertBefore", parent_index: this.__glassEntry.nodeIndex,
        child_index: child.nodeIndex, before_index: before.nodeIndex }});
      if (typeof globalThis.__glassRunNativeCustomElementInsertionReactions === "function")
        globalThis.__glassRunNativeCustomElementInsertionReactions(child, wasConnected, nodeIsConnected(this));
      if (nodeIsConnected(this)) executeInsertedScripts(child);
      return child;
    }} }},
    remove: {{ configurable: true, writable: true, value() {{
      const parent = this.__glassParent || null;
      if (!parent && this.parentIndex === null) return;
      const wasConnected = nodeIsConnected(this);
      const commitRemoval = !this.__glassCreated || this.parentIndex !== null;
      queueFragmentChildRemoval(parent, this);
      if (parent && Array.isArray(parent.__glassChildren))
        parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== this);
      this.__glassParent = null;
      this.parentIndex = null;
      if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
      if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: this.__glassEntry.nodeIndex }});
      if (wasConnected && typeof globalThis.__glassRunNativeCustomElementRemovalReactions === "function")
        globalThis.__glassRunNativeCustomElementRemovalReactions(this);
    }} }},
    removeChild: {{ configurable: true, writable: true, value(child) {{
      if (!child || child.__glassParent !== this) throw new TypeError("child is not contained by this element");
      child.remove();
      return child;
    }} }},
    append: {{ configurable: true, writable: true, value(...items) {{
      for (const item of items) this.appendChild(
        item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
          ? item : makeDetachedText(String(item)));
    }} }},
    prepend: {{ configurable: true, writable: true, value(...items) {{
      const before = this.__glassChildren[0] || null;
      for (const item of items) this.insertBefore(
        item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
          ? item : makeDetachedText(String(item)), before);
    }} }},
    before: {{ configurable: true, writable: true, value(...items) {{
      const parent = this.__glassParent;
      if (!parent || typeof parent.insertBefore !== "function") return;
      for (const item of items) parent.insertBefore(
        item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
          ? item : makeDetachedText(String(item)), this);
    }} }},
    after: {{ configurable: true, writable: true, value(...items) {{
      const parent = this.__glassParent;
      if (!parent || typeof parent.insertBefore !== "function") return;
      const before = this.nextSibling;
      for (const item of items) parent.insertBefore(
        item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
          ? item : makeDetachedText(String(item)), before);
    }} }},
    replaceWith: {{ configurable: true, writable: true, value(...items) {{
      const parent = this.__glassParent;
      if (!parent || typeof parent.insertBefore !== "function") return;
      for (const item of items) parent.insertBefore(
        item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
          ? item : makeDetachedText(String(item)), this);
      this.remove();
    }} }},
    replaceChildren: {{ configurable: true, writable: true, value(...items) {{
      for (const child of this.__glassChildren.slice()) child.remove();
      this.append(...items);
    }} }},
  }};
  const installNativeElementSharedMethods = (prototype) =>
    Object.defineProperties(prototype, nativeElementSharedMethodProperties);
  Object.defineProperties(nativeElementSharedPrototype, {{
    validity: {{ configurable: true, get() {{
      return validityFlags(this.__glassEntry);
    }} }},
    validationMessage: {{ configurable: true, get() {{
      const entry = this.__glassEntry;
      return validationMessageFor(entry, validityFlags(entry));
    }} }},
    willValidate: {{ configurable: true, get() {{
      return Boolean(this.__glassEntry?.willValidate);
    }} }},
    scrollLeft: {{ configurable: true, get() {{
      return this.__glassEntry?.tagName?.toLowerCase() === "html"
        ? Number(state.scrollX) || 0 : Number(this.__glassScrollLeft) || 0;
    }}, set(value) {{ applyNativeElementScroll(this, value, this.scrollTop); }} }},
    scrollTop: {{ configurable: true, get() {{
      return this.__glassEntry?.tagName?.toLowerCase() === "html"
        ? Number(state.scrollY) || 0 : Number(this.__glassScrollTop) || 0;
    }}, set(value) {{ applyNativeElementScroll(this, this.scrollLeft, value); }} }},
    scrollTo: {{ configurable: true, writable: true, value(leftOrOptions = 0, top = 0) {{
      if (leftOrOptions && typeof leftOrOptions === "object")
        applyNativeElementScroll(this, leftOrOptions.left, leftOrOptions.top);
      else applyNativeElementScroll(this, leftOrOptions, top);
    }} }},
    scrollBy: {{ configurable: true, writable: true, value(leftOrOptions = 0, top = 0) {{
      const coordinate = (candidate) => {{
        const numeric = Number(candidate);
        return Number.isFinite(numeric) ? Math.trunc(numeric) : 0;
      }};
      if (leftOrOptions && typeof leftOrOptions === "object")
        applyNativeElementScroll(this, this.scrollLeft + coordinate(leftOrOptions.left), this.scrollTop + coordinate(leftOrOptions.top));
      else applyNativeElementScroll(this, this.scrollLeft + coordinate(leftOrOptions), this.scrollTop + coordinate(top));
    }} }},
    scroll: {{ configurable: true, writable: true, value(leftOrOptions = 0, top = 0) {{
      return this.scrollTo(leftOrOptions, top);
    }} }},
    __glassEventOwner: {{ configurable: true, get() {{ return "node:" + this.nodeIndex; }} }},
    __glassComputedStyle: {{ configurable: true, value() {{
      return this.__glassEntry.computedStyle && typeof this.__glassEntry.computedStyle === "object"
        ? this.__glassEntry.computedStyle
        : {{}};
    }} }},
    __glassInlineStyleAllowed: {{ configurable: true, value() {{
      return this.__glassEntry.inlineStyleAllowed !== false;
    }} }},
    parentElement: {{ configurable: true, get() {{
      if (this.__glassParent) return this.__glassParent.nodeType === 1 ? this.__glassParent : null;
      if (this.__glassCreated || this.parentIndex === null) return null;
      const current = globalThis.__glassHostElements;
      if (!(current instanceof Map) || current.get(this.nodeIndex) !== this) return null;
      return current.get(this.parentIndex) || null;
    }} }},
    parentNode: {{ configurable: true, get() {{
      if (this.__glassParent) return this.__glassParent;
      const owner = globalThis.document;
      return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(this)
        ? owner
        : null;
    }} }},
    ownerDocument: {{ configurable: true, get() {{
      return templateContentsOwnerDocumentForNode(this);
    }} }},
  }});
  installNativeElementSharedMethods(nativeElementSharedPrototype);
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("module_native_elements");
