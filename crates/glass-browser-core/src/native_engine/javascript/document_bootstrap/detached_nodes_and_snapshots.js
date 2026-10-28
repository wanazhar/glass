  const makeDetachedElement = (tagName, namespace = HTML_NAMESPACE) => {{
    const namespaceURI = normalizeElementNamespace(namespace);
    const requestedName = String(tagName);
    const normalized = namespaceURI === HTML_NAMESPACE
      ? requestedName.toLowerCase()
      : requestedName;
    if (!/^[A-Za-z][A-Za-z0-9:_-]*$/.test(normalized)) throw new TypeError("invalid element name");
    const nodeIndex = allocateTemporaryNodeIndex();
    const entry = {{
      nodeIndex,
      parentIndex: null,
      formOwnerIndex: null,
      tagName: normalized,
      namespaceUri: namespaceURI,
      attributes: {{}},
      attributeNamespaces: {{}},
      text: "",
      innerHtml: "",
      value: null,
      files: [],
      checked: false,
      indeterminate: false,
      selected: false,
      disabled: false,
      hidden: false,
      focused: false,
      focusAnchorNodeIndex: null,
      hovered: false,
      userInteracted: false,
      validity: {{ valid: true }},
      validationMessage: "",
      customValidity: "",
      willValidate: false,
      selectionStart: null,
      selectionEnd: null,
      selectionDirection: "none",
      imageComplete: true,
      imageNaturalWidth: 0,
      imageNaturalHeight: 0,
      imageCurrentSrc: "",
      mediaReadyState: 0,
      mediaNetworkState: 0,
      mediaCurrentSrc: "",
      mediaDurationMillis: null,
      mediaError: null,
    }};
    const element = makeElement(entry);
    element.__glassCreated = true;
    try {{
      Object.setPrototypeOf(
        element,
        namespaceURI === HTML_NAMESPACE
          ? elementPrototypeFor(element.tagName)
          : ElementNative.prototype,
      );
    }} catch (_error) {{}}
    Object.defineProperty(element, "__glassDynamicScriptStarted", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: false,
    }});
    defineTreeAccessors(element);
    installClassList(element);
    installElementStyleAndDataset(element);
    mutationCreatedNodes.set(nodeIndex, element);
    scriptNodeObjects.set(nodeIndex, element);
    pushCommand({{
      kind: "createElement",
      node_index: nodeIndex,
      tag_name: normalized,
      namespace_uri: namespaceURI === null ? "" : namespaceURI,
    }});
    return element;
  }};
  const nodeIsConnected = (node) => {{
    let current = node;
    for (let depth = 0; current && depth <= {max_commands}; depth += 1) {{
      const activeDocument = globalThis.document;
      if (current === activeDocument
          || current === (activeDocument && activeDocument.documentElement)
          || rootChildren.includes(current)) return true;
      current = current.__glassParent || null;
    }}
    return false;
  }};
  const classicJavascriptMimeTypes = new Set([{javascript_mime_type_essences}]);
  const asciiLowercase = (value) => String(value).replace(/[A-Z]/g, (character) =>
    String.fromCharCode(character.charCodeAt(0) + 32));
  const dynamicCspMetaProcessedThisTurn = new WeakSet();
  let dynamicCspMetaPolicyFailure = false;
  applyDynamicReferrerMetaPolicyToNode = (node) => {{
    if (!node || Number(node.nodeType) !== 1
        || asciiLowercase(String(node.localName || "")) !== "meta"
        || asciiLowercase(String(node.getAttribute("name") || "")) !== "referrer") return;
    if (!liveDocumentElements().includes(node)) return;
    const content = node.getAttribute("content");
    if (content === null || String(content).length === 0) return;
    let value = asciiLowercase(String(content));
    if (value === "never") value = "no-referrer";
    else if (value === "default") value = "strict-origin-when-cross-origin";
    else if (value === "always") value = "unsafe-url";
    else if (value === "origin-when-crossorigin") value = "origin-when-cross-origin";
    if ([
      "no-referrer",
      "no-referrer-when-downgrade",
      "same-origin",
      "origin",
      "strict-origin",
      "origin-when-cross-origin",
      "strict-origin-when-cross-origin",
      "unsafe-url",
    ].includes(value)) {{
      documentReferrerPolicy.value = value;
    }}
  }};
  captureDynamicReferrerMetaPolicy = (root) => {{
    const pending = [root];
    let remaining = {native_dom_max_nodes};
    while (pending.length > 0 && remaining > 0) {{
      const node = pending.pop();
      remaining -= 1;
      const children = Array.isArray(node && node.__glassChildren)
        ? node.__glassChildren
        : [];
      for (let index = children.length - 1; index >= 0; index -= 1) {{
        pending.push(children[index]);
      }}
      applyDynamicReferrerMetaPolicyToNode(node);
    }}
  }};
  captureDynamicCspMetaPolicy = (node) => {{
    if (!node || Number(node.nodeType) !== 1
        || String(node.localName || "").toLowerCase() !== "meta"
        // Parser-discovered CSP metas were captured with the document policy
        // before scripts ran. Attribute edits must not reapply their policy.
        || node.__glassCreated !== true
        || dynamicCspMetaProcessedThisTurn.has(node)) return;
    const head = globalThis.document && globalThis.document.head;
    let current = node.__glassParent || null;
    let underHead = false;
    for (let depth = 0; current && depth <= {max_commands}; depth += 1) {{
      if (current === head) {{ underHead = true; break; }}
      current = current.__glassParent || null;
    }}
    if (!underHead
        || asciiLowercase(String(node.getAttribute("http-equiv") || "").trim())
          !== "content-security-policy") return;
    const source = String(node.getAttribute("content") || "");
    if (source.length === 0) return;
    dynamicCspMetaProcessedThisTurn.add(node);
    try {{
      const appendPolicy = globalThis.__glassAppendMetaCspPolicy;
      if (typeof appendPolicy !== "function"
          || !appendPolicy(Number(node.nodeIndex), source)) {{
        dynamicCspMetaPolicyFailure = true;
      }}
    }} catch (_error) {{
      dynamicCspMetaPolicyFailure = true;
    }}
  }};
  const executeInsertedScripts = (node) => {{
    if (suppressHostCommands > 0 || !node) return;
    if (captureDynamicReferrerMetaPolicy) captureDynamicReferrerMetaPolicy(node);
    captureDynamicCspMetaPolicy(node);
    if (Number(node.nodeType) === 1 && String(node.localName || "").toLowerCase() === "script"
        && node.__glassDynamicScriptStarted === false) {{
      node.__glassDynamicScriptStarted = true;
      const type = asciiLowercase(node.getAttribute("type") || "");
      const source = String(node.textContent || "");
      const classic = type === "" || classicJavascriptMimeTypes.has(type);
      if (!node.getAttribute("src") && classic && source) {{
        pushCommand({{ kind: "startScript", node_index: node.nodeIndex }});
        if (typeof globalThis.__glassReportOnlyInlineScript === "function") {{
          try {{
            const encodedViolations = globalThis.__glassReportOnlyInlineScript(
              source,
              String(node.getAttribute("nonce") || ""),
            );
            if (encodedViolations) {{
              const violations = JSON.parse(String(encodedViolations));
              if (violations.length > 0
                  && typeof globalThis.__glassDispatchCspViolations === "function") {{
                globalThis.__glassDispatchCspViolations(violations);
              }}
            }}
          }} catch (_error) {{}}
        }}
        let allowed = !dynamicCspMetaPolicyFailure;
        if (typeof globalThis.__glassAllowsInlineScript === "function") {{
          try {{
            allowed = allowed && Boolean(globalThis.__glassAllowsInlineScript(
              source,
              String(node.getAttribute("nonce") || ""),
            ));
          }} catch (_error) {{
            allowed = false;
          }}
        }}
        if (!allowed) {{
          dispatchTarget(node, createEvent("error"));
          return;
        }}
        try {{
          (0, eval)(source);
        }} catch (error) {{
          dispatchTarget(node, createEvent("error"));
        }}
      }}
    }}
    for (const child of node.__glassChildren || []) executeInsertedScripts(child);
  }};
  const makeDetachedText = (value) => {{
    let textContent = String(value);
    if (textContent.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
    let nodeIndex = allocateTemporaryNodeIndex();
      const text = {{
      nodeIndex,
      parentIndex: null,
      nodeType: 3,
      nodeName: "#text",
      nodeValue: textContent,
      remove() {{
        const parent = text.__glassParent || null;
        if (!parent && text.parentIndex === null) return;
        const commitRemoval = !text.__glassCreated || text.parentIndex !== null;
        queueFragmentChildRemoval(parent, text);
        if (parent && Array.isArray(parent.__glassChildren)) {{
          parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== text);
        }}
        text.__glassParent = null;
        text.parentIndex = null;
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: nodeIndex }});
      }},
    }};
    Object.defineProperty(text, "__glassParent", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: null,
    }});
    Object.defineProperty(text, "__glassCreated", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: true,
    }});
    Object.defineProperty(text, "__glassTextValue", {{
      enumerable: false,
      configurable: false,
      get() {{ return textContent; }},
    }});
    Object.defineProperty(text, "__glassMarkup", {{
      enumerable: false,
      configurable: false,
      get() {{ return serializeHtmlTextNode(text, textContent); }},
    }});
    Object.defineProperty(text, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        nodeIndex = Number(nextEntry.nodeIndex);
        text.nodeIndex = nodeIndex;
        text.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        textContent = String(nextEntry.nodeValue || "");
      }},
    }});
    Object.defineProperty(text, "__glassSetNativeIndex", {{ enumerable: false, configurable: false, value(nextIndex) {{
      nodeIndex = Number(nextIndex);
      text.nodeIndex = nodeIndex;
    }} }});
    Object.defineProperty(text, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{ return text.__glassParent || null; }},
    }});
    Object.defineProperty(text, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (text.__glassParent) return text.__glassParent;
        const owner = globalThis.document;
        return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(text)
          ? owner
          : null;
      }},
    }});
    Object.defineProperty(text, "ownerDocument", {{
      enumerable: false,
      configurable: false,
      get() {{ return templateContentsOwnerDocumentForNode(text); }},
    }});
    Object.defineProperty(text, "nodeValue", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ text.textContent = next; }},
    }});
    Object.defineProperty(text, "textContent", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{
        const value = String(next);
        if (value.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
        textContent = value;
        if (text.__glassParent && typeof text.__glassParent.__glassSyncContent === "function") text.__glassParent.__glassSyncContent();
        pushCommand({{ kind: "setTextContent", node_index: nodeIndex, value }});
      }},
    }});
    try {{ Object.setPrototypeOf(text, TextNative.prototype); }} catch (_error) {{}}
    defineTreeAccessors(text);
    mutationCreatedNodes.set(nodeIndex, text);
    scriptNodeObjects.set(nodeIndex, text);
    pushCommand({{ kind: "createTextNode", node_index: nodeIndex, value: textContent }});
    return text;
  }};
  const makeDetachedComment = (value) => {{
    let textContent = String(value);
    if (textContent.length > {storage_value_limit}) throw new RangeError("native comment node exceeds its limit");
    let nodeIndex = allocateTemporaryNodeIndex();
    const comment = {{
      nodeIndex,
      parentIndex: null,
      nodeType: 8,
      nodeName: "#comment",
      nodeValue: textContent,
      remove() {{
        const parent = comment.__glassParent || null;
        if (!parent && comment.parentIndex === null) return;
        const commitRemoval = !comment.__glassCreated || comment.parentIndex !== null;
        queueFragmentChildRemoval(parent, comment);
        if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== comment);
        comment.__glassParent = null;
        comment.parentIndex = null;
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: nodeIndex }});
      }},
    }};
    Object.defineProperty(comment, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
    Object.defineProperty(comment, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
    Object.defineProperty(comment, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return ""; }} }});
    Object.defineProperty(comment, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return "<!--" + textContent + "-->"; }} }});
    Object.defineProperty(comment, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        nodeIndex = Number(nextEntry.nodeIndex);
        comment.nodeIndex = nodeIndex;
        comment.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        textContent = String(nextEntry.nodeValue || "");
      }},
    }});
    Object.defineProperty(comment, "__glassSetNativeIndex", {{ enumerable: false, configurable: false, value(nextIndex) {{
      nodeIndex = Number(nextIndex);
      comment.nodeIndex = nodeIndex;
    }} }});
    Object.defineProperty(comment, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{
        let current = comment.__glassParent || null;
        while (current && current.nodeType !== 1) current = current.__glassParent || null;
        return current || null;
      }},
    }});
    Object.defineProperty(comment, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (comment.__glassParent) return comment.__glassParent;
        const owner = globalThis.document;
        return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(comment)
          ? owner
          : null;
      }},
    }});
    Object.defineProperty(comment, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return templateContentsOwnerDocumentForNode(comment); }} }});
    Object.defineProperty(comment, "nodeValue", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ comment.textContent = next; }},
    }});
    Object.defineProperty(comment, "textContent", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{
        const nextValue = String(next);
        if (nextValue.length > {storage_value_limit}) throw new RangeError("native comment node exceeds its limit");
        textContent = nextValue;
        if (comment.__glassParent && typeof comment.__glassParent.__glassSyncContent === "function") comment.__glassParent.__glassSyncContent();
        pushCommand({{ kind: "setTextContent", node_index: nodeIndex, value: nextValue }});
      }},
    }});
    Object.defineProperty(comment, "data", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ comment.textContent = next; }},
    }});
    try {{ Object.setPrototypeOf(comment, CommentNative.prototype); }} catch (_error) {{}}
    defineTreeAccessors(comment);
    mutationCreatedNodes.set(nodeIndex, comment);
    scriptNodeObjects.set(nodeIndex, comment);
    pushCommand({{ kind: "createComment", node_index: nodeIndex, value: textContent }});
    return comment;
  }};
  const makeDetachedDocumentType = (name, publicId = "", systemId = "", ownerDocumentResolver = () => globalThis.document) => {{
    let normalizedName = String(name);
    if (!/^[A-Za-z][A-Za-z0-9:_-]*$/.test(normalizedName)) throw new TypeError("invalid document type name");
    let publicIdentifier = String(publicId);
    let systemIdentifier = String(systemId);
    if (publicIdentifier.length > {storage_value_limit} || systemIdentifier.length > {storage_value_limit}) throw new RangeError("native document type identifier exceeds its limit");
    let nodeIndex = allocateTemporaryNodeIndex();
    const documentType = {{
      nodeIndex,
      parentIndex: null,
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
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: nodeIndex }});
      }},
    }};
    Object.defineProperty(documentType, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
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
      documentType.name = normalizedName;
      documentType.nodeName = documentType.name;
      publicIdentifier = nextEntry.publicId == null ? "" : String(nextEntry.publicId);
      systemIdentifier = nextEntry.systemId == null ? "" : String(nextEntry.systemId);
      documentType.publicId = publicIdentifier;
      documentType.systemId = systemIdentifier;
    }} }});
    Object.defineProperty(documentType, "__glassSetNativeIndex", {{ enumerable: false, configurable: false, value(nextIndex) {{
      nodeIndex = Number(nextIndex);
      documentType.nodeIndex = nodeIndex;
    }} }});
    Object.defineProperty(documentType, "parentNode", {{ enumerable: false, configurable: false, get() {{
      if (documentType.__glassParent) return documentType.__glassParent;
      const owner = ownerDocumentResolver();
      return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(documentType) ? owner : null;
    }} }});
    Object.defineProperty(documentType, "parentElement", {{ enumerable: false, configurable: false, get() {{ return null; }} }});
    Object.defineProperty(documentType, "ownerDocument", {{ enumerable: false, configurable: false, get() {{
      return documentType.__glassAdoptedOwnerDocument || ownerDocumentResolver() || null;
    }} }});
    try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
    defineTreeAccessors(documentType);
    mutationCreatedNodes.set(nodeIndex, documentType);
    scriptNodeObjects.set(nodeIndex, documentType);
    pushCommand({{ kind: "createDocumentType", node_index: nodeIndex, name: normalizedName, public_id: publicIdentifier, system_id: systemIdentifier }});
    return documentType;
  }};
  let defineTreeAccessorsForNativeFragments = null;
  const makeDocumentFragment = (snapshotEntry = null, templateHost = null) => {{
    const fragment = {{
      nodeIndex: snapshotEntry ? Number(snapshotEntry.nodeIndex) : null,
      parentIndex: snapshotEntry && snapshotEntry.parentIndex != null ? Number(snapshotEntry.parentIndex) : null,
      nodeType: 11,
      nodeName: "#document-fragment",
      __glassFragment: true,
      __glassTemplateHost: templateHost,
      templateHostIndex: snapshotEntry && snapshotEntry.templateHostIndex != null
        ? Number(snapshotEntry.templateHostIndex)
        : null,
      __glassChildren: [],
      __glassParent: null,
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
        child.parentIndex = null;
        child.parentIndex = this.nodeIndex == null ? null : this.nodeIndex;
        if (!suppressHostCommands) {{
          if (this.nodeIndex !== null && this.nodeIndex !== undefined) {{
            pushCommand({{ kind: "appendChild", parent_index: this.nodeIndex, child_index: child.nodeIndex }});
          }} else {{
            if (oldParentIndex !== null && oldParentIndex !== undefined) {{
              pushCommand({{ kind: "removeNode", node_index: child.nodeIndex }});
            }}
            if (this.__glassTemplateHost) {{
              pushCommand({{ kind: "setInnerHtml", node_index: this.__glassTemplateHost.nodeIndex, value: this.innerHTML }});
            }}
          }}
        }}
        queueMutation({{ type: "childList", target: this, addedNodes: [child], removedNodes: [], previousSibling: this.__glassChildren.length > 1 ? this.__glassChildren[this.__glassChildren.length - 2] : null, nextSibling: null }});
        return child;
      }},
      insertBefore(child, before) {{
        if (child === this) throw new TypeError("a node cannot contain itself");
        if (before == null) return this.appendChild(child);
        if (child && child.__glassFragment === true) {{
          const children = child.__glassChildren.slice();
          for (const fragmentChild of children) this.insertBefore(fragmentChild, before);
          child.__glassChildren = [];
          return child;
        }}
        if (!child || ![1, 3, 8].includes(Number(child.nodeType)))
          throw new TypeError("DocumentFragment children must be elements or text nodes");
        if (before.__glassParent !== this) throw new TypeError("reference node is not a child");
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
        child.parentIndex = null;
        child.parentIndex = this.nodeIndex == null ? null : this.nodeIndex;
        if (!suppressHostCommands) {{
          if (this.nodeIndex !== null && this.nodeIndex !== undefined) {{
            pushCommand({{ kind: "insertBefore", parent_index: this.nodeIndex, child_index: child.nodeIndex, before_index: before.nodeIndex }});
          }} else {{
            if (oldParentIndex !== null && oldParentIndex !== undefined) {{
              pushCommand({{ kind: "removeNode", node_index: child.nodeIndex }});
            }}
            if (this.__glassTemplateHost) {{
              pushCommand({{ kind: "setInnerHtml", node_index: this.__glassTemplateHost.nodeIndex, value: this.innerHTML }});
            }}
          }}
        }}
        queueMutation({{ type: "childList", target: this, addedNodes: [child], removedNodes: [], previousSibling, nextSibling }});
        return child;
      }},
    }};
    Object.defineProperty(fragment, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        fragment.nodeIndex = Number(nextEntry.nodeIndex);
        fragment.parentIndex = nextEntry.parentIndex == null ? null : Number(nextEntry.parentIndex);
        fragment.templateHostIndex = nextEntry.templateHostIndex == null ? null : Number(nextEntry.templateHostIndex);
      }},
    }});
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
      get() {{ return templateContentsOwnerDocumentForNode(fragment); }},
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
        if (value.length > storageValueLimit) throw new RangeError("native fragment innerHTML exceeds its limit");
        const oldChildren = fragment.__glassChildren.slice();
        for (const child of oldChildren) {{
          child.__glassParent = null;
          child.parentIndex = null;
        }}
        fragment.__glassChildren = [];
        const host = fragment.__glassTemplateHost
          || (fragment.templateHostIndex != null && globalThis.__glassHostElements instanceof Map
            ? globalThis.__glassHostElements.get(fragment.templateHostIndex)
            : null);
        if (host) {{
          suppressHostCommands += 1;
          try {{
            populateDetachedFragment(fragment, value, makeDetachedElement, makeDetachedText, makeDetachedComment, undefined, true);
          }} finally {{
            suppressHostCommands -= 1;
          }}
          pushCommand({{ kind: "setInnerHtml", node_index: host.nodeIndex, value }});
        }} else {{
          populateDetachedFragment(fragment, value, makeDetachedElement, makeDetachedText, makeDetachedComment, undefined, true);
        }}
        if (typeof globalThis.__glassQueueInnerHtmlMutation === "function")
          globalThis.__glassQueueInnerHtmlMutation(fragment, oldChildren, fragment.__glassChildren);
      }},
    }});
    if (defineTreeAccessorsForNativeFragments) defineTreeAccessorsForNativeFragments(fragment);
    const constructor = globalThis.DocumentFragment;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(fragment, constructor.prototype); }} catch (_error) {{}}
    }}
    return fragment;
  }};
  const templateContentsOwnerDocuments = globalThis.__glassTemplateContentsOwnerDocuments instanceof Map
    ? globalThis.__glassTemplateContentsOwnerDocuments
    : new Map();
  globalThis.__glassTemplateContentsOwnerDocuments = templateContentsOwnerDocuments;
  let templateContentsOwnerDocument = null;
  const getTemplateContentsOwnerDocument = () => {{
    const contextId = String(host.context_id || "native");
    const markOwnerDocument = (ownerDocument) => {{
      ownerDocument.__glassNativeContextId = contextId;
      ownerDocument.__glassNativeGeneration = Number(host.generation) || 0;
      ownerDocument.__glassNativeOrigin = String(host.origin || "null");
      ownerDocument.__glassNativeScriptNodes = Array.isArray(state.scriptNodes)
        ? state.scriptNodes
        : [];
      ownerDocument.__glassNativeFrameBinding = null;
      ownerDocument.__glassIsTemplateContentsOwnerDocument = true;
      ownerDocument.__glassGetTemplateContentsOwnerDocument = () => ownerDocument;
      return ownerDocument;
    }};
    if (templateContentsOwnerDocument) {{
      markOwnerDocument(templateContentsOwnerDocument);
      if (typeof globalThis.Document === "function") {{
        try {{ Object.setPrototypeOf(templateContentsOwnerDocument, globalThis.Document.prototype); }} catch (_error) {{}}
      }}
      return templateContentsOwnerDocument;
    }}
    const cachePrefix = contextId + ":";
    const cacheKey = cachePrefix + String(host.generation ?? "");
    const cached = templateContentsOwnerDocuments.get(cacheKey);
    if (cached) {{
      templateContentsOwnerDocument = markOwnerDocument(cached);
      if (typeof globalThis.Document === "function") {{
        try {{ Object.setPrototypeOf(templateContentsOwnerDocument, globalThis.Document.prototype); }} catch (_error) {{}}
      }}
      return templateContentsOwnerDocument;
    }}
    templateContentsOwnerDocument = {{
      nodeType: 9,
      nodeName: "#document",
      __glassNativeContextId: contextId,
      __glassIsTemplateContentsOwnerDocument: true,
      __glassGetTemplateContentsOwnerDocument() {{ return templateContentsOwnerDocument; }},
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
        const element = makeDetachedElement(name, HTML_NAMESPACE);
        Object.defineProperty(element, "__glassOwnerDocumentOverride", {{ value: templateContentsOwnerDocument }});
        return element;
      }},
      createElementNS(namespace, name) {{
        const element = makeDetachedElement(name, namespace);
        Object.defineProperty(element, "__glassOwnerDocumentOverride", {{ value: templateContentsOwnerDocument }});
        return element;
      }},
      createAttribute(name) {{
        return makeAttributeNode(name, "", () => templateContentsOwnerDocument);
      }},
      createAttributeNS(namespace, qualifiedName) {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        return makeAttributeNode(qualifiedName, "", () => templateContentsOwnerDocument, namespaceURI);
      }},
      createTextNode(value) {{
        const node = makeDetachedText(value);
        Object.defineProperty(node, "__glassOwnerDocumentOverride", {{ value: templateContentsOwnerDocument }});
        return node;
      }},
      createComment(value) {{
        const node = makeDetachedComment(value);
        Object.defineProperty(node, "__glassOwnerDocumentOverride", {{ value: templateContentsOwnerDocument }});
        return node;
      }},
      createDocumentFragment() {{
        const fragment = makeDocumentFragment();
        Object.defineProperty(fragment, "__glassOwnerDocumentOverride", {{ value: templateContentsOwnerDocument }});
        return fragment;
      }},
      importNode(node, options = false) {{
        return importNativeNode(templateContentsOwnerDocument, node, options);
      }},
      adoptNode(node) {{ return adoptNativeNode(templateContentsOwnerDocument, node); }},
      implementation: {{
        createDocumentType(name, publicId = "", systemId = "") {{
          return makeDetachedDocumentType(
            name,
            publicId,
            systemId,
            () => templateContentsOwnerDocument,
          );
        }},
      }},
    }};
    markOwnerDocument(templateContentsOwnerDocument);
    templateContentsOwnerDocuments.set(cacheKey, templateContentsOwnerDocument);
    for (const key of templateContentsOwnerDocuments.keys()) {{
      if (key.startsWith(cachePrefix) && key !== cacheKey) templateContentsOwnerDocuments.delete(key);
    }}
    while (templateContentsOwnerDocuments.size > 128) {{
      templateContentsOwnerDocuments.delete(templateContentsOwnerDocuments.keys().next().value);
    }}
    if (typeof globalThis.Document === "function") {{
      try {{ Object.setPrototypeOf(templateContentsOwnerDocument, globalThis.Document.prototype); }} catch (_error) {{}}
    }}
    return templateContentsOwnerDocument;
  }};
  const templateContentsOwnerDocumentForNode = (node) => {{
    if (!node) return globalThis.document || null;
    if (node.__glassAdoptedOwnerDocument) {{
      const adoptedOwnerDocument = node.__glassAdoptedOwnerDocument;
      if (adoptedOwnerDocument.__glassIsTemplateContentsOwnerDocument !== true) {{
        const contextId = String(adoptedOwnerDocument.__glassNativeContextId || "");
        const generation = Number(adoptedOwnerDocument.__glassNativeGeneration);
        const currentDocument = globalThis.document;
        if (contextId && currentDocument
            && String(currentDocument.__glassNativeContextId || "") === contextId
            && Number(currentDocument.__glassNativeGeneration) === generation) {{
          node.__glassAdoptedOwnerDocument = currentDocument;
          return currentDocument;
        }}
        const frameDocuments = globalThis.__glassFrameDocumentCache;
        const currentFrameDocument = frameDocuments instanceof Map
          ? frameDocuments.get(contextId)?.document
          : null;
        if (contextId && currentFrameDocument
            && Number(currentFrameDocument.__glassNativeGeneration) === generation) {{
          node.__glassAdoptedOwnerDocument = currentFrameDocument;
          return currentFrameDocument;
        }}
      }}
      return adoptedOwnerDocument;
    }}
    if (node.__glassOwnerDocumentOverride) return node.__glassOwnerDocumentOverride;
    let current = node;
    while (current) {{
      if (Number(current.nodeType) === 11
          && (current.templateHostIndex != null || current.__glassTemplateHost)) {{
        return getTemplateContentsOwnerDocument();
      }}
      current = current.__glassParent || null;
    }}
    return globalThis.document || null;
  }};
  const makeSnapshotText = (initialEntry) => {{
    const isComment = Number(initialEntry.nodeType) === 8;
    let textContent = String(initialEntry.nodeValue || "");
    const text = {{
      nodeIndex: initialEntry.nodeIndex,
      parentIndex: initialEntry.parentIndex == null ? null : initialEntry.parentIndex,
      nodeType: isComment ? 8 : 3,
      nodeName: isComment ? "#comment" : "#text",
      nodeValue: textContent,
      remove() {{
        const parent = text.__glassParent || null;
        if (!parent && text.parentIndex === null) return;
        const commitRemoval = !text.__glassCreated || text.parentIndex !== null;
        queueFragmentChildRemoval(parent, text);
        if (parent && Array.isArray(parent.__glassChildren)) {{
          parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== text);
        }}
        text.__glassParent = null;
        text.parentIndex = null;
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: text.nodeIndex }});
      }},
    }};
    Object.defineProperty(text, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
    Object.defineProperty(text, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: false }});
    Object.defineProperty(text, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return isComment ? "" : textContent; }} }});
    Object.defineProperty(text, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return isComment ? "<!--" + textContent + "-->" : serializeHtmlTextNode(text, textContent); }} }});
    Object.defineProperty(text, "nodeValue", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ text.textContent = next; }},
    }});
    Object.defineProperty(text, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return templateContentsOwnerDocumentForNode(text); }} }});
    Object.defineProperty(text, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (text.__glassParent) return text.__glassParent;
        const owner = globalThis.document;
        return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(text)
          ? owner
          : null;
      }},
    }});
    Object.defineProperty(text, "textContent", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{
        const value = String(next);
        if (value.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
        textContent = value;
        if (text.__glassParent && typeof text.__glassParent.__glassSyncContent === "function") text.__glassParent.__glassSyncContent();
        pushCommand({{ kind: "setTextContent", node_index: text.nodeIndex, value }});
      }},
    }});
    Object.defineProperty(text, "data", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ text.textContent = next; }},
    }});
    Object.defineProperty(text, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        text.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        textContent = String(nextEntry.nodeValue || "");
      }},
    }});
    try {{
      Object.setPrototypeOf(text, isComment ? CommentNative.prototype : TextNative.prototype);
    }} catch (_error) {{}}
    if (defineTreeAccessorsForNativeFragments) defineTreeAccessorsForNativeFragments(text);
    return text;
  }};
  const previousElements = globalThis.__glassHostElements instanceof Map
    ? globalThis.__glassHostElements
    : new Map();
  const previousNodes = globalThis.__glassHostNodes instanceof Map
    ? globalThis.__glassHostNodes
    : new Map();
  const snapshotNodes = Array.isArray(state.nodes) ? state.nodes : [];
  const snapshotElements = Array.isArray(state.elements) ? state.elements : [];
  // The native exact-state key plus these private closure references prove
  // the cached collections match. Public maps are fresh copies below, so
  // page code cannot mutate the private maps and force an O(nodes) admission
  // scan on every unchanged host install.
  const cachedCollectionsMatchCurrentView = Boolean(
    reuseCachedHostSnapshotState
      && cachedHostCollections
      && cachedHostCollections.elementsByIndexView === previousElements
      && cachedHostCollections.nodesByIndexView === previousNodes
      && Array.isArray(cachedHostCollections.elements)
      && Array.isArray(cachedHostCollections.textNodes)
      && Array.isArray(cachedHostCollections.commentNodes)
      && Array.isArray(cachedHostCollections.documentTypeNodes)
      && Array.isArray(cachedHostCollections.fragmentNodes),
  );
  const reuseExistingHostTree = cachedCollectionsMatchCurrentView;
  const snapshotNodesByIndex = new Map(snapshotNodes
    .filter((entry) => entry && Number.isSafeInteger(Number(entry.nodeIndex)))
    .map((entry) => [Number(entry.nodeIndex), entry]));
  const snapshotElementsByIndex = new Map(snapshotElements
    .filter((entry) => entry && Number.isSafeInteger(Number(entry.nodeIndex)))
    .map((entry) => [Number(entry.nodeIndex), entry]));
  const snapshotTreeNodeIndexes = new Set();
  const rootTreeSnapshotEntry = snapshotNodesByIndex.get(0);
  if (rootTreeSnapshotEntry && Array.isArray(rootTreeSnapshotEntry.children)) {{
    const pendingSnapshotIndexes = [0];
    while (pendingSnapshotIndexes.length > 0
        && snapshotTreeNodeIndexes.size <= {native_dom_max_nodes}) {{
      const nodeIndex = Number(pendingSnapshotIndexes.pop());
      if (!Number.isSafeInteger(nodeIndex) || snapshotTreeNodeIndexes.has(nodeIndex)) continue;
      snapshotTreeNodeIndexes.add(nodeIndex);
      const node = snapshotNodesByIndex.get(nodeIndex);
      if (node && Array.isArray(node.children)) pendingSnapshotIndexes.push(...node.children);
      const element = snapshotElementsByIndex.get(nodeIndex);
      if (element && element.templateContentIndex != null) {{
        pendingSnapshotIndexes.push(Number(element.templateContentIndex));
      }}
    }}
  }} else {{
    for (const entry of [...snapshotNodes, ...snapshotElements]) {{
      if (entry && Number.isSafeInteger(Number(entry.nodeIndex)))
        snapshotTreeNodeIndexes.add(Number(entry.nodeIndex));
    }}
  }}
  const preserveDetachedCreatedNode = (node) => Boolean(
    node && node.__glassCreated === true
      && !snapshotTreeNodeIndexes.has(Number(node.nodeIndex)),
  );
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("tree_admission_checked");
  const snapshotTextContentForNode = reuseExistingHostTree
    ? null
    : createSnapshotTextContentResolver(snapshotNodes);
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("text_content_resolver_ready");
  let elementBuildIndex = -1;
  let elementBuildMode = "none";
  const elements = reuseExistingHostTree ? cachedHostCollections.elements : [];
  try {{
    if (!reuseExistingHostTree) for (let index = 0; index < snapshotElements.length; index += 1) {{
      const entry = snapshotElements[index];
      elementBuildIndex = index;
      const existing = scriptNodeAliasesByIndex.get(entry.nodeIndex) || previousElements.get(entry.nodeIndex);
      if (existing && typeof existing.__glassRefresh === "function") {{
        elementBuildMode = "refresh";
        if (!preserveDetachedCreatedNode(existing))
          existing.__glassRefresh(entry, snapshotTextContentForNode);
        elements.push(existing);
        continue;
      }}
      elementBuildMode = "create";
      elements.push(makeElement(entry, snapshotTextContentForNode));
    }}
  }} catch (error) {{
    if (typeof globalThis.__glassNativeHostViewMark === "function") {{
      const failedEntry = snapshotElements[elementBuildIndex];
      globalThis.__glassNativeHostViewMark(
        `elements_error:${{elementBuildIndex}}:${{String(failedEntry?.tagName || "")}}:${{Object.keys(failedEntry?.attributes || {{}}).length}}:${{String(failedEntry?.text || "").length}}:${{elementBuildMode}}:${{nativeElementBuildPhase}}`,
      );
    }}
    throw error;
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("elements_ready");
  const elementsByIndex = reuseExistingHostTree ? cachedHostCollections.elementsByIndex : new Map();
  if (!reuseExistingHostTree) for (const element of elements) elementsByIndex.set(element.nodeIndex, element);
  const textNodes = reuseExistingHostTree ? cachedHostCollections.textNodes : [];
  if (!reuseExistingHostTree) for (const entry of snapshotNodes) {{
    if (!entry || Number(entry.nodeType) !== 3) continue;
      const existing = previousNodes.get(entry.nodeIndex);
      const scriptAlias = scriptNodeAliasesByIndex.get(entry.nodeIndex);
      const reusable = scriptAlias || existing;
      if (reusable && typeof reusable.__glassRefresh === "function") {{
        if (!preserveDetachedCreatedNode(reusable)) reusable.__glassRefresh(entry);
        textNodes.push(reusable);
        continue;
      }}
      textNodes.push(makeSnapshotText(entry));
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("text_nodes_ready");
  const commentNodes = reuseExistingHostTree ? cachedHostCollections.commentNodes : [];
  if (!reuseExistingHostTree) for (const entry of snapshotNodes) {{
    if (!entry || Number(entry.nodeType) !== 8) continue;
      const existing = previousNodes.get(entry.nodeIndex);
      const scriptAlias = scriptNodeAliasesByIndex.get(entry.nodeIndex);
      const reusable = scriptAlias || existing;
      if (reusable && typeof reusable.__glassRefresh === "function") {{
        if (!reuseExistingHostTree) reusable.__glassRefresh(entry);
        commentNodes.push(reusable);
        continue;
      }}
      commentNodes.push(makeSnapshotText(entry));
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("comment_nodes_ready");
  const documentTypeNodes = reuseExistingHostTree ? cachedHostCollections.documentTypeNodes : [];
  if (!reuseExistingHostTree) for (const entry of snapshotNodes) {{
      if (!entry || Number(entry.nodeType) !== 10) continue;
      const existing = scriptNodeAliasesByIndex.get(entry.nodeIndex) || previousNodes.get(entry.nodeIndex);
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
          const owner = parent || (documentType.parentIndex === 0 ? globalThis.document : null);
          if (owner && Array.isArray(owner.__glassChildren)) owner.__glassChildren = owner.__glassChildren.filter(candidate => candidate !== documentType);
          documentType.__glassParent = null;
          documentType.parentIndex = null;
          pushCommand({{ kind: "removeNode", node_index: documentType.nodeIndex }});
        }},
      }};
      if (!Object.prototype.hasOwnProperty.call(documentType, "__glassRefresh")) {{
        Object.defineProperty(documentType, "__glassRefresh", {{
          enumerable: false,
          configurable: false,
          value(nextEntry) {{
            documentType.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
            documentType.name = String(nextEntry.nodeName || documentType.name || "");
            documentType.nodeName = documentType.name;
            documentType.publicId = nextEntry.publicId == null ? "" : String(nextEntry.publicId);
            documentType.systemId = nextEntry.systemId == null ? "" : String(nextEntry.systemId);
          }},
        }});
      }}
      if (!reuseExistingHostTree) {{
        if (existing && typeof documentType.__glassRefresh === "function"
            && !preserveDetachedCreatedNode(documentType)) documentType.__glassRefresh(entry);
        documentType.parentIndex = entry.parentIndex == null ? null : entry.parentIndex;
        documentType.name = String(entry.nodeName || "");
        documentType.nodeName = documentType.name;
        documentType.publicId = entry.publicId == null ? "" : String(entry.publicId);
        documentType.systemId = entry.systemId == null ? "" : String(entry.systemId);
      }}
      if (!Object.prototype.hasOwnProperty.call(documentType, "ownerDocument")) {{
        Object.defineProperty(documentType, "ownerDocument", {{
          enumerable: false,
          configurable: false,
          get() {{ return globalThis.document || null; }},
        }});
      }}
      if (!Object.prototype.hasOwnProperty.call(documentType, "parentNode")) {{
        Object.defineProperty(documentType, "parentNode", {{
          enumerable: false,
          configurable: false,
          get() {{
            if (documentType.__glassParent) return documentType.__glassParent;
            const owner = globalThis.document;
            return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(documentType)
              ? owner
              : null;
          }},
        }});
      }}
      if (!Object.prototype.hasOwnProperty.call(documentType, "parentElement")) {{
        Object.defineProperty(documentType, "parentElement", {{
          enumerable: false,
          configurable: false,
          get() {{
            const parent = documentType.__glassParent || null;
            return parent && parent.nodeType === 1 ? parent : null;
          }},
        }});
      }}
      try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
      if (defineTreeAccessorsForNativeFragments) defineTreeAccessorsForNativeFragments(documentType);
      documentTypeNodes.push(documentType);
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("document_type_nodes_ready");
  const templateContentsCache = globalThis.__glassTemplateContents instanceof Map
    ? globalThis.__glassTemplateContents
    : (globalThis.__glassTemplateContents = new Map());
  const fragmentNodes = reuseExistingHostTree ? cachedHostCollections.fragmentNodes : [];
  if (!reuseExistingHostTree) for (const entry of snapshotNodes) {{
      if (!entry || Number(entry.nodeType) !== 11) continue;
      const hostIndex = entry.templateHostIndex == null ? null : Number(entry.templateHostIndex);
      const cached = hostIndex == null ? null : templateContentsCache.get(hostIndex);
      const existing = cached || previousNodes.get(entry.nodeIndex);
      const fragment = existing && typeof existing.__glassRefresh === "function"
        ? existing
        : makeDocumentFragment(entry);
      if (!reuseExistingHostTree && typeof fragment.__glassRefresh === "function"
          && !preserveDetachedCreatedNode(fragment)) fragment.__glassRefresh(entry);
      if (hostIndex != null) templateContentsCache.set(hostIndex, fragment);
      fragmentNodes.push(fragment);
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("fragment_nodes_ready");
  const nodesByIndex = reuseExistingHostTree ? cachedHostCollections.nodesByIndex : new Map();
  if (!reuseExistingHostTree) for (const nodes of [elements, textNodes, commentNodes, documentTypeNodes, fragmentNodes]) {{
    for (const node of nodes) nodesByIndex.set(node.nodeIndex, node);
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("nodes_by_index_ready");
  // Script-created detached nodes are not in the document snapshot. Keep
  // their objects addressable across realm refreshes so detached subtrees
  // retain their children and can later be inserted into the live document.
  for (const node of scriptNodeObjects.values()) {{
    const nodeIndex = Number(node && node.nodeIndex);
    if (node && Number.isSafeInteger(nodeIndex) && !nodesByIndex.has(nodeIndex)) {{
      nodesByIndex.set(nodeIndex, node);
    }}
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("collections_ready");
  const elementsByIndexView = new Map(elementsByIndex);
  const nodesByIndexView = new Map(nodesByIndex);
  globalThis.__glassHostElements = elementsByIndexView;
  globalThis.__glassHostNodes = nodesByIndexView;
  cachedHostCollections = {{
    elements,
    textNodes,
    commentNodes,
    documentTypeNodes,
    fragmentNodes,
    elementsByIndex,
    nodesByIndex,
    elementsByIndexView,
    nodesByIndexView,
  }};
  if (!reuseExistingHostTree) {{
    for (const element of elements) {{
      if (preserveDetachedCreatedNode(element)) continue;
      element.__glassChildren = [];
      element.__glassParent = null;
    }}
    for (const text of textNodes) {{
      if (preserveDetachedCreatedNode(text)) continue;
      text.__glassChildren = [];
      text.__glassParent = null;
    }}
    for (const comment of commentNodes) {{
      if (preserveDetachedCreatedNode(comment)) continue;
      comment.__glassChildren = [];
      comment.__glassParent = null;
    }}
    for (const documentType of documentTypeNodes) {{
      if (preserveDetachedCreatedNode(documentType)) continue;
      documentType.__glassChildren = [];
      documentType.__glassParent = null;
    }}
    for (const fragment of fragmentNodes) {{
      if (preserveDetachedCreatedNode(fragment)) continue;
      fragment.__glassChildren = [];
      fragment.__glassParent = null;
    }}
    if (snapshotNodes.length > 0) {{
      for (const snapshotNode of snapshotNodes) {{
        const parent = nodesByIndex.get(snapshotNode.nodeIndex);
        if (!parent || preserveDetachedCreatedNode(parent)
            || !Array.isArray(snapshotNode.children)) continue;
        for (const childIndex of snapshotNode.children) {{
          const child = nodesByIndex.get(childIndex);
          if (!child) continue;
          parent.__glassChildren.push(child);
          child.__glassParent = parent;
        }}
      }}
    }} else {{
      for (const element of elements) {{
        if (element.parentIndex === null) continue;
        const parent = elementsByIndex.get(element.parentIndex);
        if (!parent) continue;
        parent.__glassChildren.push(element);
        element.__glassParent = parent;
      }}
    }}
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("tree_linking_ready");
  globalThis.__glassApplyNativeCommand = (command) => {{
    if (!command || typeof command !== "object") throw new TypeError("native frame command must be an object");
    const current = globalThis.__glassHostNodes;
    const kind = String(command.kind);
    const temporaryIndex = Number(command.node_index);
    const temporaryIndexes = globalThis.__glassScriptNodeIndexesByTemporary;
    const resolveNodeIndex = (index) => {{
      const value = Number(index);
      return temporaryIndexes instanceof Map ? temporaryIndexes.get(value) ?? value : value;
    }};
    if (kind === "appendChild" || kind === "insertBefore") {{
      const parent = current instanceof Map
        ? current.get(resolveNodeIndex(command.parent_index))
        : null;
      const child = current instanceof Map
        ? current.get(resolveNodeIndex(command.child_index))
        : null;
      const before = kind === "insertBefore" && command.before_index != null
        ? current instanceof Map ? current.get(resolveNodeIndex(command.before_index)) : null
        : null;
      if (!parent || !child || kind === "insertBefore" && command.before_index != null && !before) {{
        throw new Error("native frame child insertion target is detached");
      }}
      suppressHostCommands += 1;
      try {{
        if (kind === "insertBefore") parent.insertBefore(child, before);
        else parent.appendChild(child);
      }} finally {{
        suppressHostCommands -= 1;
      }}
      pushCurrentDocumentCommand(command);
      return true;
    }}
    const nodeIndex = resolveNodeIndex(temporaryIndex);
    const element = current instanceof Map ? current.get(nodeIndex) : null;
    if (!element) throw new Error(
      "native frame command target is detached: " + String(command.kind)
        + " index " + temporaryIndex + " resolved " + nodeIndex
        + " (identity mapped: "
        + Boolean(temporaryIndexes instanceof Map && temporaryIndexes.has(temporaryIndex)) + ")",
    );
    switch (kind) {{
      case "focus": element.focus(); break;
      case "blur": element.blur(); break;
      case "click": element.click(); break;
      case "setValue": element.value = String(command.value); break;
      case "setSelection": element.setSelectionRange(command.start, command.end, command.direction); break;
      case "mediaLoad": if (typeof element.load === "function") element.load(); else throw new TypeError("native media load is unsupported"); break;
      case "setChecked": element.checked = Boolean(command.checked); break;
      case "setIndeterminate": element.indeterminate = Boolean(command.indeterminate); break;
      case "setSelected": element.selected = Boolean(command.selected); break;
      case "setAttribute":
        if (command.namespace_uri) {{ element.setAttributeNS(command.namespace_uri, command.name, command.value); }}
        else element.setAttribute(command.name, command.value);
        break;
      case "removeAttribute":
        if (command.namespace_uri) {{ element.removeAttributeNS(command.namespace_uri, command.name); }}
        else element.removeAttribute(command.name);
        break;
      case "setTextContent": element.textContent = String(command.value); break;
      case "setInnerHtml": element.innerHTML = String(command.value); break;
      case "removeNode": element.remove(); break;
      case "setCustomValidity": element.setCustomValidity(command.message); break;
      case "setFormAssociatedDisabledState":
        pushCurrentDocumentCommand({{
          kind: "setFormAssociatedDisabledState",
          node_index: nodeIndex,
          disabled: Boolean(command.disabled),
        }});
        break;
      case "checkValidity": element.checkValidity(); break;
      case "reportValidity": element.reportValidity(); break;
      case "activateFormReset":
        if (element.tagName !== "FORM") throw new TypeError("reset activation requires a form");
        element.reset();
        break;
      case "resetForm":
        pushCurrentDocumentCommand({{
          kind: "resetForm",
          node_index: nodeIndex,
          perform_reset: Boolean(command.perform_reset),
        }});
        break;
      default: throw new TypeError("native frame command is unsupported");
    }}
    return true;
  }};
  globalThis.__glassQueueNativeCommands = (incoming) => {{
    if (!Array.isArray(incoming)) throw new TypeError("native frame command batch must be an array");
    for (const command of incoming) {{
      if (!command || typeof command !== "object") throw new TypeError("native frame command is invalid");
      if (String(command.kind) === "frameScript" || String(command.kind) === "frameScriptBatch") {{
        throw new TypeError("nested native frame commands are not allowed");
      }}
      pushCommand(command);
    }}
    return true;
  }};
  if (!reuseExistingHostTree) {{
  for (const element of elements) {{
    if (["IFRAME", "FRAME"].includes(element.tagName)
        && !Object.prototype.hasOwnProperty.call(element, "contentWindow")) {{
      Object.defineProperty(element, "contentWindow", {{
        enumerable: false,
        configurable: false,
        get() {{
          const binding = frameBindingForNode(element.nodeIndex);
          return binding
            ? makeFrameWindow(binding, element, globalThis, relationshipTop || globalThis)
            : null;
        }},
      }});
      Object.defineProperty(element, "contentDocument", {{
        enumerable: false,
        configurable: false,
        get() {{
          const binding = frameBindingForNode(element.nodeIndex);
          return binding && binding.sameOrigin
            ? makeFrameDocument(binding, element, globalThis, relationshipTop || globalThis)
            : null;
        }},
      }});
    }}
    if (element.tagName === "SELECT" && !Object.prototype.hasOwnProperty.call(element, "options")) {{
      Object.defineProperty(element, "options", {{
        enumerable: false,
        configurable: false,
        get() {{
          const current = globalThis.__glassHostElements;
          if (!(current instanceof Map)) return [];
          return asHtmlCollection(Array.from(current.values()).filter(option =>
            option.tagName === "OPTION" && option.parentIndex === element.nodeIndex
          ));
        }},
      }});
      Object.defineProperty(element, "selectedOptions", {{
        enumerable: false,
        configurable: false,
        get() {{ return asHtmlCollection(element.options.filter(option => option.selected)); }},
      }});
    }}
  }}
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("tree_accessors_ready");
