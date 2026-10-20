  const setLocalFocus = (target) => {{
    const current = elements.find((element) => element.focused && element !== target) || null;
    if (current) {{
      current.focused = false;
      dispatchTarget(current, createFocusEvent("blur", {{ relatedTarget: target }}));
      dispatchTarget(current, createFocusEvent("focusout", {{ bubbles: true, relatedTarget: target }}));
    }}
    if (target.focused) return;
    target.focused = true;
    pushCommand({{ kind: "focus", node_index: target.nodeIndex }});
    dispatchTarget(target, createFocusEvent("focus", {{ relatedTarget: current }}));
    dispatchTarget(target, createFocusEvent("focusin", {{ bubbles: true, relatedTarget: current }}));
  }};
  const asNativeCollection = (values, constructorName) => {{
    const constructor = globalThis[constructorName];
    if (typeof constructor === "function" && constructor.prototype) {{
      Object.setPrototypeOf(values, constructor.prototype);
    }}
    return values;
  }};
  const asNodeList = (values) => asNativeCollection(values, "NodeList");
  const asHtmlCollection = (values) => asNativeCollection(values, "HTMLCollection");
  const collectionIndex = (property) => {{
    if (typeof property !== "string" || !/^(0|[1-9][0-9]*)$/.test(property)) return null;
    const index = Number(property);
    return Number.isSafeInteger(index) ? index : null;
  }};
  const liveCollection = (owner, filter, constructorName, includeDescendants = false) => {{
    const target = [];
    const current = () => includeDescendants
      ? descendantsInTree(owner, filter)
      : Array.isArray(owner.__glassChildren)
        ? owner.__glassChildren.filter((child) => child && filter(child))
        : [];
    const proxy = new Proxy(target, {{
      get(_target, property, receiver) {{
        const values = current();
        const index = collectionIndex(property);
        if (index !== null) return values[index];
        if (property === "length") return values.length;
        if (property === "item") return (requested) => {{
          const numeric = Number(requested);
          return Number.isSafeInteger(numeric) && numeric >= 0 ? current()[numeric] || null : null;
        }};
        if (property === "namedItem") return (name) => {{
          const key = String(name);
          return current().find((item) => item.id === key || item.name === key) || null;
        }};
        if (property === Symbol.iterator) return () => current()[Symbol.iterator]();
        if (property === "values") return () => current()[Symbol.iterator]();
        if (property === "keys") return () => current().keys();
        if (property === "entries") return () => current().entries();
        if (property === "forEach") return (callback, thisArg) => current().forEach(callback, thisArg);
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(receiver) : value;
      }},
      has(_target, property) {{
        const index = collectionIndex(property);
        if (index !== null) return index < current().length;
        return property === "length" || property === "item" || property === "namedItem"
          || Reflect.has(target, property);
      }},
      ownKeys(_target) {{
        return current().map((_value, index) => String(index)).concat("length");
      }},
      getOwnPropertyDescriptor(_target, property) {{
        const index = collectionIndex(property);
        if (index !== null && index < current().length) {{
          return {{ enumerable: true, configurable: true, value: current()[index], writable: false }};
        }}
        return Reflect.getOwnPropertyDescriptor(target, property);
      }},
    }});
    return asNativeCollection(proxy, constructorName);
  }};
  const defineMissing = (target, name, descriptor) => {{
    if (!Object.prototype.hasOwnProperty.call(target, name)) Object.defineProperty(target, name, descriptor);
  }};
  const descendantsInTree = (owner, predicate) => {{
    const result = [];
    const visit = (node) => {{
      for (const child of Array.isArray(node && node.__glassChildren) ? node.__glassChildren : []) {{
        if (child && child.nodeType === 1 && predicate(child)) result.push(child);
        visit(child);
      }}
    }};
    visit(owner);
    return result;
  }};
  const nativeCustomElementRegistryForDocument = (targetDocument) => {{
    if (!targetDocument || targetDocument !== document
        || targetDocument.__glassIsTemplateContentsOwnerDocument === true) return null;
    return nativeCustomElementState.registry || null;
  }};
  const importNodeOptions = (targetDocument, options) => {{
    let deep = false;
    let requestedRegistry;
    if (typeof options === "boolean") {{
      deep = options;
    }} else if (options !== undefined) {{
      if (options !== null && typeof options !== "object" && typeof options !== "function") {{
        throw new TypeError("importNode options must be a boolean or dictionary");
      }}
      if (options !== null) {{
        requestedRegistry = options.customElementRegistry;
        deep = !Boolean(options.selfOnly);
      }}
    }}

    const targetRegistry = nativeCustomElementRegistryForDocument(targetDocument);
    if (requestedRegistry !== undefined) {{
      if (requestedRegistry === null
          || (typeof requestedRegistry !== "object" && typeof requestedRegistry !== "function")
          || !(requestedRegistry instanceof CustomElementRegistryNative)) {{
        throw new TypeError("customElementRegistry must be a CustomElementRegistry");
      }}
      if (requestedRegistry !== targetRegistry
          || requestedRegistry !== nativeCustomElementState.registry) {{
        throw new DOMExceptionNative(
          "The requested custom element registry is not available to the target Document",
          "NotSupportedError",
        );
      }}
    }}
    return {{ deep, registry: requestedRegistry === undefined ? targetRegistry : requestedRegistry }};
  }};
  const cloneNativeNodeIntoDocument = (
    source,
    targetDocument,
    deep,
    state = {{ nodeCount: 0 }},
    depth = 0,
    preserveTemplateContentsOwner = false,
    registry = null,
  ) => {{
    state.nodeCount += 1;
    if (state.nodeCount > {native_dom_max_nodes}) throw new RangeError("native node clone exceeds its node limit");
    const nodeType = Number(source && source.nodeType);
    if (depth + Number(nodeType === 1) > {native_dom_max_depth}) {{
      throw new RangeError("native node clone exceeds its depth limit");
    }}
    if (nodeType === 9) throw new DOMExceptionNative("A Document cannot be imported", "NotSupportedError");
    if (nodeType === 11 && typeof globalThis.ShadowRoot === "function"
        && source instanceof globalThis.ShadowRoot) {{
      throw new DOMExceptionNative("A ShadowRoot cannot be imported", "NotSupportedError");
    }}
    const cloneChild = (child, ownerDocument) => cloneNativeNodeIntoDocument(
      child,
      ownerDocument,
      true,
      state,
      depth + Number(nodeType === 1),
      preserveTemplateContentsOwner,
      registry,
    );
    if (nodeType === 3) return targetDocument.createTextNode(String(source.nodeValue || ""));
    if (nodeType === 8) return targetDocument.createComment(String(source.nodeValue || ""));
    if (nodeType === 2) {{
      if (typeof targetDocument.createAttributeNS !== "function") {{
        throw new DOMExceptionNative("The target Document cannot create attributes", "NotSupportedError");
      }}
      const clone = targetDocument.createAttributeNS(source.namespaceURI || null, source.name);
      clone.value = String(source.value || "");
      return clone;
    }}
    if (nodeType === 10) {{
      if (!targetDocument.implementation
          || typeof targetDocument.implementation.createDocumentType !== "function") {{
        throw new DOMExceptionNative("The target Document cannot create document types", "NotSupportedError");
      }}
      return targetDocument.implementation.createDocumentType(source.name, source.publicId, source.systemId);
    }}
    if (nodeType === 1) {{
      const qualifiedName = source.prefix ? source.prefix + ":" + source.localName : source.localName;
      const namespaceURI = source.namespaceURI || null;
      const clone = targetDocument === document
        ? makeDetachedElement(qualifiedName, namespaceURI)
        : targetDocument.createElementNS(namespaceURI, qualifiedName);
      const sourceIsValue = nativeCustomElementIsValue(source);
      if (sourceIsValue !== null
          && Object.prototype.hasOwnProperty.call(clone, "__glassCustomElementIs")) {{
        clone.__glassCustomElementIs = sourceIsValue;
      }}
      if (registry !== null && namespaceURI === HTML_NAMESPACE) {{
        Object.defineProperty(clone, "__glassCustomElementRegistry", {{
          configurable: true,
          enumerable: false,
          writable: true,
          value: registry,
        }});
      }}
      for (const name of source.getAttributeNames()) {{
        const value = source.getAttribute(name);
        if (value === null) continue;
        const namespaceURI = typeof source.__glassAttributeNamespace === "function"
          ? source.__glassAttributeNamespace(name)
          : null;
        if (typeof clone.__glassSetParsedAttribute === "function") {{
          clone.__glassSetParsedAttribute(name, value, namespaceURI);
        }} else if (namespaceURI === null) clone.setAttribute(name, value);
        else clone.setAttributeNS(namespaceURI, name, value);
      }}
      if (source.namespaceURI === HTML_NAMESPACE && source.localName === "template") {{
        const sourceContent = source.content;
        const targetContent = clone.content;
        const sourceContentOwner = sourceContent && sourceContent.ownerDocument;
        if (preserveTemplateContentsOwner && sourceContentOwner
            && targetContent.ownerDocument !== sourceContentOwner
            && !targetContent.__glassOwnerDocumentOverride) {{
          Object.defineProperty(targetContent, "__glassOwnerDocumentOverride", {{
            enumerable: false,
            configurable: false,
            value: sourceContentOwner,
          }});
        }}
        if (deep) {{
          const targetContentDocument = targetContent.ownerDocument || targetDocument;
          for (const child of sourceContent.__glassChildren || []) {{
            targetContent.appendChild(cloneChild(child, targetContentDocument));
          }}
        }}
      }}
      if (deep) {{
        for (const child of source.__glassChildren || []) {{
          clone.appendChild(cloneChild(child, targetDocument));
        }}
      }}
      return clone;
    }}
    if (nodeType === 11) {{
      const clone = targetDocument.createDocumentFragment();
      if (deep) {{
        for (const child of source.__glassChildren || []) {{
          clone.appendChild(cloneChild(child, targetDocument));
        }}
      }}
      return clone;
    }}
    throw new DOMExceptionNative("This native node type cannot be cloned", "NotSupportedError");
  }};
  const importNativeNode = (targetDocument, source, options = false) => {{
    if (!source || typeof source !== "object" || !(source instanceof NodeNative)) {{
      throw new TypeError("importNode requires a Node");
    }}
    if (Number(source.nodeType) === 9) {{
      throw new DOMExceptionNative("A Document cannot be imported", "NotSupportedError");
    }}
    if ((typeof globalThis.ShadowRoot === "function" && source instanceof globalThis.ShadowRoot)
        || source.__glassShadowRoot === true) {{
      throw new DOMExceptionNative("A ShadowRoot cannot be imported", "NotSupportedError");
    }}
    const importOptions = importNodeOptions(targetDocument, options);
    const clone = cloneNativeNodeIntoDocument(
      source,
      targetDocument,
      importOptions.deep,
      {{ nodeCount: 0 }},
      0,
      false,
      importOptions.registry,
    );
    if (importOptions.registry !== null
        && importOptions.registry === nativeCustomElementState.registry
        && targetDocument.__glassIsTemplateContentsOwnerDocument !== true) {{
      nativeCustomElementState.upgrading += 1;
      try {{ nativeCustomElementUpgradeTree(clone, false); }}
      finally {{
        nativeCustomElementState.upgrading = Math.max(0, nativeCustomElementState.upgrading - 1);
        nativeCustomElementDrainReactions();
      }}
    }}
    return clone;
  }};
  const templateContentsOwnerDocumentForDocument = (ownerDocument) => {{
    if (ownerDocument && ownerDocument.__glassIsTemplateContentsOwnerDocument === true) return ownerDocument;
    if (ownerDocument && typeof ownerDocument.__glassGetTemplateContentsOwnerDocument === "function") {{
      return ownerDocument.__glassGetTemplateContentsOwnerDocument();
    }}
    throw new DOMExceptionNative(
      "The target Document has no native template-contents owner Document",
      "NotSupportedError",
    );
  }};
  const setAdoptedOwnerDocument = (node, ownerDocument) => {{
    if (node.ownerDocument === ownerDocument) return;
    if (!Object.prototype.hasOwnProperty.call(node, "__glassAdoptedOwnerDocument")) {{
      Object.defineProperty(node, "__glassAdoptedOwnerDocument", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: ownerDocument,
      }});
    }} else {{
      node.__glassAdoptedOwnerDocument = ownerDocument;
    }}
  }};
  const adoptNativeNode = (targetDocument, source) => {{
    if (!source || typeof source !== "object" || !(source instanceof NodeNative)) {{
      throw new TypeError("adoptNode requires a Node");
    }}
    const nodeType = Number(source.nodeType);
    if (nodeType === 9) throw new DOMExceptionNative("A Document cannot be adopted", "NotSupportedError");
    if ((typeof globalThis.ShadowRoot === "function" && source instanceof globalThis.ShadowRoot)
        || source.__glassShadowRoot === true) {{
      throw new DOMExceptionNative("A ShadowRoot cannot be adopted", "HierarchyRequestError");
    }}
    const sourceDocument = source.ownerDocument;
    const sourceContextId = sourceDocument && String(sourceDocument.__glassNativeContextId || "");
    const targetContextId = targetDocument && String(targetDocument.__glassNativeContextId || "");
    if (!sourceDocument || !sourceContextId || !targetContextId) {{
      throw new DOMExceptionNative(
        "The source or target Document is not a live native owner",
        "NotSupportedError",
      );
    }}
    const state = {{ nodeCount: 0, nodes: [], seen: new Set() }};
    const pending = [{{
      node: source,
      ownerDocument: targetDocument,
      depth: 0,
      entered: false,
      phase: 0,
      attributeNames: null,
      attributeIndex: 0,
      children: null,
      childIndex: 0,
      nodeType: 0,
    }}];
    while (pending.length > 0) {{
      const frame = pending[pending.length - 1];
      const {{ node, ownerDocument, depth }} = frame;
      if (!frame.entered) {{
        if (!node || typeof node !== "object" || state.seen.has(node)) {{
          pending.pop();
          continue;
        }}
        state.seen.add(node);
        if (!(node instanceof NodeNative)) {{
          throw new DOMExceptionNative("The adoption subtree contains a non-native node", "NotSupportedError");
        }}
        const currentType = Number(node.nodeType);
        if (![1, 2, 3, 8, 10, 11].includes(currentType)) {{
          throw new DOMExceptionNative("This native node type cannot be adopted", "NotSupportedError");
        }}
        state.nodeCount += 1;
        if (state.nodeCount > {native_dom_max_nodes}) {{
          throw new RangeError("native node adoption exceeds its node limit");
        }}
        if (depth + Number(currentType === 1) > {native_dom_max_depth}) {{
          throw new RangeError("native node adoption exceeds its depth limit");
        }}
        state.nodes.push([node, ownerDocument]);
        frame.entered = true;
        frame.nodeType = currentType;
        if (currentType === 1
            && typeof node.getAttributeNames === "function"
            && typeof node.getAttributeNode === "function") {{
          frame.attributeNames = Array.from(node.getAttributeNames());
        }} else {{
          frame.attributeNames = [];
        }}
        continue;
      }}

      if (frame.phase === 0) {{
        if (frame.attributeIndex < frame.attributeNames.length) {{
          const name = frame.attributeNames[frame.attributeIndex++];
          const attribute = node.getAttributeNode(name);
          if (attribute) {{
            pending.push({{
              node: attribute,
              ownerDocument,
              depth,
              entered: false,
              phase: 0,
              attributeNames: null,
              attributeIndex: 0,
              children: null,
              childIndex: 0,
              nodeType: 0,
            }});
          }}
          continue;
        }}
        frame.phase = 1;
      }}

      if (frame.phase === 1) {{
        frame.phase = 2;
        if (frame.nodeType === 1
            && node.namespaceURI === HTML_NAMESPACE
            && node.localName === "template") {{
          const contents = node.content;
          if (!contents || Number(contents.nodeType) !== 11) {{
            throw new DOMExceptionNative("The template content fragment is unavailable", "NotSupportedError");
          }}
          pending.push({{
            node: contents,
            ownerDocument: templateContentsOwnerDocumentForDocument(ownerDocument),
            depth: depth + 1,
            entered: false,
            phase: 0,
            attributeNames: null,
            attributeIndex: 0,
            children: null,
            childIndex: 0,
            nodeType: 0,
          }});
          continue;
        }}
      }}

      if (frame.phase === 2) {{
        if (frame.children === null) {{
          frame.children = Array.isArray(node.__glassChildren) ? node.__glassChildren : [];
        }}
        if (frame.childIndex < frame.children.length) {{
          const child = frame.children[frame.childIndex++];
          pending.push({{
            node: child,
            ownerDocument,
            depth: depth + Number(frame.nodeType === 1),
            entered: false,
            phase: 0,
            attributeNames: null,
            attributeIndex: 0,
            children: null,
            childIndex: 0,
            nodeType: 0,
          }});
          continue;
        }}
      }}

      pending.pop();
    }}

    if (sourceContextId !== targetContextId) {{
      const sourceOrigin = String(sourceDocument.__glassNativeOrigin || "null");
      const targetOrigin = String(targetDocument.__glassNativeOrigin || "null");
      const sourceGeneration = Number(sourceDocument.__glassNativeGeneration);
      const targetGeneration = Number(targetDocument.__glassNativeGeneration);
      if (sourceOrigin === "null" || targetOrigin === "null" || sourceOrigin !== targetOrigin) {{
        throw new DOMExceptionNative("Native nodes cannot be adopted across origins", "SecurityError");
      }}
      if (!Number.isSafeInteger(sourceGeneration) || sourceGeneration <= 0
          || !Number.isSafeInteger(targetGeneration) || targetGeneration <= 0) {{
        throw new DOMExceptionNative("A source or target Document is stale", "InvalidStateError");
      }}

      const targetAliases = new Set(
        (Array.isArray(targetDocument.__glassNativeScriptNodes)
          ? targetDocument.__glassNativeScriptNodes
          : [])
          .map((identity) => Number(identity && identity.temporaryIndex))
          .filter((index) => Number.isSafeInteger(index) && index >= 0),
      );
      const usedAliases = new Set();
      const identityRecords = [];
      const transfers = [];
      const currentContextId = String(host.frame_id || host.context_id || "");
      const allocateDestinationAlias = () => {{
        for (let attempt = 0; attempt <= {native_dom_max_nodes}; attempt += 1) {{
          const alias = allocateTemporaryNodeIndex();
          if (!targetAliases.has(alias) && !usedAliases.has(alias)) {{
            usedAliases.add(alias);
            return alias;
          }}
        }}
        throw new RangeError("native destination identity limit exceeded");
      }};
      const appendNativeSubtree = (root) => {{
        const identities = [];
        const seen = new Set();
        const pending = [{{ node: root, depth: 0 }}];
        while (pending.length > 0) {{
          const item = pending.pop();
          const node = item && item.node;
          if (!node || typeof node !== "object" || seen.has(node)) continue;
          seen.add(node);
          const type = Number(node.nodeType);
          if (type === 2) continue;
          if (type === 11 && node.__glassFragment === true
              && (typeof node.nodeIndex !== "number"
                || !Number.isSafeInteger(node.nodeIndex)
                || node.nodeIndex >= 4294963200)) {{
            const children = Array.isArray(node.__glassChildren) ? node.__glassChildren : [];
            for (let index = children.length - 1; index >= 0; index -= 1) {{
              pending.push({{ node: children[index], depth: item.depth }});
            }}
            continue;
          }}
          const sourceIndex = Number(node.nodeIndex);
          if (!Number.isSafeInteger(sourceIndex) || sourceIndex < 0) {{
            throw new DOMExceptionNative(
              "The adoption subtree contains a node without a native identity",
              "NotSupportedError",
            );
          }}
          if (identities.length >= {native_dom_max_nodes}) {{
            throw new RangeError("native node adoption exceeds its node limit");
          }}
          const destinationTemporaryIndex = allocateDestinationAlias();
          identities.push({{ sourceIndex, destinationTemporaryIndex }});
          identityRecords.push({{ node, sourceIndex, destinationTemporaryIndex }});
          const nextDepth = item.depth + Number(type === 1);
          if (nextDepth > {native_dom_max_depth}) {{
            throw new RangeError("native node adoption exceeds its depth limit");
          }}
          const children = Array.isArray(node.__glassChildren) ? node.__glassChildren : [];
          for (let index = children.length - 1; index >= 0; index -= 1) {{
            pending.push({{ node: children[index], depth: nextDepth }});
          }}
          if (type === 1 && node.namespaceURI === HTML_NAMESPACE && node.localName === "template") {{
            const content = node.content;
            if (content && Number(content.nodeType) === 11) {{
              pending.push({{ node: content, depth: nextDepth }});
            }}
          }}
        }}
        if (identities.length > 0) {{
          transfers.push({{
            sourceIndex: identities[0].sourceIndex,
            identities,
          }});
        }}
      }};
      const roots = Number(source.nodeType) === 11 && source.__glassFragment === true
          && (typeof source.nodeIndex !== "number"
            || !Number.isSafeInteger(source.nodeIndex)
            || source.nodeIndex >= 4294963200)
        ? (Array.isArray(source.__glassChildren) ? source.__glassChildren.slice() : [])
        : Number(source.nodeType) === 2
          ? []
          : [source];
      for (const root of roots) appendNativeSubtree(root);
      if (identityRecords.length > {native_dom_max_nodes}) {{
        throw new RangeError("native node adoption exceeds its total identity limit");
      }}

      const attributeOwner = Number(source.nodeType) === 2 ? source.ownerElement : null;
      if (attributeOwner) {{
        if (source.namespaceURI == null) attributeOwner.removeAttribute(source.name);
        else attributeOwner.removeAttributeNS(source.namespaceURI, source.localName);
      }}
      if (transfers.length > 0) {{
        const relayFrameId = targetContextId === currentContextId
          ? sourceContextId
          : targetContextId;
        const queueFrameCommand = globalThis.__glassQueueNativeFrameCommand;
        if (typeof queueFrameCommand !== "function") {{
          throw new DOMExceptionNative("Native frame command routing is unavailable", "InvalidStateError");
        }}
        queueFrameCommand(relayFrameId, {{
          kind: "adoptNode",
          source_frame_id: sourceContextId,
          destination_frame_id: targetContextId,
          source_generation: sourceGeneration,
          destination_generation: targetGeneration,
          transfers,
        }});
      }}

      const scriptObjects = globalThis.__glassScriptNodeObjects instanceof Map
        ? globalThis.__glassScriptNodeObjects
        : null;
      const frameObjectsById = globalThis.__glassFrameScriptNodeObjectsByFrame instanceof Map
        ? globalThis.__glassFrameScriptNodeObjectsByFrame
        : new Map();
      globalThis.__glassFrameScriptNodeObjectsByFrame = frameObjectsById;
      let destinationObjects;
      if (targetContextId === currentContextId) {{
        destinationObjects = scriptObjects || new Map();
        globalThis.__glassScriptNodeObjects = destinationObjects;
      }} else {{
        destinationObjects = frameObjectsById.get(targetContextId);
        if (!(destinationObjects instanceof Map)) {{
          destinationObjects = new Map();
          frameObjectsById.set(targetContextId, destinationObjects);
        }}
      }}
      const caches = [scriptObjects, ...frameObjectsById.values()]
        .filter((cache) => cache instanceof Map);
      const ownerRoutes = globalThis.__glassTransferredNodeOwners instanceof Map
        ? globalThis.__glassTransferredNodeOwners
        : new Map();
      const pendingOwners = globalThis.__glassPendingTransferredNodeOwners instanceof Map
        ? globalThis.__glassPendingTransferredNodeOwners
        : new Map();
      globalThis.__glassTransferredNodeOwners = ownerRoutes;
      globalThis.__glassPendingTransferredNodeOwners = pendingOwners;
      const relayBinding = targetContextId === currentContextId
        ? sourceDocument.__glassNativeFrameBinding
        : targetDocument.__glassNativeFrameBinding;
      for (const record of identityRecords) {{
        const {{ node, sourceIndex, destinationTemporaryIndex }} = record;
        for (const cache of caches) {{
          for (const [alias, cached] of cache) {{
            if (cached === node) cache.delete(alias);
          }}
        }}
        ownerRoutes.delete(sourceIndex);
        pendingOwners.delete(sourceIndex);
        if (typeof node.__glassSetNativeIndex === "function") {{
          node.__glassSetNativeIndex(destinationTemporaryIndex);
        }} else {{
          node.nodeIndex = destinationTemporaryIndex;
        }}
        destinationObjects.set(destinationTemporaryIndex, node);
        const owner = {{
          frameId: targetContextId,
          relayFrameId: targetContextId === currentContextId ? sourceContextId : targetContextId,
          binding: targetContextId === currentContextId ? null : targetDocument.__glassNativeFrameBinding,
          relayBinding,
        }};
        ownerRoutes.set(destinationTemporaryIndex, owner);
        pendingOwners.set(destinationTemporaryIndex, true);
      }}
      for (const [node, ownerDocument] of state.nodes) {{
        setAdoptedOwnerDocument(node, ownerDocument);
        if (node !== source && node.parentNode && node.parentNode.nodeIndex != null
            && typeof node.parentNode.nodeIndex === "number") {{
          node.parentIndex = node.parentNode.nodeIndex;
        }}
      }}
      if (Number(source.nodeType) !== 2) {{
        const oldParent = source.parentNode;
        if (oldParent) {{
          suppressHostCommands += 1;
          try {{
            if (typeof source.remove === "function") source.remove();
            else if (typeof oldParent.removeChild === "function") oldParent.removeChild(source);
            else throw new DOMExceptionNative("The source node cannot be detached", "NotSupportedError");
          }} finally {{
            suppressHostCommands -= 1;
          }}
        }}
      }}
      if (sourceDocument !== targetDocument
          && typeof globalThis.__glassQueueNativeCustomElementReaction === "function") {{
        for (const [node] of state.nodes) {{
          if (Number(node && node.nodeType) === 1) {{
            globalThis.__glassQueueNativeCustomElementReaction(
              node,
              "adoptedCallback",
              [sourceDocument, targetDocument],
            );
          }}
        }}
      }}
      return source;
    }}

    const oldParent = source.parentNode;
    if (oldParent) {{
      if (typeof source.remove === "function") source.remove();
      else if (typeof oldParent.removeChild === "function") oldParent.removeChild(source);
      else throw new DOMExceptionNative("The source node cannot be detached", "NotSupportedError");
    }}
    for (const [node, ownerDocument] of state.nodes) setAdoptedOwnerDocument(node, ownerDocument);
    if (sourceDocument !== targetDocument
        && typeof globalThis.__glassQueueNativeCustomElementReaction === "function") {{
      for (const [node] of state.nodes) {{
        if (Number(node && node.nodeType) === 1) {{
          globalThis.__glassQueueNativeCustomElementReaction(
            node,
            "adoptedCallback",
            [sourceDocument, targetDocument],
          );
        }}
      }}
    }}
    return source;
  }};
  const defineTreeAccessors = (node) => {{
    if (!node || typeof node !== "object") return node;
    const children = () => Array.isArray(node.__glassChildren) ? node.__glassChildren : [];
    const parent = () => node.__glassParent
      || (node.parentNode && node.parentNode !== node && Array.isArray(node.parentNode.__glassChildren)
        ? node.parentNode
        : null);
    defineMissing(node, "childNodes", {{
      enumerable: false,
      configurable: false,
      get() {{ return liveCollection(node, () => true, "NodeList"); }},
    }});
    defineMissing(node, "children", {{
      enumerable: false,
      configurable: false,
      get() {{ return liveCollection(node, (child) => child.nodeType === 1, "HTMLCollection"); }},
    }});
    defineMissing(node, "firstChild", {{
      enumerable: false,
      configurable: false,
      get() {{ return children()[0] || null; }},
    }});
    defineMissing(node, "lastChild", {{
      enumerable: false,
      configurable: false,
      get() {{ const values = children(); return values[values.length - 1] || null; }},
    }});
    defineMissing(node, "firstElementChild", {{
      enumerable: false,
      configurable: false,
      get() {{ return children().find((child) => child.nodeType === 1) || null; }},
    }});
    defineMissing(node, "lastElementChild", {{
      enumerable: false,
      configurable: false,
      get() {{ const values = children().filter((child) => child.nodeType === 1); return values[values.length - 1] || null; }},
    }});
    defineMissing(node, "nextSibling", {{
      enumerable: false,
      configurable: false,
      get() {{
        const owner = parent();
        if (!owner) return null;
        const values = Array.isArray(owner.__glassChildren) ? owner.__glassChildren : [];
        const index = values.indexOf(node);
        return index >= 0 ? values[index + 1] || null : null;
      }},
    }});
    defineMissing(node, "previousSibling", {{
      enumerable: false,
      configurable: false,
      get() {{
        const owner = parent();
        if (!owner) return null;
        const values = Array.isArray(owner.__glassChildren) ? owner.__glassChildren : [];
        const index = values.indexOf(node);
        return index > 0 ? values[index - 1] : null;
      }},
    }});
    defineMissing(node, "nextElementSibling", {{
      enumerable: false,
      configurable: false,
      get() {{
        let sibling = node.nextSibling;
        while (sibling && sibling.nodeType !== 1) sibling = sibling.nextSibling;
        return sibling || null;
      }},
    }});
    defineMissing(node, "previousElementSibling", {{
      enumerable: false,
      configurable: false,
      get() {{
        let sibling = node.previousSibling;
        while (sibling && sibling.nodeType !== 1) sibling = sibling.previousSibling;
        return sibling || null;
      }},
    }});
    defineMissing(node, "hasChildNodes", {{
      enumerable: false,
      configurable: false,
      value() {{ return children().length > 0; }},
    }});
    defineMissing(node, "contains", {{
      enumerable: false,
      configurable: false,
      value(candidate) {{
        if (candidate === node) return true;
        let current = candidate && candidate.__glassParent;
        while (current) {{
          if (current === node) return true;
          if (current.__glassParent) {{
            current = current.__glassParent;
            continue;
          }}
          return current.ownerDocument === node;
        }}
        return Boolean(candidate && candidate.ownerDocument === node
        && node.__glassChildren && node.__glassChildren.includes(candidate));
      }},
    }});
    if ([1, 2, 3, 8, 10, 11].includes(Number(node.nodeType))) {{
      if (node.nodeType === 1 || node.nodeType === 11) {{
        defineMissing(node, "querySelector", {{
          enumerable: false,
          configurable: false,
          value(selector) {{
            return descendantsInTree(node, (candidate) => matchesSelector(candidate, selector, node))[0] || null;
          }},
        }});
        defineMissing(node, "querySelectorAll", {{
          enumerable: false,
          configurable: false,
          value(selector) {{
            return asNodeList(descendantsInTree(node, (candidate) => matchesSelector(candidate, selector, node)));
          }},
        }});
        defineMissing(node, "getElementsByTagName", {{
          enumerable: false,
          configurable: false,
          value(name) {{
            const value = String(name).toLowerCase();
            return asHtmlCollection(descendantsInTree(node, (candidate) =>
              value === "*" || candidate.tagName.toLowerCase() === value));
          }},
        }});
        defineMissing(node, "getElementsByClassName", {{
          enumerable: false,
          configurable: false,
          value(name) {{
            const value = String(name).trim();
            if (!value) return asHtmlCollection([]);
            const tokens = value.split(/\s+/);
            return asHtmlCollection(descendantsInTree(node, (candidate) =>
              tokens.every((token) => String(candidate.className).split(/\s+/).includes(token))));
          }},
        }});
      }}
      const insertionText = (value) => node.ownerDocument
        && typeof node.ownerDocument.createTextNode === "function"
        ? node.ownerDocument.createTextNode(String(value))
        : makeDetachedText(String(value));
      const insertionNode = (item) => item && [1, 3, 8, 10, 11].includes(Number(item.nodeType))
        ? item
        : insertionText(item);
      if (node.nodeType === 1 || node.nodeType === 11) {{
        defineMissing(node, "append", {{
          enumerable: false,
          configurable: false,
          value(...items) {{
            for (const item of items) node.appendChild(insertionNode(item));
          }},
        }});
        defineMissing(node, "prepend", {{
          enumerable: false,
          configurable: false,
          value(...items) {{
            const before = node.__glassChildren[0] || null;
            for (const item of items) node.insertBefore(insertionNode(item), before);
          }},
        }});
      }}
      defineMissing(node, "before", {{
        enumerable: false,
        configurable: false,
        value(...items) {{
          const owner = node.__glassParent;
          if (!owner || typeof owner.insertBefore !== "function") return;
          for (const item of items) owner.insertBefore(insertionNode(item), node);
        }},
      }});
      defineMissing(node, "after", {{
        enumerable: false,
        configurable: false,
        value(...items) {{
          const owner = node.__glassParent;
          if (!owner || typeof owner.insertBefore !== "function") return;
          const before = node.nextSibling;
          for (const item of items) owner.insertBefore(insertionNode(item), before);
        }},
      }});
      defineMissing(node, "replaceWith", {{
        enumerable: false,
        configurable: false,
        value(...items) {{
          const owner = node.__glassParent;
          if (!owner || typeof owner.insertBefore !== "function") return;
          for (const item of items) owner.insertBefore(insertionNode(item), node);
          if (typeof node.remove === "function") node.remove();
        }},
      }});
      if (node.nodeType === 1 || node.nodeType === 11) {{
        defineMissing(node, "replaceChildren", {{
          enumerable: false,
          configurable: false,
          value(...items) {{
            for (const child of node.__glassChildren.slice()) child.remove();
            node.append(...items);
          }},
        }});
      }}
    }}
    if (node.nodeType === 1 || node.nodeType === 11) {{
      defineMissing(node, "replaceChild", {{
        enumerable: false,
        configurable: false,
        value(next, oldChild) {{
          if (!oldChild || oldChild.__glassParent !== node) throw new TypeError("old child is not contained by this node");
          if (!next || ![1, 3, 8, 11].includes(Number(next.nodeType))) throw new TypeError("replacement must be a native node");
          if (next === oldChild) return oldChild;
          node.insertBefore(next, oldChild);
          oldChild.remove();
          return oldChild;
        }},
      }});
    }}
    defineMissing(node, "isConnected", {{
      enumerable: false,
      configurable: false,
      get() {{
        const owner = node.ownerDocument;
        if (!owner) return false;
        if (owner.__glassChildren && owner.__glassChildren.includes(node)) return true;
        let current = node.__glassParent || null;
        while (current) {{
          if (owner.__glassChildren && owner.__glassChildren.includes(current)) return true;
          current = current.__glassParent || null;
        }}
        return false;
      }},
    }});
    defineMissing(node, "getRootNode", {{
      enumerable: false,
      configurable: false,
      value(_options) {{
        let current = node;
        for (let depth = 0; depth <= {max_commands}; depth += 1) {{
          if (!current.__glassParent) break;
          current = current.__glassParent;
        }}
        const owner = node.ownerDocument || null;
        return owner && Array.isArray(owner.__glassChildren)
          && owner.__glassChildren.includes(current)
          ? owner
          : current;
      }},
    }});
    defineMissing(node, "cloneNode", {{
      enumerable: false,
      configurable: false,
      value(deep = false) {{
        const owner = node.ownerDocument || null;
        if (!owner) throw new DOMExceptionNative("The node has no owner document", "InvalidStateError");
        return cloneNativeNodeIntoDocument(node, owner, Boolean(deep), {{ nodeCount: 0 }}, 0, true);
      }},
    }});
    defineMissing(node, "isSameNode", {{
      enumerable: false,
      configurable: false,
      value(other) {{ return node === other; }},
    }});
    const equalNodes = (left, right, depth = 0) => {{
      if (left === right) return true;
      if (!right || Number(left.nodeType) !== Number(right.nodeType) || depth > {max_commands}) return false;
      if ([3, 8].includes(Number(left.nodeType))) return String(left.nodeValue || "") === String(right.nodeValue || "");
      if (Number(left.nodeType) === 2) return String(left.name || "") === String(right.name || "")
        && String(left.value || "") === String(right.value || "")
        && String(left.namespaceURI || "") === String(right.namespaceURI || "")
        && String(left.localName || "") === String(right.localName || "");
      if (Number(left.nodeType) === 1) {{
        if (String(left.localName) !== String(right.localName)
            || String(left.namespaceURI || "") !== String(right.namespaceURI || "")) return false;
        const leftNames = left.getAttributeNames();
        const rightNames = right.getAttributeNames();
        if (leftNames.length !== rightNames.length) return false;
        for (const name of leftNames) {{
          if (left.getAttribute(name) !== right.getAttribute(name)) return false;
          const leftNamespace = typeof left.__glassAttributeNamespace === "function"
            ? left.__glassAttributeNamespace(name)
            : null;
          const rightNamespace = typeof right.__glassAttributeNamespace === "function"
            ? right.__glassAttributeNamespace(name)
            : null;
          if (leftNamespace !== rightNamespace) return false;
        }}
      }} else if (Number(left.nodeType) === 10) {{
        return String(left.name || left.nodeName || "") === String(right.name || right.nodeName || "")
          && String(left.publicId || "") === String(right.publicId || "")
          && String(left.systemId || "") === String(right.systemId || "");
      }} else if (![9, 11].includes(Number(left.nodeType))) {{
        return false;
      }}
      const leftChildren = Array.isArray(left.__glassChildren) ? left.__glassChildren : [];
      const rightChildren = Array.isArray(right.__glassChildren) ? right.__glassChildren : [];
      return leftChildren.length === rightChildren.length
        && leftChildren.every((child, index) => equalNodes(child, rightChildren[index], depth + 1));
    }};
    defineMissing(node, "isEqualNode", {{
      enumerable: false,
      configurable: false,
      value(other) {{ return equalNodes(node, other); }},
    }});
    defineMissing(node, "compareDocumentPosition", {{
      enumerable: false,
      configurable: false,
      value(other) {{
        if (!other || typeof other !== "object") return 1 | 32;
        if (node === other) return 0;
        const rootOf = (candidate) => candidate && typeof candidate.getRootNode === "function"
          ? candidate.getRootNode()
          : candidate;
        if (rootOf(node) !== rootOf(other)) return 1 | 32;
        const containsNode = (ancestor, candidate) => {{
          let current = candidate;
          for (let depth = 0; current && depth <= {max_commands}; depth += 1) {{
            current = current.parentNode || null;
            if (current === ancestor) return true;
          }}
          return false;
        }};
        if (containsNode(node, other)) return 4 | 16;
        if (containsNode(other, node)) return 2 | 8;
        const ordered = [];
        const visit = (candidate) => {{
          if (!candidate || ordered.includes(candidate)) return;
          ordered.push(candidate);
          for (const child of Array.isArray(candidate.__glassChildren) ? candidate.__glassChildren : []) visit(child);
        }};
        visit(rootOf(node));
        const left = ordered.indexOf(node);
        const right = ordered.indexOf(other);
        if (left < 0 || right < 0) return 1 | 32;
        return right > left ? 4 : 2;
      }},
    }});
    defineMissing(node, "normalize", {{
      enumerable: false,
      configurable: false,
      value() {{
        if (![1, 9, 11].includes(Number(node.nodeType))) return;
        const normalizeChildren = (owner) => {{
          let previousText = null;
          for (const child of (Array.isArray(owner.__glassChildren) ? owner.__glassChildren : []).slice()) {{
            if (Number(child.nodeType) === 3) {{
              const value = String(child.nodeValue || "");
              if (!value) {{
                child.remove();
              }} else if (previousText) {{
                previousText.data = String(previousText.data || "") + value;
                child.remove();
              }} else {{
                previousText = child;
              }}
            }} else {{
              if (typeof child.normalize === "function") child.normalize();
              previousText = null;
            }}
          }}
        }};
        normalizeChildren(node);
      }},
    }});
    return node;
  }};
  defineTreeAccessorsForNativeFragments = defineTreeAccessors;
  for (const element of elements) defineTreeAccessors(element);
  for (const text of textNodes) defineTreeAccessors(text);
  for (const fragment of fragmentNodes) defineTreeAccessors(fragment);
  for (const comment of commentNodes) {{
    defineTreeAccessors(comment);
    try {{ Object.setPrototypeOf(comment, CommentNative.prototype); }} catch (_error) {{}}
  }}
  for (const documentType of documentTypeNodes) {{
    defineTreeAccessors(documentType);
    try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
  }}
