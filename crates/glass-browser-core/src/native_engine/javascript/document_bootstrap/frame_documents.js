  const makeFrameDocument = (
    binding,
    ownerFrameElement = null,
    parentWindow = globalThis,
    topWindow = globalThis,
  ) => {{
    const currentBinding = frameBindingForId(frameIdentifier(binding)) || binding;
    const currentFrameId = frameIdentifier(currentBinding);
    const topologyKey = frameTopologyKey(currentBinding);
    const cached = frameDocumentCache.get(currentFrameId);
    if (cached && cached.url === currentBinding.url && cached.revision === currentBinding.revision
        && cached.topologyKey === topologyKey) return cached.document;
    const snapshot = currentBinding.document && typeof currentBinding.document === "object"
      ? currentBinding.document
      : {{ title: "", visibleText: "", elements: [] }};
    const frameScriptNodeObjectsByFrame = globalThis.__glassFrameScriptNodeObjectsByFrame instanceof Map
      ? globalThis.__glassFrameScriptNodeObjectsByFrame
      : new Map();
    const frameScriptNodeGenerations = globalThis.__glassFrameScriptNodeGenerations instanceof Map
      ? globalThis.__glassFrameScriptNodeGenerations
      : new Map();
    globalThis.__glassFrameScriptNodeObjectsByFrame = frameScriptNodeObjectsByFrame;
    globalThis.__glassFrameScriptNodeGenerations = frameScriptNodeGenerations;
    const frameGeneration = String(currentBinding.generation || "");
    let frameScriptNodeObjects = frameScriptNodeObjectsByFrame.get(currentFrameId);
    if (!(frameScriptNodeObjects instanceof Map)
        || frameScriptNodeGenerations.get(currentFrameId) !== frameGeneration) {{
      frameScriptNodeObjects = new Map();
      frameScriptNodeObjectsByFrame.set(currentFrameId, frameScriptNodeObjects);
      frameScriptNodeGenerations.set(currentFrameId, frameGeneration);
    }}
    for (const node of frameScriptNodeObjects.values()) {{
      if (node && typeof node === "object") node.__glassAttached = false;
    }}
    const frameScriptNodeAliasesByIndex = new Map();
    for (const identity of Array.isArray(snapshot.scriptNodes) ? snapshot.scriptNodes : []) {{
      const temporaryIndex = Number(identity && identity.temporaryIndex);
      const nodeIndex = Number(identity && identity.nodeIndex);
      const object = frameScriptNodeObjects.get(temporaryIndex);
      if (!object || !Number.isSafeInteger(nodeIndex) || nodeIndex < 0) continue;
      if (typeof globalThis.__glassRebindTransferredNodeIndex === "function") {{
        globalThis.__glassRebindTransferredNodeIndex(
          temporaryIndex,
          nodeIndex,
          "frame:" + currentFrameId + ":node:",
        );
      }}
      object.nodeIndex = nodeIndex;
      frameScriptNodeAliasesByIndex.set(nodeIndex, object);
    }}
    let frameDocumentTitle = String(snapshot.title || "");
    let frameDocument;
    let frameTemplateContentsOwnerDocument = null;
    const getFrameTemplateContentsOwnerDocument = () => {{
      const markFrameTemplateOwner = (ownerDocument) => {{
        ownerDocument.__glassNativeContextId = currentFrameId;
        ownerDocument.__glassNativeGeneration = Number(frameGeneration) || 0;
        ownerDocument.__glassNativeOrigin = String(currentBinding.origin || "null");
        ownerDocument.__glassNativeScriptNodes = Array.isArray(snapshot.scriptNodes)
          ? snapshot.scriptNodes
          : [];
        ownerDocument.__glassNativeFrameBinding = currentBinding;
        ownerDocument.__glassIsTemplateContentsOwnerDocument = true;
        ownerDocument.__glassGetTemplateContentsOwnerDocument = () => ownerDocument;
        return ownerDocument;
      }};
      const existing = frameTemplateContentsOwnerDocuments.get(currentFrameId);
      if (existing && existing.generation === frameGeneration && existing.document) {{
        frameTemplateContentsOwnerDocument = markFrameTemplateOwner(existing.document);
      }}
      if (frameTemplateContentsOwnerDocument) {{
        markFrameTemplateOwner(frameTemplateContentsOwnerDocument);
        try {{ Object.setPrototypeOf(frameTemplateContentsOwnerDocument, DocumentNative.prototype); }} catch (_error) {{}}
        return frameTemplateContentsOwnerDocument;
      }}
      const ownerDocument = {{
        nodeType: 9,
        nodeName: "#document",
        __glassNativeContextId: currentFrameId,
        __glassNativeGeneration: Number(frameGeneration) || 0,
        __glassNativeOrigin: String(currentBinding.origin || "null"),
        __glassNativeScriptNodes: Array.isArray(snapshot.scriptNodes) ? snapshot.scriptNodes : [],
        __glassNativeFrameBinding: currentBinding,
        __glassIsTemplateContentsOwnerDocument: true,
        __glassGetTemplateContentsOwnerDocument() {{ return ownerDocument; }},
        URL: "",
        documentURI: "",
        contentType: "text/html",
        compatMode: "CSS1Compat",
        defaultView: null,
        get documentElement() {{ return null; }},
        get childNodes() {{ return asNodeList([]); }},
        get children() {{ return asNodeList([]); }},
        getRootNode() {{ return this; }},
        createElement(name) {{
          const element = makeFrameDetachedElement(name, HTML_NAMESPACE);
          Object.defineProperty(element, "__glassOwnerDocumentOverride", {{ value: ownerDocument }});
          return element;
        }},
        createElementNS(namespace, name) {{
          const element = makeFrameDetachedElement(name, normalizeElementNamespace(namespace));
          Object.defineProperty(element, "__glassOwnerDocumentOverride", {{ value: ownerDocument }});
          return element;
        }},
        createAttribute(name) {{
          return makeAttributeNode(name, "", () => ownerDocument);
        }},
        createAttributeNS(namespace, qualifiedName) {{
          const namespaceURI = normalizeAttributeNamespace(namespace);
          return makeAttributeNode(qualifiedName, "", () => ownerDocument, namespaceURI);
        }},
        createTextNode(value) {{
          const node = makeFrameDetachedText(value);
          Object.defineProperty(node, "__glassOwnerDocumentOverride", {{ value: ownerDocument }});
          return node;
        }},
        createComment(value) {{
          const node = makeFrameDetachedComment(value);
          Object.defineProperty(node, "__glassOwnerDocumentOverride", {{ value: ownerDocument }});
          return node;
        }},
        createDocumentFragment() {{
          const fragment = makeFrameDocumentFragment();
          Object.defineProperty(fragment, "__glassOwnerDocumentOverride", {{ value: ownerDocument }});
          return fragment;
        }},
        importNode(node, options = false) {{
          return importNativeNode(ownerDocument, node, options);
        }},
        adoptNode(node) {{ return adoptNativeNode(ownerDocument, node); }},
        implementation: {{
          createDocumentType(name, publicId = "", systemId = "") {{
            return makeFrameDetachedDocumentType(
              name,
              publicId,
              systemId,
              () => ownerDocument,
            );
          }},
        }},
      }};
      frameTemplateContentsOwnerDocument = markFrameTemplateOwner(ownerDocument);
      frameTemplateContentsOwnerDocuments.set(currentFrameId, {{
        generation: frameGeneration,
        document: ownerDocument,
      }});
      try {{ Object.setPrototypeOf(ownerDocument, DocumentNative.prototype); }} catch (_error) {{}}
      return ownerDocument;
    }};
    const frameTemplateContentsOwnerDocumentForNode = (node) => {{
      if (!node) return frameDocument || null;
      if (node.__glassAdoptedOwnerDocument) return node.__glassAdoptedOwnerDocument;
      const ownerDocumentOverride = node.__glassOwnerDocumentOverride || null;
      if (ownerDocumentOverride && ownerDocumentOverride.__glassIsTemplateContentsOwnerDocument) {{
        return ownerDocumentOverride;
      }}
      const cachedDocument = frameDocumentCache.get(currentFrameId)?.document || null;
      if (node.__glassFrameGeneration === frameGeneration
          && cachedDocument && cachedDocument.__glassFrameGeneration === frameGeneration) {{
        return cachedDocument;
      }}
      if (ownerDocumentOverride) return ownerDocumentOverride;
      let current = node;
      while (current) {{
        if (Number(current.nodeType) === 11
            && (current.templateHostIndex != null || current.__glassTemplateHost)) {{
          return getFrameTemplateContentsOwnerDocument();
        }}
        current = current.__glassParent || null;
      }}
      return frameDocument || null;
    }};
    const markFrameTemplateContentsOwner = (node, ownerDocument) => {{
      if (!node) return;
      if (!node.__glassOwnerDocumentOverride) {{
        Object.defineProperty(node, "__glassOwnerDocumentOverride", {{ value: ownerDocument }});
      }}
      for (const child of node.__glassChildren || []) markFrameTemplateContentsOwner(child, ownerDocument);
    }};
    const frameSnapshotTextContentForNode = createSnapshotTextContentResolver(snapshot.nodes);
    const frameElements = (Array.isArray(snapshot.elements) ? snapshot.elements : []).map((entry) => {{
      const existing = frameScriptNodeAliasesByIndex.get(Number(entry.nodeIndex));
      if (existing && typeof existing.__glassRefresh === "function") {{
        existing.__glassRefresh(entry, frameSnapshotTextContentForNode);
        return existing;
      }}
      const attributes = entry.attributes && typeof entry.attributes === "object" ? entry.attributes : {{}};
      const attributeNamespaces = {{}};
      for (const [name, namespace] of Object.entries(
        entry.attributeNamespaces && typeof entry.attributeNamespaces === "object"
          ? entry.attributeNamespaces
          : {{}},
      )) attributeNamespaces[name] = String(namespace);
      let textContent = snapshotElementTextContent(entry, frameSnapshotTextContentForNode);
      let innerHtml = String(entry.innerHtml || "");
      let templateContentIndex = entry.templateContentIndex == null ? null : Number(entry.templateContentIndex);
      let templateContent = null;
      const namespaceURI = namespaceUriForEntry(entry);
      let value = entry.value == null ? "" : entry.value;
      let checked = Boolean(entry.checked);
      let indeterminate = Boolean(entry.indeterminate);
      let selected = Boolean(entry.selected);
      let disabled = Boolean(entry.disabled);
      let hidden = Boolean(entry.hidden);
      let multiple = Object.prototype.hasOwnProperty.call(attributes, "multiple");
      const frameAttributeNamespace = (name) => {{
        const value = attributeNamespaces[String(name)];
        return value === undefined || value === "" ? null : String(value);
      }};
      const inlineAttributeHandlers = new Map();
      const frameInlineEventTarget = (name, type) =>
        nativeWindowInlineEventTargetFor(projected, type, frameAttributeNamespace(name));
      const updateFrameInlineAttributeHandler = (name, source) => {{
        const type = nativeInlineEventType(name);
        if (!type) return;
        const target = frameInlineEventTarget(name, type);
        if (target) {{
          installNativeWindowInlineEventHandler(
            type,
            String(source === null || source === undefined ? "" : source),
            target,
            inlineAttributeHandlers,
          );
          return;
        }}
        const previous = inlineAttributeHandlers.get(type);
        if (previous && previous.shared) setEventHandlerValue(previous.target, type, null, false);
        inlineAttributeHandlers.delete(type);
      }};
      const removeFrameInlineAttributeHandler = (name, hadAttribute = true) => {{
        if (!hadAttribute) return;
        const type = nativeInlineEventType(name);
        if (!type) return;
        const record = inlineAttributeHandlers.get(type);
        const target = record && record.shared ? record.target : frameInlineEventTarget(name, type);
        if (target) setEventHandlerValue(target, type, null, false);
        inlineAttributeHandlers.delete(type);
      }};
      const refreshFrameInlineAttributeHandlers = () => {{
        const current = new Map();
        for (const [name, value] of Object.entries(attributes)) {{
          const type = nativeInlineEventType(name);
          if (type) current.set(type, {{ name, value: String(value) }});
        }}
        for (const type of inlineAttributeHandlers.keys()) {{
          if (!current.has(type)) removeFrameInlineAttributeHandler("on" + type);
        }}
        for (const [type, attribute] of current) {{
          const target = frameInlineEventTarget(attribute.name, type);
          const record = inlineAttributeHandlers.get(type);
          if (record
              && record.source === attribute.value
              && record.shared === Boolean(target)
              && (!target || record.target === target)) continue;
          updateFrameInlineAttributeHandler(attribute.name, attribute.value);
        }}
      }};
      const frameStoredAttributeLocalName = (name) =>
        storedAttributeLocalName(name, frameAttributeNamespace(name));
      const setFrameNamespacedAttribute = (namespace, name, nextValue) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const qualified = qualifiedAttributeName(name, namespaceURI);
      const stringValue = String(nextValue);
      if (stringValue.length > storageValueLimit) throw new RangeError("native frame attribute value exceeds its limit");
      attributes[qualified] = stringValue;
      if (namespaceURI === null) delete attributeNamespaces[qualified];
      else attributeNamespaces[qualified] = namespaceURI;
      updateFrameInlineAttributeHandler(qualified, stringValue);
      projected.__glassSyncContent();
      if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
      queueFrameCommand(currentBinding, {{
          kind: "setAttribute",
          node_index: entry.nodeIndex,
          name: qualified,
          value: stringValue,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
      }};
      const frameNamespacedAttributeValue = (namespace, name) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const localName = String(name);
        for (const key of Object.keys(attributes)) {{
          if (frameStoredAttributeLocalName(key) === localName
              && frameAttributeNamespace(key) === namespaceURI) return attributes[key];
        }}
        return null;
      }};
      const removeFrameNamespacedAttribute = (namespace, name) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const localName = String(name);
        const key = Object.keys(attributes).find((candidate) =>
          frameStoredAttributeLocalName(candidate) === localName
            && frameAttributeNamespace(candidate) === namespaceURI);
        if (key === undefined) return;
        delete attributes[key];
        delete attributeNamespaces[key];
        removeFrameInlineAttributeHandler(key, namespaceURI === null);
        projected.__glassSyncContent();
        if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        queueFrameCommand(currentBinding, {{
          kind: "removeAttribute",
          node_index: entry.nodeIndex,
          name: key,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
      }};
      const projected = {{
        nodeIndex: entry.nodeIndex,
        parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
        __glassFrameGeneration: frameGeneration,
        tagName: tagNameForEntry(entry),
        nodeType: 1,
        nodeName: tagNameForEntry(entry),
        localName: namespaceURI === HTML_NAMESPACE
          ? String(entry.tagName || "").toLowerCase()
          : String(entry.tagName || ""),
        namespaceURI,
        id: attributes.id || "",
        className: attributes.class || "",
        value,
        checked,
        indeterminate,
        selected,
        disabled,
        hidden,
        multiple,
        getBoundingClientRect() {{ return makeDomRect(geometryForNode(projected)); }},
        getClientRects() {{
          const geometry = geometryForNode(projected);
          return geometry.width > 0 && geometry.height > 0
            ? asNodeList([makeDomRect(geometry)])
            : asNodeList([]);
        }},
        get clientWidth() {{ return Number(geometryForNode(projected).width) || 0; }},
        get clientHeight() {{ return Number(geometryForNode(projected).height) || 0; }},
        get offsetWidth() {{ return Number(geometryForNode(projected).width) || 0; }},
        get offsetHeight() {{ return Number(geometryForNode(projected).height) || 0; }},
        get scrollWidth() {{ return Number(geometryForNode(projected).width) || 0; }},
        get scrollHeight() {{ return Number(geometryForNode(projected).height) || 0; }},
        getAttribute(name) {{
          const htmlElement = namespaceURI === HTML_NAMESPACE;
          const key = htmlElement ? String(name).toLowerCase() : String(name);
          for (const attribute of Object.keys(attributes)) {{
            if (htmlElement ? attribute.toLowerCase() === key : attribute === key) return attributes[attribute];
          }}
          return null;
        }},
        getAttributeNS(namespace, name) {{ return frameNamespacedAttributeValue(namespace, name); }},
        getAttributeNames() {{ return Object.keys(attributes); }},
        hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
        hasAttributeNS(namespace, name) {{ return this.getAttributeNS(namespace, name) !== null; }},
        matches(selector) {{ return matchesSelector(projected, selector); }},
        closest(selector) {{
          let current = projected;
          while (current) {{
            if (matchesSelector(current, selector)) return current;
            current = current.parentElement;
          }}
          return null;
        }},
        querySelector(selector) {{
          return descendantsInTree(projected, (candidate) => matchesSelector(candidate, selector, projected))[0] || null;
        }},
        querySelectorAll(selector) {{
          return asNodeList(descendantsInTree(projected, (candidate) => matchesSelector(candidate, selector, projected)));
        }},
        getElementsByTagName(name) {{
          const value = String(name).toLowerCase();
          return asHtmlCollection(descendantsInTree(projected, (candidate) =>
            value === "*" || candidate.tagName.toLowerCase() === value));
        }},
        getElementsByClassName(name) {{
          const value = String(name).trim();
          if (!value) return asHtmlCollection([]);
          const tokens = value.split(/\s+/);
          return asHtmlCollection(descendantsInTree(projected, (candidate) =>
            tokens.every((token) => String(candidate.className).split(/\s+/).includes(token))));
        }},
        focus() {{
          if (this.disabled || this.hidden || nativeFormAssociatedDisabledForFocus(this)
              || nativeInertForFocus(this)) return;
          const frameKey = nativeFrameFocusKey(currentBinding);
          const previousIndex = nativeFrameFocusedNodeIndex(currentBinding);
          const frameDocument = makeFrameDocument(currentBinding);
          const previous = previousIndex === null
            ? null
            : frameDocument.__glassEventTargetForNode(previousIndex);
          if (previous === this) return;
          if (previous) {{
            dispatchTarget(previous, createFocusEvent("blur", {{ relatedTarget: this, view: frameDocument.defaultView }}));
            dispatchTarget(previous, createFocusEvent("focusout", {{ bubbles: true, relatedTarget: this, view: frameDocument.defaultView }}));
          }}
          nativeFrameFocusedNodeIndices.set(frameKey, Number(entry.nodeIndex));
          dispatchTarget(this, createFocusEvent("focus", {{ relatedTarget: previous || null, view: frameDocument.defaultView }}));
          dispatchTarget(this, createFocusEvent("focusin", {{ bubbles: true, relatedTarget: previous || null, view: frameDocument.defaultView }}));
          queueFrameCommand(currentBinding, {{ kind: "focus", node_index: entry.nodeIndex }});
        }},
        blur() {{
          const frameKey = nativeFrameFocusKey(currentBinding);
          if (nativeFrameFocusedNodeIndex(currentBinding) !== Number(entry.nodeIndex)) return;
          const frameDocument = makeFrameDocument(currentBinding);
          nativeFrameFocusedNodeIndices.set(frameKey, null);
          dispatchTarget(this, createFocusEvent("blur", {{ relatedTarget: null, view: frameDocument.defaultView }}));
          dispatchTarget(this, createFocusEvent("focusout", {{ bubbles: true, relatedTarget: null, view: frameDocument.defaultView }}));
          queueFrameCommand(currentBinding, {{ kind: "blur", node_index: entry.nodeIndex }});
        }},
        click() {{
          if (this.disabled || this.hidden) return;
          const event = createEvent("click", {{ bubbles: true, cancelable: true }});
          if (!dispatchTarget(this, event)) return;
          queueFrameCommand(currentBinding, {{ kind: "click", node_index: entry.nodeIndex }});
        }},
        addEventListener(type, callback, options) {{
          addListener(ownerFor(projected), type, callback, options);
        }},
        removeEventListener(type, callback, options) {{
          removeListener(ownerFor(projected), type, callback, options);
        }},
        dispatchEvent(event) {{
          return dispatchTarget(this, event);
        }},
        setAttribute(name, nextValue) {{
          const key = String(name).toLowerCase();
          const stringValue = String(nextValue);
          attributes[key] = stringValue;
          delete attributeNamespaces[key];
          updateFrameInlineAttributeHandler(key, stringValue);
          if (key === "disabled") disabled = true;
          if (key === "hidden") hidden = true;
          if (key === "multiple") multiple = true;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "setAttribute", node_index: entry.nodeIndex, name: key, value: stringValue, namespace_uri: "" }});
        }},
        setAttributeNS(namespace, qualifiedName, nextValue) {{
          setFrameNamespacedAttribute(namespace, qualifiedName, nextValue);
        }},
        removeAttribute(name) {{
          const key = String(name).toLowerCase();
          const oldValue = Object.prototype.hasOwnProperty.call(attributes, key)
            ? String(attributes[key])
            : null;
          delete attributes[key];
          delete attributeNamespaces[key];
          removeFrameInlineAttributeHandler(key, oldValue !== null);
          if (key === "disabled") disabled = false;
          if (key === "hidden") hidden = false;
          if (key === "multiple") multiple = false;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "removeAttribute", node_index: entry.nodeIndex, name: key, namespace_uri: "" }});
        }},
        removeAttributeNS(namespace, name) {{
          removeFrameNamespacedAttribute(namespace, name);
        }},
        appendChild(child) {{
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) projected.appendChild(fragmentChild);
            child.__glassChildren = [];
            return child;
          }}
          if (!child || typeof child.nodeIndex !== "number") throw new TypeError("child must be a native node");
          if (child === projected) throw new TypeError("a node cannot contain itself");
          let ancestor = projected;
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
          const oldParentIndex = child.parentIndex;
          projected.__glassChildren = projected.__glassChildren.filter(candidate => candidate !== child);
          projected.__glassChildren.push(child);
          child.__glassParent = projected;
          child.parentIndex = projected.nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (projected.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "appendChild", parent_index: entry.nodeIndex, child_index: child.nodeIndex }});
          return child;
        }},
        insertBefore(child, before) {{
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) projected.insertBefore(fragmentChild, before);
            child.__glassChildren = [];
            return child;
          }}
          if (before == null) return this.appendChild(child);
          if (!child || typeof child.nodeIndex !== "number"
              || typeof before.nodeIndex !== "number") throw new TypeError("insertBefore requires native nodes");
          if (before.__glassParent !== projected) throw new TypeError("reference node is not a child");
          if (child === before) return child;
          let ancestor = projected;
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
          const oldParentIndex = child.parentIndex;
          projected.__glassChildren = projected.__glassChildren.filter(candidate => candidate !== child);
          const index = projected.__glassChildren.indexOf(before);
          projected.__glassChildren.splice(index < 0 ? projected.__glassChildren.length : index, 0, child);
          child.__glassParent = projected;
          child.parentIndex = projected.nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (projected.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{
            kind: "insertBefore",
            parent_index: entry.nodeIndex,
            child_index: child.nodeIndex,
            before_index: before.nodeIndex,
          }});
          return child;
        }},
        remove() {{
          const parent = projected.__glassParent || null;
          if (!parent && projected.parentIndex === null) return;
          const commitRemoval = !projected.__glassCreated || projected.parentIndex !== null;
          queueFragmentChildRemoval(parent, projected);
          if (parent && Array.isArray(parent.__glassChildren)) {{
            parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== projected);
          }}
          projected.__glassParent = null;
          projected.parentIndex = null;
          detachFrameSubtree(projected);
          if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
          if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: entry.nodeIndex }});
        }},
        removeChild(child) {{
          if (!child || child.__glassParent !== projected) {{
            throw new TypeError("child is not contained by this element");
          }}
          child.remove();
          return child;
        }},
      }};
      Object.defineProperty(projected, "__glassRefreshInlineEventHandlers", {{
        enumerable: false,
        configurable: false,
        value: refreshFrameInlineAttributeHandlers,
      }});
      installCommonAttributeProperties(projected, {{
        disabled: () => disabled,
        hidden: () => hidden,
        multiple: () => multiple,
      }}, String(currentBinding.url));
      Object.defineProperty(projected, "__glassChildren", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: [],
      }});
      Object.defineProperty(projected, "__glassEventOwner", {{
        enumerable: false,
        configurable: false,
        get() {{ return "frame:" + currentFrameId + ":node:" + projected.nodeIndex; }},
      }});
      Object.defineProperty(projected, "__glassParent", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: null,
      }});
      Object.defineProperty(projected, "__glassCreated", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: false,
      }});
      Object.defineProperty(projected, "__glassAttached", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: true,
      }});
      Object.defineProperty(projected, "__glassTemplateContentRef", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: null,
      }});
      Object.defineProperty(projected, "__glassAttributeSource", {{
        enumerable: false,
        configurable: false,
        value: () => attributes,
      }});
      Object.defineProperty(projected, "__glassAttributeNamespace", {{
        enumerable: false,
        configurable: false,
        value: (name) => frameAttributeNamespace(name),
      }});
      Object.defineProperty(projected, "__glassGeometrySource", {{
        enumerable: false,
        configurable: false,
        value: () => {{
          const active = frameBindingForId(currentFrameId) || currentBinding;
          const geometry = active && active.document && Array.isArray(active.document.geometry)
            ? active.document.geometry
            : [];
          return geometry.find((candidate) => Number(candidate.nodeIndex) === Number(entry.nodeIndex)) || null;
        }},
      }});
      Object.defineProperty(projected, "__glassSyncContent", {{
        enumerable: false,
        configurable: false,
        value(clearEmpty = false) {{
          if (projected.__glassChildren.length > 0) {{
            innerHtml = projected.__glassChildren.map(child => child.__glassMarkup).join("");
            textContent = projected.__glassChildren.map(child => child.__glassTextValue).join("");
          }} else if (clearEmpty) {{
            innerHtml = "";
            textContent = "";
          }}
          if (projected.__glassParent && typeof projected.__glassParent.__glassSyncContent === "function") projected.__glassParent.__glassSyncContent(clearEmpty);
        }},
      }});
      Object.defineProperty(projected, "__glassTextValue", {{
        enumerable: false,
        configurable: false,
        get() {{ return textContent; }},
      }});
      Object.defineProperty(projected, "__glassMarkup", {{
        enumerable: false,
        configurable: false,
        get() {{
          const markup = Object.keys(attributes)
            .sort()
            .map(name => " " + name + "=\"" + escapeHtmlText(attributes[name]).replace(/\"/g, "&quot;") + "\"")
            .join("");
          const opening = "<" + projected.localName + markup + ">";
          if (["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"].includes(projected.localName)) return opening;
          const content = projected.namespaceURI === HTML_NAMESPACE && projected.localName === "template"
            ? projected.content.innerHTML
            : projected.__glassChildren.length > 0
            ? projected.__glassChildren.map(child => child.__glassMarkup).join("")
            : innerHtml;
          return opening + content + "</" + projected.localName + ">";
        }},
      }});
      Object.defineProperty(projected, "outerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return projected.__glassMarkup; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame element outerHTML exceeds its limit");
          const owner = projected.__glassParent || null;
          if (!owner || typeof owner.insertBefore !== "function") throw new TypeError("outerHTML requires an attached element");
          const fragment = makeFrameDocumentFragment();
          populateDetachedFragment(fragment, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment, owner);
          for (const child of fragment.__glassChildren.slice()) owner.insertBefore(child, projected);
          projected.remove();
        }},
      }});
      Object.defineProperty(projected, "parentElement", {{
        enumerable: false,
        configurable: false,
        get() {{ return projected.__glassParent && projected.__glassParent.nodeType === 1 ? projected.__glassParent : null; }},
      }});
      Object.defineProperty(projected, "parentNode", {{
        enumerable: false,
        configurable: false,
        get() {{
          if (projected.__glassParent) return projected.__glassParent;
          const owner = projected.ownerDocument;
          return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(projected)
            ? owner
            : null;
        }},
      }});
      if (namespaceURI === HTML_NAMESPACE && projected.localName === "template") {{
        Object.defineProperty(projected, "content", {{
          enumerable: true,
          configurable: false,
          get() {{
            if (templateContent) return templateContent;
            if (projected.__glassTemplateContentRef) {{
              templateContent = projected.__glassTemplateContentRef;
              return templateContent;
            }}
            const indexed = templateContentIndex === null ? null : frameNodesByIndex.get(templateContentIndex);
            if (indexed && Number(indexed.nodeType) === 11) {{
              templateContent = indexed;
              indexed.__glassTemplateHost = projected;
              projected.__glassTemplateContentRef = indexed;
              return templateContent;
            }}
            templateContent = makeFrameDocumentFragment(null, projected);
            projected.__glassTemplateContentRef = templateContent;
            return templateContent;
          }},
        }});
      }}
      for (const property of ["textContent", "innerText"]) {{
        Object.defineProperty(projected, property, {{
            enumerable: true,
            configurable: false,
            get() {{ return textContent; }},
            set(next) {{
              const value = String(next);
              if (value.length > storageValueLimit) throw new RangeError("native frame element textContent exceeds its limit");
              for (const child of projected.__glassChildren) {{
                child.__glassParent = null;
                child.parentIndex = null;
                detachFrameSubtree(child);
              }}
              projected.__glassChildren = [];
              innerHtml = "";
              textContent = "";
              suppressHostCommands += 1;
              try {{
                if (value) projected.appendChild(makeFrameDetachedText(value));
              }} finally {{
                suppressHostCommands -= 1;
              }}
              projected.__glassSyncContent(true);
              queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: entry.nodeIndex, value }});
            }},
          }});
        }}
      Object.defineProperty(projected, "innerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return projected.localName === "template" ? projected.content.innerHTML : innerHtml; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame element innerHTML exceeds its limit");
          if (namespaceURI === HTML_NAMESPACE && projected.localName === "template") {{
            const content = projected.content;
            for (const child of content.__glassChildren) {{
              child.__glassParent = null;
              child.parentIndex = null;
              detachFrameSubtree(child);
            }}
            content.__glassChildren = [];
            innerHtml = "";
            textContent = "";
            suppressHostCommands += 1;
            try {{
              populateDetachedFragment(content, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment, projected);
            }} finally {{
              suppressHostCommands -= 1;
            }}
            innerHtml = value;
            projected.__glassSyncContent(true);
            queueFrameCommand(currentBinding, {{ kind: "setInnerHtml", node_index: entry.nodeIndex, value }});
            return;
          }}
          for (const child of projected.__glassChildren) {{
            child.__glassParent = null;
            child.parentIndex = null;
            detachFrameSubtree(child);
          }}
          projected.__glassChildren = [];
          innerHtml = "";
          textContent = "";
          suppressHostCommands += 1;
          try {{
          populateDetachedFragment(projected, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment);
          }} finally {{
            suppressHostCommands -= 1;
          }}
          projected.__glassSyncContent(true);
          queueFrameCommand(currentBinding, {{ kind: "setInnerHtml", node_index: entry.nodeIndex, value }});
        }},
      }});
      Object.defineProperty(projected, "value", {{
        enumerable: true,
        configurable: false,
        get() {{ return value; }},
        set(nextValue) {{
          value = String(nextValue);
          queueFrameCommand(currentBinding, {{ kind: "setValue", node_index: entry.nodeIndex, value }});
        }},
      }});
      Object.defineProperty(projected, "checked", {{
        enumerable: true,
        configurable: false,
        get() {{ return checked; }},
        set(nextValue) {{
          checked = Boolean(nextValue);
          queueFrameCommand(currentBinding, {{ kind: "setChecked", node_index: entry.nodeIndex, checked }});
        }},
      }});
      Object.defineProperty(projected, "indeterminate", {{
        enumerable: true,
        configurable: false,
        get() {{ return indeterminate; }},
        set(nextValue) {{
          if (String(entry.tagName || "").toLowerCase() !== "input") throw new TypeError("indeterminate requires an input element");
          indeterminate = Boolean(nextValue);
          queueFrameCommand(currentBinding, {{ kind: "setIndeterminate", node_index: entry.nodeIndex, indeterminate }});
        }},
      }});
      Object.defineProperty(projected, "selected", {{
        enumerable: true,
        configurable: false,
        get() {{ return selected; }},
        set(nextValue) {{
          selected = Boolean(nextValue);
          queueFrameCommand(currentBinding, {{ kind: "setSelected", node_index: entry.nodeIndex, selected }});
        }},
      }});
      Object.defineProperty(projected, "__glassSetFormResetState", {{
        enumerable: false,
        configurable: false,
        value(next) {{
          if (Object.prototype.hasOwnProperty.call(next, "value")) value = String(next.value);
          if (Object.prototype.hasOwnProperty.call(next, "checked")) checked = Boolean(next.checked);
          if (Object.prototype.hasOwnProperty.call(next, "selected")) selected = Boolean(next.selected);
        }},
      }});
      if (projected.tagName === "FORM") Object.defineProperty(projected, "reset", {{
        enumerable: false,
        configurable: false,
        value() {{ return nativeFormReset(projected, command => queueFrameCommand(currentBinding, command)); }},
      }});
      Object.defineProperty(projected, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return frameTemplateContentsOwnerDocumentForNode(projected); }},
      }});
      installAttributeNodeSurface(projected, () => frameDocument);
      const childBinding = frameChildBindingForNode(currentBinding, entry.nodeIndex);
      if (["IFRAME", "FRAME"].includes(projected.tagName)) {{
        Object.defineProperty(projected, "contentWindow", {{
          enumerable: false,
          configurable: false,
          get() {{
            return childBinding
              ? currentFrameWindow(childBinding, projected, currentFrameWindow(currentBinding, ownerFrameElement, parentWindow, topWindow), topWindow)
              : null;
          }},
        }});
        Object.defineProperty(projected, "contentDocument", {{
          enumerable: false,
          configurable: false,
          get() {{
            if (!childBinding || !childBinding.sameOrigin) return null;
            const currentWindow = currentFrameWindow(currentBinding, ownerFrameElement, parentWindow, topWindow);
            return makeFrameDocument(childBinding, projected, currentWindow, topWindow);
          }},
        }});
      }}
      Object.defineProperty(projected, "__glassSetNativeIndex", {{
        enumerable: false,
        configurable: false,
        value(nextIndex) {{
          const index = Number(nextIndex);
          if (!Number.isSafeInteger(index) || index < 0) throw new TypeError("native node index is invalid");
          entry = {{ ...entry, nodeIndex: index }};
          projected.nodeIndex = index;
        }},
      }});
      Object.defineProperty(projected, "__glassRefresh", {{
        enumerable: false,
        configurable: false,
        value(nextEntry, nextExactTextForNode = frameSnapshotTextContentForNode) {{
          entry = nextEntry;
          projected.nodeIndex = Number(nextEntry.nodeIndex);
          projected.parentIndex = nextEntry.parentIndex == null ? null : Number(nextEntry.parentIndex);
          projected.tagName = tagNameForEntry(nextEntry);
          projected.nodeName = tagNameForEntry(nextEntry);
          projected.localName = String(nextEntry.tagName || "").toLowerCase();
          for (const name of Object.keys(attributes)) delete attributes[name];
          for (const [name, value] of Object.entries(nextEntry.attributes || {{}})) attributes[name] = String(value);
          for (const name of Object.keys(attributeNamespaces)) delete attributeNamespaces[name];
          for (const [name, namespace] of Object.entries(nextEntry.attributeNamespaces || {{}})) {{
            attributeNamespaces[name] = String(namespace);
          }}
          textContent = snapshotElementTextContent(nextEntry, nextExactTextForNode);
          innerHtml = String(nextEntry.innerHtml || "");
          templateContentIndex = nextEntry.templateContentIndex == null ? null : Number(nextEntry.templateContentIndex);
          value = nextEntry.value == null ? "" : nextEntry.value;
          checked = Boolean(nextEntry.checked);
          indeterminate = Boolean(nextEntry.indeterminate);
          selected = Boolean(nextEntry.selected);
          disabled = Boolean(nextEntry.disabled);
          hidden = Boolean(nextEntry.hidden);
          multiple = Object.prototype.hasOwnProperty.call(attributes, "multiple");
          refreshFrameInlineAttributeHandlers();
          if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        }},
      }});
      try {{ Object.setPrototypeOf(projected, elementPrototypeFor(projected.tagName)); }} catch (_error) {{}}
      return projected;
    }});
    const frameNodeSnapshots = Array.isArray(snapshot.nodes) ? snapshot.nodes : [];
    const frameTextNodes = frameNodeSnapshots
      .filter((entry) => entry && [3, 8].includes(Number(entry.nodeType)))
      .map((entry) => {{
        const existing = frameScriptNodeAliasesByIndex.get(Number(entry.nodeIndex));
        if (existing && typeof existing.__glassRefresh === "function") {{
          existing.__glassRefresh(entry);
          return existing;
        }}
        const isComment = Number(entry.nodeType) === 8;
        let textContent = String(entry.nodeValue || "");
        const text = {{
          nodeIndex: entry.nodeIndex,
          parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
          nodeType: isComment ? 8 : 3,
          nodeName: isComment ? "#comment" : "#text",
          nodeValue: textContent,
          remove() {{
            const parent = text.__glassParent || null;
            if (!parent && text.parentIndex === null) return;
            const commitRemoval = !text.__glassCreated || text.parentIndex !== null;
            queueFragmentChildRemoval(parent, text);
            if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== text);
            text.__glassParent = null;
            text.parentIndex = null;
            text.__glassAttached = false;
            if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
            if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: text.nodeIndex }});
          }},
        }};
        Object.defineProperty(text, "__glassRefresh", {{
          enumerable: false,
          configurable: false,
          value(nextEntry) {{
            text.nodeIndex = Number(nextEntry.nodeIndex);
            text.parentIndex = nextEntry.parentIndex == null ? null : Number(nextEntry.parentIndex);
            textContent = String(nextEntry.nodeValue || "");
            text.nodeValue = textContent;
          }},
        }});
        Object.defineProperty(text, "__glassSetNativeIndex", {{ enumerable: false, configurable: false, value(nextIndex) {{
          text.nodeIndex = Number(nextIndex);
        }} }});
        Object.defineProperty(text, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
        Object.defineProperty(text, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: false }});
        Object.defineProperty(text, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: true }});
    Object.defineProperty(text, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return isComment ? "" : textContent; }} }});
        Object.defineProperty(text, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return isComment ? "<!--" + textContent + "-->" : serializeHtmlTextNode(text, textContent); }} }});
        Object.defineProperty(text, "parentElement", {{
          enumerable: false,
          configurable: false,
          get() {{
            let current = text.__glassParent || null;
            while (current && current.nodeType !== 1) current = current.__glassParent || null;
            return current || null;
          }},
        }});
        Object.defineProperty(text, "parentNode", {{
          enumerable: false,
          configurable: false,
          get() {{
            if (text.__glassParent) return text.__glassParent;
            const owner = text.ownerDocument;
            return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(text)
              ? owner
              : null;
          }},
        }});
        Object.defineProperty(text, "nodeValue", {{
          enumerable: true,
          configurable: false,
          get() {{ return textContent; }},
          set(next) {{ text.textContent = next; }},
        }});
        Object.defineProperty(text, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return frameTemplateContentsOwnerDocumentForNode(text); }} }});
        Object.defineProperty(text, "textContent", {{
          enumerable: true,
          configurable: false,
          get() {{ return textContent; }},
          set(next) {{
            const value = String(next);
            if (value.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
            textContent = value;
            if (text.__glassParent && typeof text.__glassParent.__glassSyncContent === "function") text.__glassParent.__glassSyncContent();
            queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: text.nodeIndex, value }});
          }},
        }});
        Object.defineProperty(text, "data", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ text.textContent = next; }} }});
        try {{ Object.setPrototypeOf(text, isComment ? CommentNative.prototype : TextNative.prototype); }} catch (_error) {{}}
        return text;
      }});
    const frameDocumentTypeNodes = frameNodeSnapshots
      .filter((entry) => entry && Number(entry.nodeType) === 10)
      .map((entry) => {{
        const existing = frameScriptNodeAliasesByIndex.get(Number(entry.nodeIndex));
        const documentType = existing || {{
          nodeIndex: entry.nodeIndex,
          parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
          nodeType: 10,
          nodeName: String(entry.nodeName || ""),
          nodeValue: null,
          name: String(entry.nodeName || ""),
          publicId: entry.publicId == null ? "" : String(entry.publicId),
          systemId: entry.systemId == null ? "" : String(entry.systemId),
          __glassChildren: [],
          __glassParent: null,
          textContent: null,
          remove() {{
            const parent = documentType.__glassParent || null;
            if (!parent && documentType.parentIndex == null) return;
            const owner = parent || (documentType.parentIndex === 0 ? frameDocument : null);
            if (owner && Array.isArray(owner.__glassChildren)) owner.__glassChildren = owner.__glassChildren.filter(candidate => candidate !== documentType);
            documentType.__glassParent = null;
            documentType.parentIndex = null;
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: documentType.nodeIndex }});
          }},
        }};
        if (!Object.prototype.hasOwnProperty.call(documentType, "__glassRefresh")) {{
          Object.defineProperty(documentType, "__glassRefresh", {{ enumerable: false, configurable: false, value(nextEntry) {{
            documentType.nodeIndex = Number(nextEntry.nodeIndex);
            documentType.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
            documentType.name = String(nextEntry.nodeName || documentType.name || "");
            documentType.nodeName = documentType.name;
            documentType.publicId = nextEntry.publicId == null ? "" : String(nextEntry.publicId);
            documentType.systemId = nextEntry.systemId == null ? "" : String(nextEntry.systemId);
          }} }});
        }}
        if (existing && typeof documentType.__glassRefresh === "function") documentType.__glassRefresh(entry);
        if (!Object.prototype.hasOwnProperty.call(documentType, "ownerDocument")) Object.defineProperty(documentType, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return frameDocument; }} }});
        if (!Object.prototype.hasOwnProperty.call(documentType, "parentNode")) Object.defineProperty(documentType, "parentNode", {{ enumerable: false, configurable: false, get() {{ return documentType.__glassParent || null; }} }});
        if (!Object.prototype.hasOwnProperty.call(documentType, "parentElement")) Object.defineProperty(documentType, "parentElement", {{ enumerable: false, configurable: false, get() {{
          const parent = documentType.__glassParent || null;
          return parent && parent.nodeType === 1 ? parent : null;
        }} }});
        try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
        return documentType;
      }});
    const frameNodesByIndex = new Map([
      ...frameElements.map((element) => [element.nodeIndex, element]),
      ...frameTextNodes.map((text) => [text.nodeIndex, text]),
      ...frameDocumentTypeNodes.map((documentType) => [documentType.nodeIndex, documentType]),
    ]);
    const registerFrameSubtree = (node) => {{
      const pending = [node];
      const visited = new Set();
      while (pending.length > 0) {{
        const current = pending.pop();
        if (!current || visited.has(current)) continue;
        visited.add(current);
        if (current.nodeType === 1 && !frameElements.includes(current)) frameElements.push(current);
        current.__glassAttached = true;
        for (const child of current.__glassChildren || []) pending.push(child);
      }}
    }};
    const detachFrameSubtree = (node) => {{
      const pending = [node];
      const visited = new Set();
      while (pending.length > 0) {{
        const current = pending.pop();
        if (!current || visited.has(current)) continue;
        visited.add(current);
        current.__glassAttached = false;
        for (const child of current.__glassChildren || []) pending.push(child);
      }}
    }};
    for (const node of [...frameElements, ...frameTextNodes, ...frameDocumentTypeNodes]) {{
      node.__glassChildren = [];
      node.__glassParent = null;
      node.__glassAttached = false;
    }}
    if (frameNodeSnapshots.length > 0) {{
      for (const snapshotNode of frameNodeSnapshots) {{
        const parent = frameNodesByIndex.get(snapshotNode.nodeIndex);
        if (!parent || !Array.isArray(snapshotNode.children)) continue;
        for (const childIndex of snapshotNode.children) {{
          const child = frameNodesByIndex.get(childIndex);
          if (!child) continue;
          parent.__glassChildren.push(child);
          child.__glassParent = parent;
        }}
      }}
    }} else {{
      for (const element of frameElements) {{
        if (element.parentIndex === null) continue;
        const parent = frameElements.find(candidate => candidate.nodeIndex === element.parentIndex);
        if (!parent) continue;
        parent.__glassChildren.push(element);
        element.__glassParent = parent;
      }}
    }}
    for (const snapshotNode of frameNodeSnapshots) {{
      const nodeIndex = Number(snapshotNode.nodeIndex);
      const key = frameMutationKey(currentBinding, nodeIndex);
      const node = frameNodesByIndex.get(nodeIndex);
      if (node) frameMutationNodes.set(key, node);
      if (snapshotNode.parentIndex !== null && snapshotNode.parentIndex !== undefined) {{
        frameMutationParents.set(key, Number(snapshotNode.parentIndex));
      }}
      frameMutationChildren.set(key, Array.isArray(snapshotNode.children)
        ? snapshotNode.children.map(Number)
        : []);
      if ([3, 8].includes(Number(snapshotNode.nodeType))) {{
        frameMutationText.set(key, String(snapshotNode.nodeValue || ""));
      }} else if (node && node.nodeType === 1) {{
        frameMutationAttributes.set(key, {{ ...(frameElements.find((element) => element.nodeIndex === nodeIndex)?.__glassAttributeSource?.() || {{}}) }});
      }}
    }}
    for (const element of frameElements) defineTreeAccessors(element);
    for (const text of frameTextNodes) defineTreeAccessors(text);
    for (const documentType of frameDocumentTypeNodes) defineTreeAccessors(documentType);
    for (const element of frameElements) {{
      installClassList(element);
      installElementStyleAndDataset(element);
    }}
    const makeFrameDetachedElement = (tagName, namespace = HTML_NAMESPACE) => {{
      const namespaceURI = normalizeElementNamespace(namespace);
      const requestedName = String(tagName);
      const normalized = namespaceURI === HTML_NAMESPACE
        ? requestedName.toLowerCase()
        : requestedName;
      if (!/^[A-Za-z][A-Za-z0-9:_-]*$/.test(normalized)) throw new TypeError("invalid element name");
      let nodeIndex = allocateTemporaryNodeIndex();
      const attributes = {{}};
      const attributeNamespaces = {{}};
      let textContent = "";
      let innerHtml = "";
      let templateContentIndex = null;
      let templateContent = null;
      let value = "";
      let checked = false;
      let indeterminate = false;
      let selected = false;
      let disabled = false;
      let hidden = false;
      let multiple = false;
      const frameAttributeNamespace = (name) => {{
        const value = attributeNamespaces[String(name)];
        return value === undefined || value === "" ? null : String(value);
      }};
      const inlineEventHandlers = createNativeWindowInlineAttributeController(
        () => projected,
        frameAttributeNamespace,
      );
      const updateFrameInlineAttributeHandler = inlineEventHandlers.set;
      const removeFrameInlineAttributeHandler = inlineEventHandlers.remove;
      const refreshFrameInlineAttributeHandlers = () => inlineEventHandlers.refresh(attributes);
      const frameStoredAttributeLocalName = (name) =>
        storedAttributeLocalName(name, frameAttributeNamespace(name));
      const setFrameNamespacedAttribute = (namespace, name, nextValue) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const qualified = qualifiedAttributeName(name, namespaceURI);
          const stringValue = String(nextValue);
          if (stringValue.length > storageValueLimit) throw new RangeError("native frame attribute value exceeds its limit");
          attributes[qualified] = stringValue;
          if (namespaceURI === null) delete attributeNamespaces[qualified];
          else attributeNamespaces[qualified] = namespaceURI;
          updateFrameInlineAttributeHandler(qualified, stringValue);
          projected.__glassSyncContent();
        if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        queueFrameCommand(currentBinding, {{
          kind: "setAttribute",
          node_index: nodeIndex,
          name: qualified,
          value: stringValue,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
      }};
      const frameNamespacedAttributeValue = (namespace, name) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const localName = String(name);
        for (const key of Object.keys(attributes)) {{
          if (frameStoredAttributeLocalName(key) === localName
              && frameAttributeNamespace(key) === namespaceURI) return attributes[key];
        }}
        return null;
      }};
      const removeFrameNamespacedAttribute = (namespace, name) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const localName = String(name);
        const key = Object.keys(attributes).find((candidate) =>
          frameStoredAttributeLocalName(candidate) === localName
            && frameAttributeNamespace(candidate) === namespaceURI);
        if (key === undefined) return;
        delete attributes[key];
        delete attributeNamespaces[key];
        removeFrameInlineAttributeHandler(key, namespaceURI === null);
        projected.__glassSyncContent();
        if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        queueFrameCommand(currentBinding, {{
          kind: "removeAttribute",
          node_index: nodeIndex,
          name: key,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
      }};
      const projected = {{
        nodeIndex,
        parentIndex: null,
        __glassFrameGeneration: frameGeneration,
        tagName: namespaceURI === HTML_NAMESPACE ? normalized.toUpperCase() : normalized,
        nodeType: 1,
        nodeName: namespaceURI === HTML_NAMESPACE ? normalized.toUpperCase() : normalized,
        localName: normalized,
        namespaceURI,
        id: "",
        className: "",
        value: "",
        checked: false,
        indeterminate: false,
        selected: false,
        disabled,
        hidden,
        multiple,
        getAttribute(name) {{
          const htmlElement = namespaceURI === HTML_NAMESPACE;
          const key = htmlElement ? String(name).toLowerCase() : String(name);
          for (const attribute of Object.keys(attributes)) {{
            if (htmlElement ? attribute.toLowerCase() === key : attribute === key) return attributes[attribute];
          }}
          return null;
        }},
        getAttributeNS(namespace, name) {{ return frameNamespacedAttributeValue(namespace, name); }},
        getAttributeNames() {{ return Object.keys(attributes); }},
        hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
        hasAttributeNS(namespace, name) {{ return this.getAttributeNS(namespace, name) !== null; }},
        matches(selector) {{ return projectedFrameMatches(projected, selector); }},
        closest(selector) {{
          let current = projected;
          while (current) {{
            if (projectedFrameMatches(current, selector)) return current;
            current = current.parentElement;
          }}
          return null;
        }},
        addEventListener(type, callback, options) {{
          addListener(ownerFor(projected), type, callback, options);
        }},
        removeEventListener(type, callback, options) {{
          removeListener(ownerFor(projected), type, callback, options);
        }},
        dispatchEvent(event) {{
          return dispatchTarget(this, event);
        }},
        setAttribute(name, nextValue) {{
          const key = String(name).toLowerCase();
          const stringValue = String(nextValue);
          attributes[key] = stringValue;
          delete attributeNamespaces[key];
          updateFrameInlineAttributeHandler(key, stringValue);
          if (key === "disabled") disabled = true;
          if (key === "hidden") hidden = true;
          if (key === "multiple") multiple = true;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "setAttribute", node_index: nodeIndex, name: key, value: stringValue, namespace_uri: "" }});
        }},
        __glassSetParsedAttribute(name, nextValue, namespace) {{
          const qualifiedName = String(name);
          const namespaceURI = namespace === null || namespace === undefined ? null : String(namespace);
          const stringValue = String(nextValue);
          if (stringValue.length > storageValueLimit) throw new RangeError("native parsed frame attribute value exceeds its limit");
          attributes[qualifiedName] = stringValue;
          if (namespaceURI === null) delete attributeNamespaces[qualifiedName];
          else attributeNamespaces[qualifiedName] = namespaceURI;
          const key = qualifiedName.toLowerCase();
          updateFrameInlineAttributeHandler(key, stringValue);
          if (key === "disabled") disabled = true;
          if (key === "hidden") hidden = true;
          if (key === "multiple") multiple = true;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{
            kind: "setAttribute",
            node_index: nodeIndex,
            name: qualifiedName,
            value: stringValue,
            namespace_uri: namespaceURI === null ? "" : namespaceURI,
          }});
        }},
        setAttributeNS(namespace, qualifiedName, nextValue) {{
          setFrameNamespacedAttribute(namespace, qualifiedName, nextValue);
        }},
        removeAttribute(name) {{
          const key = String(name).toLowerCase();
          const oldValue = Object.prototype.hasOwnProperty.call(attributes, key)
            ? String(attributes[key])
            : null;
          delete attributes[key];
          delete attributeNamespaces[key];
          removeFrameInlineAttributeHandler(key, oldValue !== null);
          if (key === "disabled") disabled = false;
          if (key === "hidden") hidden = false;
          if (key === "multiple") multiple = false;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "removeAttribute", node_index: nodeIndex, name: key, namespace_uri: "" }});
        }},
        removeAttributeNS(namespace, name) {{
          removeFrameNamespacedAttribute(namespace, name);
        }},
        appendChild(child) {{
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) projected.appendChild(fragmentChild);
            child.__glassChildren = [];
            return child;
          }}
          if (!child || typeof child.nodeIndex !== "number") throw new TypeError("child must be a native node");
          if (child === projected) throw new TypeError("a node cannot contain itself");
          let ancestor = projected;
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
          const oldParentIndex = child.parentIndex;
          projected.__glassChildren = projected.__glassChildren.filter(candidate => candidate !== child);
          projected.__glassChildren.push(child);
          child.__glassParent = projected;
          child.parentIndex = nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (projected.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "appendChild", parent_index: nodeIndex, child_index: child.nodeIndex }});
          return child;
        }},
        insertBefore(child, before) {{
          if (child === this) throw new TypeError("a node cannot contain itself");
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) projected.insertBefore(fragmentChild, before);
            child.__glassChildren = [];
            return child;
          }}
          if (before == null) return this.appendChild(child);
          if (!child || typeof child.nodeIndex !== "number"
              || typeof before.nodeIndex !== "number") throw new TypeError("insertBefore requires native nodes");
          if (before.__glassParent !== projected) throw new TypeError("reference node is not a child");
          if (child === before) return child;
          let ancestor = projected;
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
          const oldParentIndex = child.parentIndex;
          projected.__glassChildren = projected.__glassChildren.filter(candidate => candidate !== child);
          const index = projected.__glassChildren.indexOf(before);
          projected.__glassChildren.splice(index < 0 ? projected.__glassChildren.length : index, 0, child);
          child.__glassParent = projected;
          child.parentIndex = nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (projected.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "insertBefore", parent_index: nodeIndex, child_index: child.nodeIndex, before_index: before.nodeIndex }});
          return child;
        }},
        remove() {{
          const parent = projected.__glassParent || null;
          if (!parent && projected.parentIndex === null) return;
          const commitRemoval = !projected.__glassCreated || projected.parentIndex !== null;
          queueFragmentChildRemoval(parent, projected);
          if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== projected);
          projected.__glassParent = null;
          projected.parentIndex = null;
          detachFrameSubtree(projected);
          if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
          if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: nodeIndex }});
        }},
        removeChild(child) {{
          if (!child || child.__glassParent !== projected) throw new TypeError("child is not contained by this element");
          child.remove();
          return child;
        }},
      }};
      installCommonAttributeProperties(projected, {{
        disabled: () => disabled,
        hidden: () => hidden,
        multiple: () => multiple,
      }}, String(currentBinding.url));
      Object.defineProperty(projected, "__glassChildren", {{ enumerable: false, configurable: false, writable: true, value: [] }});
      Object.defineProperty(projected, "__glassEventOwner", {{
        enumerable: false,
        configurable: false,
        get() {{ return "frame:" + currentFrameId + ":node:" + projected.nodeIndex; }},
      }});
      Object.defineProperty(projected, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
      Object.defineProperty(projected, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
      Object.defineProperty(projected, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: false }});
      Object.defineProperty(projected, "__glassTemplateContentRef", {{ enumerable: false, configurable: false, writable: true, value: null }});
      if (namespaceURI === HTML_NAMESPACE && normalized === "template") {{
        Object.defineProperty(projected, "content", {{
          enumerable: true,
          configurable: false,
          get() {{
            if (templateContent) return templateContent;
            if (projected.__glassTemplateContentRef) {{
              templateContent = projected.__glassTemplateContentRef;
              return templateContent;
            }}
            const indexed = templateContentIndex === null ? null : frameNodesByIndex.get(templateContentIndex);
            if (indexed && Number(indexed.nodeType) === 11) {{
              templateContent = indexed;
              indexed.__glassTemplateHost = projected;
            }} else {{
              templateContent = makeFrameDocumentFragment(null, projected);
            }}
            projected.__glassTemplateContentRef = templateContent;
            return templateContent;
          }},
        }});
      }}
      Object.defineProperty(projected, "__glassSyncContent", {{
        enumerable: false,
        configurable: false,
        value(clearEmpty = false) {{
          if (projected.__glassChildren.length > 0) {{
            innerHtml = projected.__glassChildren.map(child => child.__glassMarkup).join("");
            textContent = projected.__glassChildren.map(child => child.__glassTextValue).join("");
          }} else if (clearEmpty) {{
            innerHtml = "";
            textContent = "";
          }}
          if (projected.__glassParent && typeof projected.__glassParent.__glassSyncContent === "function") projected.__glassParent.__glassSyncContent(clearEmpty);
        }},
      }});
      Object.defineProperty(projected, "__glassSetNativeIndex", {{ enumerable: false, configurable: false, value(nextIndex) {{
        nodeIndex = Number(nextIndex);
        projected.nodeIndex = nodeIndex;
      }} }});
      Object.defineProperty(projected, "__glassRefresh", {{
        enumerable: false,
        configurable: false,
        value(nextEntry, nextExactTextForNode = frameSnapshotTextContentForNode) {{
          nodeIndex = Number(nextEntry.nodeIndex);
          projected.nodeIndex = nodeIndex;
          projected.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
          projected.tagName = tagNameForEntry(nextEntry);
          projected.nodeName = tagNameForEntry(nextEntry);
          projected.localName = String(nextEntry.tagName || "").toLowerCase();
          for (const name of Object.keys(attributes)) delete attributes[name];
          const nextAttributes = nextEntry.attributes && typeof nextEntry.attributes === "object"
            ? nextEntry.attributes
            : {{}};
          for (const [name, nextValue] of Object.entries(nextAttributes)) attributes[name] = String(nextValue);
          for (const name of Object.keys(attributeNamespaces)) delete attributeNamespaces[name];
          const nextAttributeNamespaces = nextEntry.attributeNamespaces && typeof nextEntry.attributeNamespaces === "object"
            ? nextEntry.attributeNamespaces
            : {{}};
          for (const [name, namespace] of Object.entries(nextAttributeNamespaces)) attributeNamespaces[name] = String(namespace);
          refreshFrameInlineAttributeHandlers();
          textContent = snapshotElementTextContent(nextEntry, nextExactTextForNode);
          innerHtml = String(nextEntry.innerHtml || "");
          templateContentIndex = nextEntry.templateContentIndex == null ? null : Number(nextEntry.templateContentIndex);
          value = nextEntry.value == null ? "" : nextEntry.value;
          checked = Boolean(nextEntry.checked);
          indeterminate = Boolean(nextEntry.indeterminate);
          selected = Boolean(nextEntry.selected);
          disabled = Boolean(nextEntry.disabled);
          hidden = Boolean(nextEntry.hidden);
          multiple = Object.prototype.hasOwnProperty.call(attributes, "multiple");
          projected.__glassAttached = true;
          if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        }},
      }});
      Object.defineProperty(projected, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return textContent; }} }});
      Object.defineProperty(projected, "__glassMarkup", {{
        enumerable: false,
        configurable: false,
        get() {{
          const markup = Object.keys(attributes).sort().map(name => " " + name + "=\"" + escapeHtmlText(attributes[name]).replace(/\"/g, "&quot;") + "\"").join("");
          const opening = "<" + normalized + markup + ">";
          if (["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"].includes(normalized)) return opening;
          const content = projected.namespaceURI === HTML_NAMESPACE && projected.localName === "template"
            ? projected.content.innerHTML
            : projected.__glassChildren.length > 0 ? projected.__glassChildren.map(child => child.__glassMarkup).join("") : innerHtml;
          return opening + content + "</" + normalized + ">";
        }},
      }});
      Object.defineProperty(projected, "outerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return projected.__glassMarkup; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame element outerHTML exceeds its limit");
          const owner = projected.__glassParent || null;
          if (!owner || typeof owner.insertBefore !== "function") throw new TypeError("outerHTML requires an attached element");
          const fragment = makeFrameDocumentFragment();
          populateDetachedFragment(fragment, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment, owner);
          for (const child of fragment.__glassChildren.slice()) owner.insertBefore(child, projected);
          projected.remove();
        }},
      }});
      Object.defineProperty(projected, "parentElement", {{ enumerable: false, configurable: false, get() {{ return projected.__glassParent && projected.__glassParent.nodeType === 1 ? projected.__glassParent : null; }} }});
      Object.defineProperty(projected, "parentNode", {{ enumerable: false, configurable: false, get() {{ return projected.__glassParent || projected.parentElement; }} }});
      for (const property of ["textContent", "innerText"]) {{
        Object.defineProperty(projected, property, {{
          enumerable: true,
          configurable: false,
          get() {{ return textContent; }},
          set(next) {{
            const value = String(next);
            if (value.length > storageValueLimit) throw new RangeError("native frame element textContent exceeds its limit");
            for (const child of projected.__glassChildren) {{ child.__glassParent = null; child.parentIndex = null; detachFrameSubtree(child); }}
            projected.__glassChildren = [];
            innerHtml = "";
            textContent = "";
            suppressHostCommands += 1;
            try {{
              if (value) projected.appendChild(makeFrameDetachedText(value));
            }} finally {{
              suppressHostCommands -= 1;
            }}
            projected.__glassSyncContent(true);
            queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: nodeIndex, value }});
          }},
        }});
      }}
      Object.defineProperty(projected, "innerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return projected.localName === "template" ? projected.content.innerHTML : innerHtml; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame element innerHTML exceeds its limit");
          if (namespaceURI === HTML_NAMESPACE && projected.localName === "template") {{
            const content = projected.content;
            for (const child of content.__glassChildren) {{
              child.__glassParent = null;
              child.parentIndex = null;
              detachFrameSubtree(child);
            }}
            content.__glassChildren = [];
            innerHtml = "";
            textContent = "";
            suppressHostCommands += 1;
            try {{
              populateDetachedFragment(content, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment, projected);
            }} finally {{
              suppressHostCommands -= 1;
            }}
            innerHtml = value;
            projected.__glassSyncContent(true);
            queueFrameCommand(currentBinding, {{ kind: "setInnerHtml", node_index: nodeIndex, value }});
            return;
          }}
          for (const child of projected.__glassChildren) {{ child.__glassParent = null; child.parentIndex = null; detachFrameSubtree(child); }}
          projected.__glassChildren = [];
          innerHtml = "";
          textContent = "";
          suppressHostCommands += 1;
          try {{
            populateDetachedFragment(projected, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment);
          }} finally {{
            suppressHostCommands -= 1;
          }}
          projected.__glassSyncContent(true);
          queueFrameCommand(currentBinding, {{ kind: "setInnerHtml", node_index: nodeIndex, value }});
        }},
      }});
      Object.defineProperty(projected, "value", {{ enumerable: true, configurable: false, get() {{ return value; }}, set(next) {{ value = String(next); queueFrameCommand(currentBinding, {{ kind: "setValue", node_index: nodeIndex, value }}); }} }});
      Object.defineProperty(projected, "checked", {{ enumerable: true, configurable: false, get() {{ return checked; }}, set(next) {{ checked = Boolean(next); queueFrameCommand(currentBinding, {{ kind: "setChecked", node_index: nodeIndex, checked }}); }} }});
      Object.defineProperty(projected, "indeterminate", {{ enumerable: true, configurable: false, get() {{ return indeterminate; }}, set(next) {{ if (String(projected.tagName || "").toLowerCase() !== "input") throw new TypeError("indeterminate requires an input element"); indeterminate = Boolean(next); queueFrameCommand(currentBinding, {{ kind: "setIndeterminate", node_index: nodeIndex, indeterminate }}); }} }});
      Object.defineProperty(projected, "selected", {{ enumerable: true, configurable: false, get() {{ return selected; }}, set(next) {{ selected = Boolean(next); queueFrameCommand(currentBinding, {{ kind: "setSelected", node_index: nodeIndex, selected }}); }} }});
      Object.defineProperty(projected, "__glassSetFormResetState", {{
        enumerable: false,
        configurable: false,
        value(next) {{
          if (Object.prototype.hasOwnProperty.call(next, "value")) value = String(next.value);
          if (Object.prototype.hasOwnProperty.call(next, "checked")) checked = Boolean(next.checked);
          if (Object.prototype.hasOwnProperty.call(next, "selected")) selected = Boolean(next.selected);
        }},
      }});
      if (projected.tagName === "FORM") Object.defineProperty(projected, "reset", {{
        enumerable: false,
        configurable: false,
        value() {{ return nativeFormReset(projected, command => queueFrameCommand(currentBinding, command)); }},
      }});
      Object.defineProperty(projected, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return frameTemplateContentsOwnerDocumentForNode(projected); }},
      }});
      Object.defineProperty(projected, "__glassAttributeSource", {{
        enumerable: false,
        configurable: false,
        value: () => attributes,
      }});
      Object.defineProperty(projected, "__glassAttributeNamespace", {{
        enumerable: false,
        configurable: false,
        value: (name) => frameAttributeNamespace(name),
      }});
      installAttributeNodeSurface(projected, () => frameTemplateContentsOwnerDocumentForNode(projected));
      try {{ Object.setPrototypeOf(projected, elementPrototypeFor(projected.tagName)); }} catch (_error) {{}}
      defineTreeAccessors(projected);
      installClassList(projected);
      installElementStyleAndDataset(projected);
      frameMutationNodes.set(frameMutationKey(currentBinding, nodeIndex), projected);
      frameMutationAttributes.set(frameMutationKey(currentBinding, nodeIndex), {{}});
      frameMutationChildren.set(frameMutationKey(currentBinding, nodeIndex), []);
      frameScriptNodeObjects.set(nodeIndex, projected);
      queueFrameCommand(currentBinding, {{
        kind: "createElement",
        node_index: nodeIndex,
        tag_name: normalized,
        namespace_uri: namespaceURI === null ? "" : namespaceURI,
      }});
      return projected;
    }};
    const makeFrameDetachedText = (value) => {{
      let textContent = String(value);
      if (textContent.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
      let nodeIndex = allocateTemporaryNodeIndex();
      const text = {{ nodeIndex, parentIndex: null, __glassFrameGeneration: frameGeneration, nodeType: 3, nodeName: "#text", nodeValue: textContent }};
      Object.defineProperty(text, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
      Object.defineProperty(text, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
      Object.defineProperty(text, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: false }});
      Object.defineProperty(text, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return textContent; }} }});
      Object.defineProperty(text, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return serializeHtmlTextNode(text, textContent); }} }});
      Object.defineProperty(text, "parentElement", {{ enumerable: false, configurable: false, get() {{
        let current = text.__glassParent || null;
        while (current && current.nodeType !== 1) current = current.__glassParent || null;
        return current || null;
      }} }});
      Object.defineProperty(text, "parentNode", {{
        enumerable: false,
        configurable: false,
        get() {{
          if (text.__glassParent) return text.__glassParent;
          const owner = text.ownerDocument;
          return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(text)
            ? owner
            : null;
        }},
      }});
      Object.defineProperty(text, "nodeValue", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ text.textContent = next; }} }});
      Object.defineProperty(text, "textContent", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ const value = String(next); if (value.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit"); textContent = value; if (text.__glassParent && typeof text.__glassParent.__glassSyncContent === "function") text.__glassParent.__glassSyncContent(); queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: nodeIndex, value: textContent }}); }} }});
      Object.defineProperty(text, "data", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ text.textContent = next; }} }});
      Object.defineProperty(text, "__glassRefresh", {{
        enumerable: false,
        configurable: false,
        value(nextEntry) {{
          nodeIndex = Number(nextEntry.nodeIndex);
          text.nodeIndex = nodeIndex;
          text.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
          textContent = String(nextEntry.nodeValue || "");
          text.__glassAttached = true;
        }},
      }});
      Object.defineProperty(text, "__glassSetNativeIndex", {{ enumerable: false, configurable: false, value(nextIndex) {{
        nodeIndex = Number(nextIndex);
        text.nodeIndex = nodeIndex;
      }} }});
      Object.defineProperty(text, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return frameTemplateContentsOwnerDocumentForNode(text); }},
      }});
      text.remove = () => {{
        const parent = text.__glassParent || null;
        if (!parent && text.parentIndex === null) return;
        const commitRemoval = !text.__glassCreated || text.parentIndex !== null;
        queueFragmentChildRemoval(parent, text);
        if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== text);
        text.__glassParent = null;
        text.parentIndex = null;
        detachFrameSubtree(text);
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: nodeIndex }});
      }};
      defineTreeAccessors(text);
      frameMutationNodes.set(frameMutationKey(currentBinding, nodeIndex), text);
      frameMutationText.set(frameMutationKey(currentBinding, nodeIndex), textContent);
      frameMutationChildren.set(frameMutationKey(currentBinding, nodeIndex), []);
      frameScriptNodeObjects.set(nodeIndex, text);
      queueFrameCommand(currentBinding, {{ kind: "createTextNode", node_index: nodeIndex, value: textContent }});
      try {{ Object.setPrototypeOf(text, TextNative.prototype); }} catch (_error) {{}}
      return text;
    }};
    const makeFrameDetachedComment = (value) => {{
      let textContent = String(value);
      if (textContent.length > {storage_value_limit}) throw new RangeError("native frame comment node exceeds its limit");
      let nodeIndex = allocateTemporaryNodeIndex();
      const comment = {{ nodeIndex, parentIndex: null, __glassFrameGeneration: frameGeneration, nodeType: 8, nodeName: "#comment", nodeValue: textContent }};
      Object.defineProperty(comment, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
      Object.defineProperty(comment, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
      Object.defineProperty(comment, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: false }});
      Object.defineProperty(comment, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return ""; }} }});
      Object.defineProperty(comment, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return "<!--" + textContent + "-->"; }} }});
      Object.defineProperty(comment, "parentElement", {{ enumerable: false, configurable: false, get() {{
        let current = comment.__glassParent || null;
        while (current && current.nodeType !== 1) current = current.__glassParent || null;
        return current || null;
      }} }});
      Object.defineProperty(comment, "parentNode", {{ enumerable: false, configurable: false, get() {{ return comment.__glassParent || null; }} }});
      Object.defineProperty(comment, "nodeValue", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ comment.textContent = next; }} }});
      Object.defineProperty(comment, "textContent", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{
        const nextValue = String(next);
        if (nextValue.length > {storage_value_limit}) throw new RangeError("native frame comment node exceeds its limit");
        textContent = nextValue;
        if (comment.__glassParent && typeof comment.__glassParent.__glassSyncContent === "function") comment.__glassParent.__glassSyncContent();
        queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: nodeIndex, value: nextValue }});
      }} }});
      Object.defineProperty(comment, "data", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ comment.textContent = next; }} }});
      Object.defineProperty(comment, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return frameTemplateContentsOwnerDocumentForNode(comment); }} }});
      Object.defineProperty(comment, "__glassRefresh", {{ enumerable: false, configurable: false, value(nextEntry) {{
        nodeIndex = Number(nextEntry.nodeIndex);
        comment.nodeIndex = nodeIndex;
        comment.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        textContent = String(nextEntry.nodeValue || "");
      }} }});
      Object.defineProperty(comment, "__glassSetNativeIndex", {{ enumerable: false, configurable: false, value(nextIndex) {{
        nodeIndex = Number(nextIndex);
        comment.nodeIndex = nodeIndex;
      }} }});
      comment.remove = () => {{
        const parent = comment.__glassParent || null;
        if (!parent && comment.parentIndex === null) return;
        const commitRemoval = !comment.__glassCreated || comment.parentIndex !== null;
        queueFragmentChildRemoval(parent, comment);
        if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== comment);
        comment.__glassParent = null;
        comment.parentIndex = null;
        detachFrameSubtree(comment);
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: nodeIndex }});
      }};
      defineTreeAccessors(comment);
      frameMutationNodes.set(frameMutationKey(currentBinding, nodeIndex), comment);
      frameMutationText.set(frameMutationKey(currentBinding, nodeIndex), textContent);
      frameMutationChildren.set(frameMutationKey(currentBinding, nodeIndex), []);
      frameScriptNodeObjects.set(nodeIndex, comment);
      queueFrameCommand(currentBinding, {{ kind: "createComment", node_index: nodeIndex, value: textContent }});
      try {{ Object.setPrototypeOf(comment, CommentNative.prototype); }} catch (_error) {{}}
      return comment;
    }};
    const makeFrameDetachedDocumentType = (
      name,
      publicId = "",
      systemId = "",
      ownerDocumentResolver = () => frameDocumentCache.get(currentFrameId)?.document || frameDocument,
    ) => {{
      let normalizedName = String(name);
      if (!/^[A-Za-z][A-Za-z0-9:_-]*$/.test(normalizedName)) throw new TypeError("invalid document type name");
      let publicIdentifier = String(publicId);
      let systemIdentifier = String(systemId);
      if (publicIdentifier.length > {storage_value_limit} || systemIdentifier.length > {storage_value_limit}) throw new RangeError("native frame document type identifier exceeds its limit");
      let nodeIndex = allocateTemporaryNodeIndex();
      const documentType = {{
        nodeIndex,
        parentIndex: null,
        __glassFrameGeneration: frameGeneration,
        nodeType: 10,
        nodeName: normalizedName,
        nodeValue: null,
        name: normalizedName,
        publicId: publicIdentifier,
        systemId: systemIdentifier,
        textContent: null,
        __glassChildren: [],
        __glassParent: null,
        remove() {{
          const parent = documentType.__glassParent || null;
          if (!parent && documentType.parentIndex === null) return;
          const commitRemoval = !documentType.__glassCreated || documentType.parentIndex !== null;
          if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== documentType);
          documentType.__glassParent = null;
          documentType.parentIndex = null;
          if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: nodeIndex }});
        }},
      }};
      Object.defineProperty(documentType, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
      Object.defineProperty(documentType, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: false }});
      Object.defineProperty(documentType, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{
        const publicPart = publicIdentifier ? " PUBLIC \"" + publicIdentifier + "\"" : "";
        const systemPart = systemIdentifier ? (publicPart ? " \"" + systemIdentifier + "\"" : " SYSTEM \"" + systemIdentifier + "\"") : "";
        return "<!DOCTYPE " + normalizedName + publicPart + systemPart + ">";
      }} }});
      Object.defineProperty(documentType, "__glassRefresh", {{ enumerable: false, configurable: false, value(nextEntry) {{
        nodeIndex = Number(nextEntry.nodeIndex);
        documentType.nodeIndex = nodeIndex;
        documentType.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        normalizedName = String(nextEntry.nodeName || normalizedName);
        publicIdentifier = nextEntry.publicId == null ? "" : String(nextEntry.publicId);
        systemIdentifier = nextEntry.systemId == null ? "" : String(nextEntry.systemId);
        documentType.name = normalizedName;
        documentType.nodeName = normalizedName;
        documentType.publicId = publicIdentifier;
        documentType.systemId = systemIdentifier;
      }} }});
      Object.defineProperty(documentType, "__glassSetNativeIndex", {{ enumerable: false, configurable: false, value(nextIndex) {{
        nodeIndex = Number(nextIndex);
        documentType.nodeIndex = nodeIndex;
      }} }});
      Object.defineProperty(documentType, "parentNode", {{ enumerable: false, configurable: false, get() {{ return documentType.__glassParent || null; }} }});
      Object.defineProperty(documentType, "parentElement", {{ enumerable: false, configurable: false, get() {{ return null; }} }});
      Object.defineProperty(documentType, "ownerDocument", {{ enumerable: false, configurable: false, get() {{
        return documentType.__glassAdoptedOwnerDocument || ownerDocumentResolver() || null;
      }} }});
      defineTreeAccessors(documentType);
      frameMutationNodes.set(frameMutationKey(currentBinding, nodeIndex), documentType);
      frameMutationChildren.set(frameMutationKey(currentBinding, nodeIndex), []);
      frameScriptNodeObjects.set(nodeIndex, documentType);
      queueFrameCommand(currentBinding, {{ kind: "createDocumentType", node_index: nodeIndex, name: normalizedName, public_id: publicIdentifier, system_id: systemIdentifier }});
      try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
      return documentType;
    }};
    const makeFrameDocumentFragment = (snapshotEntry = null, templateHost = null) => {{
      const fragment = {{
        nodeIndex: snapshotEntry ? Number(snapshotEntry.nodeIndex) : null,
        parentIndex: snapshotEntry && snapshotEntry.parentIndex != null ? Number(snapshotEntry.parentIndex) : null,
        __glassFrameGeneration: frameGeneration,
        nodeType: 11,
        nodeName: "#document-fragment",
        __glassFragment: true,
        __glassTemplateHost: templateHost,
        templateHostIndex: snapshotEntry && snapshotEntry.templateHostIndex != null
          ? Number(snapshotEntry.templateHostIndex)
          : null,
        __glassChildren: [],
        __glassParent: null,
        __glassAttached: false,
        appendChild(child) {{
          if (child === this) throw new TypeError("a node cannot contain itself");
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) this.appendChild(fragmentChild);
            child.__glassChildren = [];
            return child;
          }}
          if (!child || ![1, 3, 8].includes(Number(child.nodeType)))
            throw new TypeError("DocumentFragment children must be elements or text nodes");
          const originalOwnerDocument = child.ownerDocument || null;
          if (!child.__glassOwnerDocumentOverride && originalOwnerDocument) {{
            Object.defineProperty(child, "__glassOwnerDocumentOverride", {{ value: originalOwnerDocument }});
          }}
          const oldParent = child.__glassParent || null;
          if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
            queueFragmentChildRemoval(oldParent, child);
            oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
            if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
          }}
          const oldParentIndex = child.parentIndex;
          this.__glassChildren = this.__glassChildren.filter(candidate => candidate !== child);
          this.__glassChildren.push(child);
          child.__glassParent = this;
          child.parentIndex = this.nodeIndex == null ? null : this.nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (!suppressHostCommands) {{
            if (this.nodeIndex !== null && this.nodeIndex !== undefined) {{
              queueFrameCommand(currentBinding, {{ kind: "appendChild", parent_index: this.nodeIndex, child_index: child.nodeIndex }});
            }} else if (this.__glassTemplateHost) {{
              queueFrameCommand(currentBinding, {{
                kind: "setInnerHtml",
                node_index: this.__glassTemplateHost.nodeIndex,
                value: this.innerHTML,
              }});
            }}
          }}
          queueMutation({{ type: "childList", target: this, addedNodes: [child], removedNodes: [], previousSibling: this.__glassChildren.length > 1 ? this.__glassChildren[this.__glassChildren.length - 2] : null, nextSibling: null }});
          if (this.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          return child;
        }},
        insertBefore(child, before) {{
          if (child === this) throw new TypeError("a node cannot contain itself");
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) this.insertBefore(fragmentChild, before);
            child.__glassChildren = [];
            return child;
          }}
          if (before == null) return this.appendChild(child);
          if (!child || ![1, 3, 8].includes(Number(child.nodeType)))
            throw new TypeError("DocumentFragment children must be elements or text nodes");
          if (before.__glassParent !== this) throw new TypeError("reference node is not a child");
          const originalOwnerDocument = child.ownerDocument || null;
          if (!child.__glassOwnerDocumentOverride && originalOwnerDocument) {{
            Object.defineProperty(child, "__glassOwnerDocumentOverride", {{ value: originalOwnerDocument }});
          }}
          const oldParent = child.__glassParent || null;
          if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
            queueFragmentChildRemoval(oldParent, child);
            oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
            if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
          }}
          const oldParentIndex = child.parentIndex;
          this.__glassChildren = this.__glassChildren.filter(candidate => candidate !== child);
          const index = this.__glassChildren.indexOf(before);
          const previousSibling = index > 0 ? this.__glassChildren[index - 1] : null;
          const nextSibling = before;
          this.__glassChildren.splice(index < 0 ? this.__glassChildren.length : index, 0, child);
          child.__glassParent = this;
          child.parentIndex = this.nodeIndex == null ? null : this.nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (!suppressHostCommands) {{
            if (this.nodeIndex !== null && this.nodeIndex !== undefined) {{
              queueFrameCommand(currentBinding, {{ kind: "insertBefore", parent_index: this.nodeIndex, child_index: child.nodeIndex, before_index: before.nodeIndex }});
            }} else if (this.__glassTemplateHost) {{
              queueFrameCommand(currentBinding, {{
                kind: "setInnerHtml",
                node_index: this.__glassTemplateHost.nodeIndex,
                value: this.innerHTML,
              }});
            }}
          }}
          queueMutation({{ type: "childList", target: this, addedNodes: [child], removedNodes: [], previousSibling, nextSibling }});
          if (this.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          return child;
        }},
      }};
      Object.defineProperty(fragment, "__glassTextValue", {{
        enumerable: false,
        configurable: false,
        get() {{ return fragment.__glassChildren.map(child => child.__glassTextValue || "").join(""); }},
      }});
      Object.defineProperty(fragment, "__glassMarkup", {{
        enumerable: false,
        configurable: false,
        get() {{ return fragment.__glassChildren.map(child => child.__glassMarkup || "").join(""); }},
      }});
      Object.defineProperty(fragment, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return frameTemplateContentsOwnerDocumentForNode(fragment); }},
      }});
      Object.defineProperty(fragment, "parentNode", {{
        enumerable: false,
        configurable: false,
        get() {{ return null; }},
      }});
      Object.defineProperty(fragment, "parentElement", {{
        enumerable: false,
        configurable: false,
        get() {{ return null; }},
      }});
      Object.defineProperty(fragment, "textContent", {{
        enumerable: true,
        configurable: false,
        get() {{ return fragment.__glassTextValue; }},
        set(next) {{
          for (const child of fragment.__glassChildren.slice()) child.remove();
          const value = String(next);
          if (value) fragment.appendChild(fragment.ownerDocument.createTextNode(value));
          }},
        }});
      Object.defineProperty(fragment, "innerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return fragment.__glassMarkup; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame fragment innerHTML exceeds its limit");
          for (const child of fragment.__glassChildren.slice()) child.remove();
          populateDetachedFragment(
            fragment,
            value,
            makeFrameDetachedElement,
            makeFrameDetachedText,
            makeFrameDetachedComment,
            fragment.__glassTemplateHost || fragment,
          );
          if (fragment.__glassTemplateHost) {{
            queueFrameCommand(currentBinding, {{
              kind: "setInnerHtml",
              node_index: fragment.__glassTemplateHost.nodeIndex,
              value,
            }});
          }}
        }},
      }});
      Object.defineProperty(fragment, "__glassRefresh", {{
        enumerable: false,
        configurable: false,
        value(nextEntry, nextTemplateHost = null) {{
          fragment.nodeIndex = Number(nextEntry.nodeIndex);
          fragment.parentIndex = nextEntry.parentIndex == null ? null : Number(nextEntry.parentIndex);
          fragment.templateHostIndex = nextEntry.templateHostIndex == null ? null : Number(nextEntry.templateHostIndex);
          if (nextTemplateHost) fragment.__glassTemplateHost = nextTemplateHost;
        }},
      }});
      if (templateHost) {{
        Object.defineProperty(fragment, "__glassOwnerDocumentOverride", {{
          value: getFrameTemplateContentsOwnerDocument(),
        }});
      }}
      defineTreeAccessors(fragment);
      const constructor = globalThis.DocumentFragment;
      if (typeof constructor === "function" && constructor.prototype) {{
        try {{ Object.setPrototypeOf(fragment, constructor.prototype); }} catch (_error) {{}}
      }}
      return fragment;
    }};
    const frameFragmentNodes = frameNodeSnapshots
      .filter((entry) => entry && Number(entry.nodeType) === 11)
      .map((entry) => {{
        const nodeIndex = Number(entry.nodeIndex);
        const hostIndex = entry.templateHostIndex == null ? null : Number(entry.templateHostIndex);
        const templateHost = hostIndex === null ? null : frameNodesByIndex.get(hostIndex) || null;
        const key = frameMutationKey(currentBinding, nodeIndex);
        const cached = frameMutationNodes.get(key)
          || (templateHost && templateHost.__glassTemplateContentRef);
        const fragment = cached && cached.__glassFrameGeneration === frameGeneration
          && typeof cached.__glassRefresh === "function"
          ? cached
          : makeFrameDocumentFragment(entry, templateHost);
        fragment.__glassChildren = [];
        fragment.__glassParent = null;
        fragment.__glassAttached = false;
        fragment.__glassRefresh(entry, templateHost);
        frameNodesByIndex.set(nodeIndex, fragment);
        frameMutationNodes.set(key, fragment);
        frameMutationChildren.set(key, Array.isArray(entry.children) ? entry.children.map(Number) : []);
        if (templateHost) {{
          templateHost.__glassTemplateContentRef = fragment;
          fragment.__glassTemplateHost = templateHost;
        }}
        for (const childIndex of Array.isArray(entry.children) ? entry.children : []) {{
          const child = frameNodesByIndex.get(Number(childIndex));
          if (!child) continue;
          fragment.__glassChildren.push(child);
          child.__glassParent = fragment;
          child.parentIndex = nodeIndex;
        }}
        if (templateHost) markFrameTemplateContentsOwner(fragment, getFrameTemplateContentsOwnerDocument());
        detachFrameSubtree(fragment);
        return fragment;
      }});
    const find = (selector) => frameElements.filter((element) =>
      element.__glassAttached && projectedFrameMatches(element, selector, documentElement));
    const findById = (id) => frameElements.find((element) => element.__glassAttached && element.id === String(id)) || null;
    const body = frameElements.find((element) => element.tagName === "BODY") || null;
    const documentElement = frameElements.find((element) => element.tagName === "HTML") || null;
    const head = frameElements.find((element) => element.tagName === "HEAD") || null;
    const frameRootSnapshot = frameNodeSnapshots.find((entry) => entry && Number(entry.nodeIndex) === 0);
    const frameRootChildren = frameRootSnapshot && Array.isArray(frameRootSnapshot.children)
      ? frameRootSnapshot.children.map((index) => frameNodesByIndex.get(index)).filter(Boolean)
      : frameElements.filter((element) => element.parentIndex === null);
    const frameSnapshotsByIndex = new Map(frameNodeSnapshots.map((entry) => [Number(entry.nodeIndex), entry]));
    const attachedNodeIndices = new Set();
    const pendingNodeIndices = frameRootSnapshot && Array.isArray(frameRootSnapshot.children)
      ? frameRootSnapshot.children.map(Number)
      : frameRootChildren.map((node) => Number(node.nodeIndex));
    while (pendingNodeIndices.length > 0) {{
      const nodeIndex = pendingNodeIndices.pop();
      if (attachedNodeIndices.has(nodeIndex)) continue;
      attachedNodeIndices.add(nodeIndex);
      const nodeSnapshot = frameSnapshotsByIndex.get(nodeIndex);
      if (nodeSnapshot && Array.isArray(nodeSnapshot.children)) {{
        for (const childIndex of nodeSnapshot.children) pendingNodeIndices.push(Number(childIndex));
      }}
    }}
    for (const node of [...frameElements, ...frameTextNodes, ...frameDocumentTypeNodes]) {{
      node.__glassAttached = attachedNodeIndices.has(Number(node.nodeIndex));
    }}
    frameDocument = {{
      nodeType: 9,
      nodeName: "#document",
      __glassFrameGeneration: frameGeneration,
      __glassNativeContextId: currentFrameId,
      __glassNativeGeneration: Number(frameGeneration) || 0,
      __glassNativeOrigin: String(currentBinding.origin || "null"),
      __glassNativeScriptNodes: Array.isArray(snapshot.scriptNodes) ? snapshot.scriptNodes : [],
      __glassNativeFrameBinding: currentBinding,
      __glassGetTemplateContentsOwnerDocument() {{ return getFrameTemplateContentsOwnerDocument(); }},
      URL: String(currentBinding.url),
      documentURI: String(currentBinding.url),
      get title() {{
        const titleElement = frameElements.find((element) => element.__glassAttached && element.tagName === "TITLE");
        return titleElement ? titleElement.textContent : frameDocumentTitle;
      }},
      set title(value) {{
        const text = String(value);
        if (text.length > storageValueLimit) throw new RangeError("native frame document.title exceeds its limit");
        frameDocumentTitle = text;
        const titleElement = frameElements.find((element) => element.__glassAttached && element.tagName === "TITLE");
        if (titleElement) {{
          titleElement.textContent = text;
          return;
        }}
        let parent = frameDocument.head;
        if (!parent && documentElement) {{
          parent = makeFrameDetachedElement("head");
          documentElement.insertBefore(parent, documentElement.firstChild);
        }}
        if (parent) {{
          const created = makeFrameDetachedElement("title");
          created.textContent = text;
          parent.appendChild(created);
          return;
        }}
        queueFrameCommand(currentBinding, {{ kind: "setDocumentTitle", value: text }});
      }},
      textContent: String(snapshot.visibleText || ""),
      body,
      documentElement,
      get head() {{ return frameElements.find((element) => element.__glassAttached && element.tagName === "HEAD") || head; }},
      get doctype() {{ return frameDocument.__glassChildren.find((node) => Number(node.nodeType) === 10) || null; }},
      get forms() {{ return liveCollection(frameDocument, (element) => element.tagName === "FORM", "HTMLCollection", true); }},
      get links() {{ return liveCollection(frameDocument, (element) => ["A", "AREA"].includes(element.tagName) && element.getAttribute("href") !== null, "HTMLCollection", true); }},
      get scripts() {{ return liveCollection(frameDocument, (element) => element.tagName === "SCRIPT", "HTMLCollection", true); }},
      get images() {{ return liveCollection(frameDocument, (element) => element.tagName === "IMG", "HTMLCollection", true); }},
      get scrollingElement() {{ return documentElement; }},
      get defaultView() {{
        const current = frameBindingForId(currentFrameId) || currentBinding;
        return currentFrameWindow(current, ownerFrameElement, parentWindow, topWindow);
      }},
      createElement(tagName) {{ return makeFrameDetachedElement(tagName, HTML_NAMESPACE); }},
      createElementNS(namespace, qualifiedName) {{
        return makeFrameDetachedElement(qualifiedName, normalizeElementNamespace(namespace));
      }},
      importNode(node, options = false) {{
        return importNativeNode(frameDocument, node, options);
      }},
      adoptNode(node) {{ return adoptNativeNode(frameDocument, node); }},
      createAttribute(name) {{
        return makeAttributeNode(name, "", () => frameTemplateContentsOwnerDocumentForNode(null));
      }},
      createAttributeNS(namespace, qualifiedName) {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        return makeAttributeNode(qualifiedName, "", () => frameTemplateContentsOwnerDocumentForNode(null), namespaceURI);
      }},
      createTextNode(value) {{ return makeFrameDetachedText(value); }},
      createComment(value) {{ return makeFrameDetachedComment(value); }},
      appendChild(child) {{
        if (!child || Number(child.nodeType) !== 10) throw new TypeError("Document children must be a document type");
        if (frameDocument.__glassChildren.includes(child)) return child;
        if (child.__glassParent && child.__glassParent !== frameDocument) throw new DOMExceptionNative("The document type has another parent", "HierarchyRequestError");
        const existing = frameDocument.__glassChildren.find((candidate) => Number(candidate.nodeType) === 10);
        if (existing && existing !== child) throw new DOMExceptionNative("The document already has a document type", "HierarchyRequestError");
        frameDocument.__glassChildren = frameDocument.__glassChildren.filter((candidate) => candidate !== child);
        frameDocument.__glassChildren.push(child);
        child.__glassParent = frameDocument;
        child.parentIndex = 0;
        child.__glassAttached = true;
        queueFrameCommand(currentBinding, {{ kind: "appendChild", parent_index: 0, child_index: child.nodeIndex }});
        return child;
      }},
      insertBefore(child, before) {{
        if (before == null) return frameDocument.appendChild(child);
        if (!before || !frameDocument.__glassChildren.includes(before)) throw new TypeError("reference node is not a document child");
        if (!child || Number(child.nodeType) !== 10) throw new TypeError("Document children must be a document type");
        if (child === before) return child;
        const existing = frameDocument.__glassChildren.find((candidate) => Number(candidate.nodeType) === 10);
        if (existing && existing !== child) throw new DOMExceptionNative("The document already has a document type", "HierarchyRequestError");
        frameDocument.__glassChildren = frameDocument.__glassChildren.filter((candidate) => candidate !== child);
        const index = frameDocument.__glassChildren.indexOf(before);
        frameDocument.__glassChildren.splice(index < 0 ? frameDocument.__glassChildren.length : index, 0, child);
        child.__glassParent = frameDocument;
        child.parentIndex = 0;
        child.__glassAttached = true;
        queueFrameCommand(currentBinding, {{ kind: "insertBefore", parent_index: 0, child_index: child.nodeIndex, before_index: before.nodeIndex }});
        return child;
      }},
      implementation: {{
        createDocumentType(name, publicId = "", systemId = "") {{
          return makeFrameDetachedDocumentType(name, publicId, systemId);
        }},
      }},
      createDocumentFragment() {{ return makeFrameDocumentFragment(); }},
      getElementById: findById,
      querySelector(selector) {{ return find(selector)[0] || null; }},
      querySelectorAll(selector) {{ return asNodeList(find(selector)); }},
      getElementsByTagName(name) {{
        const value = String(name).toLowerCase();
        return liveCollection(frameDocument, (element) => value === "*" || element.tagName.toLowerCase() === value, "HTMLCollection", true);
      }},
      getElementsByClassName(name) {{
        const value = String(name);
        return liveCollection(frameDocument, (element) => element.className.split(/\s+/).includes(value), "HTMLCollection", true);
      }},
      getElementsByName(name) {{
        const value = String(name);
        return asNodeList(frameElements.filter((element) => element.__glassAttached
          && element.getAttribute("name") === value));
      }},
      addEventListener(type, callback, options) {{
        addListener(ownerFor(frameDocument), type, callback, options);
      }},
      removeEventListener(type, callback, options) {{
        removeListener(ownerFor(frameDocument), type, callback, options);
      }},
      dispatchEvent(event) {{
        return dispatchTarget(this, event);
      }},
    }};
    Object.defineProperty(frameDocument, "__glassChildren", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: frameRootChildren,
    }});
    for (const child of frameRootChildren) {{
      Object.defineProperty(child, "__glassMutationDocument", {{
        enumerable: false,
        configurable: true,
        writable: true,
        value: frameDocument,
      }});
    }}
    const frameDocumentKey = frameMutationKey(currentBinding, 0);
    frameMutationNodes.set(frameDocumentKey, frameDocument);
    frameMutationChildren.set(
      frameDocumentKey,
      frameRootChildren.map((child) => Number(child.nodeIndex)),
    );
    for (const child of frameRootChildren) {{
      frameMutationParents.set(frameMutationKey(currentBinding, child.nodeIndex), 0);
    }}
    Object.defineProperty(frameDocument, "__glassEventOwner", {{
      enumerable: false,
      configurable: false,
      value: "frame:" + currentFrameId + ":document",
    }});
    Object.defineProperty(frameDocument, "__glassEventTargetForNode", {{
      enumerable: false,
      configurable: false,
      value(nodeIndex) {{
        const index = Number(nodeIndex);
        if (index === 0) return frameDocument;
        if (index === 4294967295) return frameDocument.defaultView;
        const node = frameNodesByIndex.get(index);
        return node && node.__glassAttached ? node : null;
      }},
    }});
    defineTreeAccessors(frameDocument);
    try {{ Object.setPrototypeOf(frameDocument, DocumentNative.prototype); }} catch (_error) {{}}
    frameDocumentCache.set(currentFrameId, {{
      url: currentBinding.url,
      revision: currentBinding.revision,
      topologyKey,
      document: frameDocument,
    }});
    for (const element of frameElements) {{
      if (typeof element.__glassRefreshInlineEventHandlers === "function")
        element.__glassRefreshInlineEventHandlers();
    }}
    return frameDocument;
  }};
  globalThis.__glassDispatchFrameEvents = (frameId, events) => {{
    const identifier = String(frameId || "");
    const binding = frameBindingForId(identifier);
    if (!binding || binding.sameOrigin !== true) throw crossOriginSecurityError("event");
    if (!Array.isArray(events)) throw new TypeError("native frame events must be an array");
    if (events.length > {max_commands}) throw new RangeError("native frame event limit exceeded");
    const projectedDocument = makeFrameDocument(binding);
    const generation = Number(binding.generation);
    const delivered = [];
    for (const descriptor of events) {{
      if (!descriptor || typeof descriptor !== "object") throw new TypeError("native frame event is invalid");
      const nodeIndex = Number(descriptor.node_index);
      if (nodeIndex !== 4294967295 && Number(descriptor.generation) !== generation) continue;
      const target = projectedDocument.__glassEventTargetForNode(nodeIndex);
      if (!target) throw new TypeError("native frame event target is detached");
      const eventType = String(descriptor.type);
      const focusEvent = ["blur", "focusout", "focus", "focusin"].includes(eventType);
      const eventFactory = focusEvent ? createFocusEvent : createEvent;
      const event = eventFactory(eventType, {{
        bubbles: Boolean(descriptor.bubbles),
        cancelable: Boolean(descriptor.cancelable),
        view: projectedDocument.defaultView,
        relatedTarget: descriptor.related_target_node_index == null
          ? null
          : projectedDocument.__glassEventTargetForNode(Number(descriptor.related_target_node_index)) || null,
      }});
      const focusKey = nativeFrameFocusKey(binding);
      if (descriptor.type === "blur" || descriptor.type === "focusout") {{
        if (nativeFrameFocusedNodeIndex(binding) === nodeIndex) {{
          nativeFrameFocusedNodeIndices.set(focusKey,
            descriptor.related_target_node_index == null
              ? null
              : Number(descriptor.related_target_node_index));
        }}
      }} else if (descriptor.type === "focus" || descriptor.type === "focusin") {{
        nativeFrameFocusedNodeIndices.set(focusKey, nodeIndex);
      }}
      delivered.push(dispatchTarget(target, event));
    }}
    return delivered;
  }};
  const makeFrameWindow = (
    binding,
    frameElement,
    parentWindow = globalThis,
    topWindow = globalThis,
  ) => {{
    const currentBinding = frameBindingForId(frameIdentifier(binding)) || binding;
    const currentFrameId = frameIdentifier(currentBinding);
    const proxy = makeWindowProxy(
      "",
      "",
      currentFrameId,
      String(currentBinding.url),
      Boolean(currentBinding.sameOrigin),
    );
    const cacheKey = currentFrameId + "\\u0000";
    const state = windowProxyStates.get(cacheKey);
    if (state) {{
      state.targetLocationHref = String(currentBinding.url);
      state.closed = false;
      state.sameOrigin = Boolean(currentBinding.sameOrigin);
      if (frameElement !== null && frameElement !== undefined) state.frameElement = frameElement;
      state.parentWindow = parentWindow;
      state.topWindow = topWindow;
    }}
    if (!frameWindowCache.has(currentFrameId)) {{
      Object.defineProperty(proxy, "window", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "self", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "parent", {{
        enumerable: true,
        configurable: false,
        get: () => state.parentWindow || globalThis,
      }});
      Object.defineProperty(proxy, "top", {{
        enumerable: true,
        configurable: false,
        get: () => state.topWindow || globalThis,
      }});
      Object.defineProperty(proxy, "frameElement", {{
        enumerable: true,
        configurable: false,
        get: () => !state.sameOrigin
          ? null
          : state.frameElement || (globalThis.__glassHostElements instanceof Map
          ? globalThis.__glassHostElements.get(currentBinding.nodeIndex) || null
          : null),
      }});
      Object.defineProperty(proxy, "document", {{
        enumerable: true,
        configurable: false,
        get() {{
          const current = frameBindingForId(currentFrameId) || currentBinding;
          if (!state.sameOrigin || !current.sameOrigin) throw crossOriginSecurityError("document");
          return makeFrameDocument(
            current,
            state.frameElement || null,
            state.parentWindow || globalThis,
            state.topWindow || globalThis,
          );
        }},
      }});
      Object.defineProperty(proxy, "frames", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "length", {{
        enumerable: true,
        configurable: false,
        get() {{
          const current = frameBindingForId(currentFrameId) || currentBinding;
          return state.sameOrigin && current.sameOrigin ? frameChildBindings(current).length : 0;
        }},
      }});
      for (let index = 0; index < {max_frame_window_indices}; index += 1) {{
        Object.defineProperty(proxy, String(index), {{
          enumerable: false,
          configurable: false,
          get() {{
            const current = frameBindingForId(currentFrameId) || currentBinding;
            if (!state.sameOrigin || !current.sameOrigin) return undefined;
            const child = frameChildBindings(current)[index];
            return child
              ? makeFrameWindow(child, null, proxy, state.topWindow || globalThis)
              : undefined;
          }},
        }});
      }}
      frameWindowCache.set(currentFrameId, proxy);
    }}
    return frameWindowCache.get(currentFrameId);
  }};
  const relationshipWindowCache = globalThis.__glassRelationshipWindowCache instanceof Map
    ? globalThis.__glassRelationshipWindowCache
    : new Map();
  globalThis.__glassRelationshipWindowCache = relationshipWindowCache;
  const snapshotElementFromEntry = (entry) => {{
    if (!entry || typeof entry !== "object") return null;
    const attributes = entry.attributes && typeof entry.attributes === "object" ? entry.attributes : {{}};
    const projected = {{
      nodeIndex: entry.nodeIndex,
      parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
      tagName: tagNameForEntry(entry),
      nodeType: 1,
      nodeName: tagNameForEntry(entry),
      localName: namespaceUriForEntry(entry) === HTML_NAMESPACE
        ? String(entry.tagName || "").toLowerCase()
        : String(entry.tagName || ""),
      namespaceURI: namespaceUriForEntry(entry),
      id: attributes.id || "",
      className: attributes.class || "",
      textContent: String(entry.text || ""),
      innerText: String(entry.text || ""),
      value: entry.value == null ? "" : entry.value,
      checked: Boolean(entry.checked),
      indeterminate: Boolean(entry.indeterminate),
      selected: Boolean(entry.selected),
      disabled: Boolean(entry.disabled),
      hidden: Boolean(entry.hidden),
      getAttribute(name) {{
        const htmlElement = namespaceUriForEntry(entry) === HTML_NAMESPACE;
        const key = htmlElement ? String(name).toLowerCase() : String(name);
        for (const attribute of Object.keys(attributes)) {{
          if (htmlElement ? attribute.toLowerCase() === key : attribute === key) return attributes[attribute];
        }}
        return null;
      }},
      hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
    }};
    Object.defineProperty(projected, "ownerDocument", {{
      enumerable: false,
      configurable: false,
      get: () => null,
    }});
    try {{ Object.setPrototypeOf(projected, elementPrototypeFor(projected.tagName)); }} catch (_error) {{}}
    return projected;
  }};
  const makeRelationshipWindow = (descriptor, parentWindow = null, topWindow = null) => {{
    if (!descriptor || typeof descriptor !== "object" || !descriptor.contextId) return null;
    const contextId = String(descriptor.contextId);
    const proxy = makeWindowProxy(
      "",
      "",
      contextId,
      String(descriptor.url || "about:blank"),
      Boolean(descriptor.sameOrigin),
    );
    const cacheKey = contextId + "\\u0000";
    const state = windowProxyStates.get(cacheKey);
    if (state) {{
      state.targetLocationHref = String(descriptor.url || "about:blank");
      state.closed = false;
      state.sameOrigin = Boolean(descriptor.sameOrigin);
      state.relationshipDescriptor = descriptor;
      state.relationshipParent = parentWindow || proxy;
      state.relationshipTop = topWindow || proxy;
    }}
    if (!relationshipWindowCache.has(contextId)) {{
      Object.defineProperty(proxy, "window", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "self", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "parent", {{
        enumerable: true,
        configurable: false,
        get: () => state.relationshipParent || proxy,
      }});
      Object.defineProperty(proxy, "top", {{
        enumerable: true,
        configurable: false,
        get: () => state.relationshipTop || proxy,
      }});
      Object.defineProperty(proxy, "frameElement", {{
        enumerable: true,
        configurable: false,
        value: null,
      }});
      Object.defineProperty(proxy, "document", {{
        enumerable: true,
        configurable: false,
        get() {{
          const current = state.relationshipDescriptor;
          if (!state.sameOrigin || !current || !current.sameOrigin) throw crossOriginSecurityError("document");
          return makeFrameDocument(
            current,
            null,
            state.relationshipParent || proxy,
            state.relationshipTop || proxy,
          );
        }},
      }});
      Object.defineProperty(proxy, "frames", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "length", {{
        enumerable: true,
        configurable: false,
        get() {{
          const current = state.relationshipDescriptor;
          return state.sameOrigin && current && current.sameOrigin
            ? frameChildBindings(current).length
            : 0;
        }},
      }});
      for (let index = 0; index < {max_frame_window_indices}; index += 1) {{
        Object.defineProperty(proxy, String(index), {{
          enumerable: false,
          configurable: false,
          get() {{
            const current = state.relationshipDescriptor;
            if (!state.sameOrigin || !current || !current.sameOrigin) return undefined;
            const child = frameChildBindings(current)[index];
            return child
              ? makeFrameWindow(child, null, proxy, state.relationshipTop || proxy)
              : undefined;
          }},
        }});
      }}
      relationshipWindowCache.set(contextId, proxy);
      frameWindowCache.set(contextId, proxy);
    }}
    return relationshipWindowCache.get(contextId);
  }};
  const frameBindings = Array.isArray(host.frames) ? host.frames : [];
  globalThis.__glassFrameBindings = frameBindings;
  globalThis.length = frameBindings.length;
  for (let index = 0; index < frameBindings.length; index += 1) {{
    try {{
      Object.defineProperty(globalThis, String(index), {{
        enumerable: false,
        configurable: true,
        get() {{
          return makeFrameWindow(
            frameBindings[index], null, globalThis, relationshipTop || globalThis
          );
        }},
      }});
    }} catch (_error) {{}}
  }};
  const frameContext = host.frame_context && typeof host.frame_context === "object"
    ? host.frame_context
    : null;
  globalThis.__glassFrameContext = frameContext;
  let relationshipParent = null;
  let relationshipTop = null;
  let selectedFrameElement = null;
  if (frameContext) {{
    frameWindowCache.set(String(frameContext.currentFrameId), globalThis);
    const topDescriptor = frameContext.top && typeof frameContext.top === "object"
      ? frameContext.top
      : null;
    relationshipTop = makeRelationshipWindow(topDescriptor);
    if (frameContext.parent && typeof frameContext.parent === "object") {{
      relationshipParent = topDescriptor
          && String(frameContext.parent.contextId || "") === String(topDescriptor.contextId || "")
        ? relationshipTop
        : makeRelationshipWindow(frameContext.parent, relationshipTop, relationshipTop);
    }}
    selectedFrameElement = frameContext.parent
      && frameContext.parent.sameOrigin === true
      ? snapshotElementFromEntry(frameContext.frameElement)
      : null;
  }}
