  const allocateTemporaryNodeIndex = () => {{
    const value = Number(globalThis.__glassNextTemporaryNodeIndex);
    if (value < 4294963200) throw new RangeError("native temporary node limit exceeded");
    globalThis.__glassNextTemporaryNodeIndex = value - 1;
    return value;
  }};
  const attributeNodeKey = (namespace, name) =>
    String(namespace || "") + "\u0000" + String(name);
  const normalizeAttributeNodeName = (name) => {{
    const value = String(name);
    if (!/^[A-Za-z_:][A-Za-z0-9:._-]*$/.test(value)) {{
      const constructor = globalThis.DOMException;
      if (typeof constructor === "function") throw new constructor("The attribute name is invalid", "InvalidCharacterError");
      throw new TypeError("The attribute name is invalid");
    }}
    return value.toLowerCase();
  }};
  const makeAttributeNode = (name, initialValue = "", ownerDocumentResolver = () => null, namespace = null, preserveName = false) => {{
    const namespaceURI = normalizeAttributeNamespace(namespace);
    const normalized = preserveName ? String(name) : normalizeAttributeNodeName(name);
    const separator = namespaceURI === null ? -1 : normalized.indexOf(":");
    let attributeValue = String(initialValue);
    if (attributeValue.length > {storage_value_limit}) throw new RangeError("native attribute value exceeds its limit");
    let ownerElement = null;
    const attribute = {{
      nodeIndex: allocateTemporaryNodeIndex(),
      parentIndex: null,
      nodeType: 2,
      nodeName: normalized,
      name: normalized,
      localName: separator < 0 ? normalized : normalized.slice(separator + 1),
      prefix: separator < 0 ? null : normalized.slice(0, separator),
      namespaceURI,
      specified: true,
      __glassAttribute: true,
      __glassChildren: [],
    }};
    Object.defineProperty(attribute, "ownerElement", {{
      enumerable: true,
      configurable: false,
      get() {{ return ownerElement; }},
    }});
    Object.defineProperty(attribute, "ownerDocument", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (attribute.__glassAdoptedOwnerDocument) return attribute.__glassAdoptedOwnerDocument;
        if (ownerElement && ownerElement.ownerDocument) return ownerElement.ownerDocument;
        return ownerDocumentResolver() || null;
      }},
    }});
    Object.defineProperty(attribute, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{ return null; }},
    }});
    Object.defineProperty(attribute, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{ return null; }},
    }});
    Object.defineProperties(attribute, {{
      value: {{
        enumerable: true,
        configurable: false,
        get() {{ return attributeValue; }},
        set(next) {{
          const value = String(next);
          if (value.length > {storage_value_limit}) throw new RangeError("native attribute value exceeds its limit");
          if (ownerElement) {{
            if (namespaceURI === null) ownerElement.setAttribute(normalized, value);
            else ownerElement.setAttributeNS(namespaceURI, normalized, value);
          }}
          else attributeValue = value;
        }},
      }},
      nodeValue: {{
        enumerable: true,
        configurable: false,
        get() {{ return attributeValue; }},
        set(next) {{ attribute.value = next; }},
      }},
      textContent: {{
        enumerable: true,
        configurable: false,
        get() {{ return attributeValue; }},
        set(next) {{ attribute.value = next; }},
      }},
    }});
    Object.defineProperty(attribute, "__glassSetOwner", {{
      enumerable: false,
      configurable: false,
      value(nextOwner) {{ ownerElement = nextOwner || null; }},
    }});
    Object.defineProperty(attribute, "__glassRefreshValue", {{
      enumerable: false,
      configurable: false,
      value(nextValue) {{
        const value = String(nextValue);
        attributeValue = value.length > {storage_value_limit}
          ? value.slice(0, {storage_value_limit})
          : value;
      }},
    }});
    defineTreeAccessors(attribute);
    const constructor = globalThis.Attr;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(attribute, constructor.prototype); }} catch (_error) {{}}
    }}
    return attribute;
  }};
  const attributeNodeException = (message, name) => {{
    const constructor = globalThis.DOMException;
    if (typeof constructor === "function") return new constructor(message, name);
    const error = new Error(message);
    error.name = name;
    return error;
  }};
  const installAttributeNodeSurface = (element, ownerDocumentResolver) => {{
    if (Object.prototype.hasOwnProperty.call(element, "__glassAttributeNodes")) return;
    const attributeNodes = new Map();
    Object.defineProperty(element, "__glassAttributeNodes", {{
      enumerable: false,
      configurable: false,
      value: attributeNodes,
    }});
    const originalSetAttribute = typeof element.setAttribute === "function"
      ? element.setAttribute.bind(element)
      : null;
    const originalRemoveAttribute = typeof element.removeAttribute === "function"
      ? element.removeAttribute.bind(element)
      : null;
    const syncAttributeNodes = () => {{
      for (const [key, attribute] of attributeNodes) {{
        const namespaceURI = attribute.namespaceURI || null;
        const value = namespaceURI === null
          ? element.getAttribute(attribute.name)
          : element.getAttributeNS(namespaceURI, attribute.localName);
        if (value === null) {{
          attribute.__glassSetOwner(null);
          attributeNodes.delete(key);
        }} else {{
          attribute.__glassRefreshValue(value);
          attribute.__glassSetOwner(element);
        }}
      }}
    }};
    Object.defineProperty(element, "__glassSyncAttributeNodes", {{
      enumerable: false,
      configurable: false,
      value: syncAttributeNodes,
    }});
    if (originalSetAttribute) {{
      element.setAttribute = (name, value) => {{
        originalSetAttribute(name, value);
        const keyName = element.namespaceURI === HTML_NAMESPACE ? String(name).toLowerCase() : String(name);
        const key = attributeNodeKey(null, keyName);
        const attribute = attributeNodes.get(key);
        if (attribute) {{
          attribute.__glassRefreshValue(element.getAttribute(attribute.name));
          attribute.__glassSetOwner(element);
        }}
        syncAttributeNodes();
      }};
    }}
    if (originalRemoveAttribute) {{
      element.removeAttribute = (name) => {{
        originalRemoveAttribute(name);
        const keyName = element.namespaceURI === HTML_NAMESPACE ? String(name).toLowerCase() : String(name);
        const key = attributeNodeKey(null, keyName);
        const attribute = attributeNodes.get(key);
        if (attribute) {{
          attribute.__glassSetOwner(null);
          attributeNodes.delete(key);
        }}
        syncAttributeNodes();
      }};
    }}
    const getAttributeNode = (name) => {{
      const key = element.namespaceURI === HTML_NAMESPACE
        ? normalizeAttributeNodeName(name)
        : String(name);
      if (element.getAttribute(key) === null) return null;
      const namespaceURI = typeof element.__glassAttributeNamespace === "function"
        ? element.__glassAttributeNamespace(key)
        : null;
      if (namespaceURI !== null) return getAttributeNodeNS(namespaceURI, storedAttributeLocalName(key, namespaceURI));
      const nodeKey = attributeNodeKey(null, key);
      const existing = attributeNodes.get(nodeKey);
      if (existing) {{
        existing.__glassRefreshValue(element.getAttribute(key));
        existing.__glassSetOwner(element);
        return existing;
      }}
      const attribute = makeAttributeNode(key, element.getAttribute(key), ownerDocumentResolver, null, true);
      attribute.__glassSetOwner(element);
      attributeNodes.set(nodeKey, attribute);
      return attribute;
    }};
    const setAttributeNode = (attribute) => {{
      if (!attribute || attribute.__glassAttribute !== true) throw new TypeError("setAttributeNode requires an Attr");
      const ownerDocument = ownerDocumentResolver() || null;
      if (attribute.ownerDocument !== ownerDocument) throw attributeNodeException("The attribute belongs to another document", "WrongDocumentError");
      if (attribute.ownerElement && attribute.ownerElement !== element) throw attributeNodeException("The attribute is already in use", "InUseAttributeError");
      const key = normalizeAttributeNodeName(attribute.name);
      const namespaceURI = attribute.namespaceURI || null;
      const old = namespaceURI === null
        ? getAttributeNode(key)
        : getAttributeNodeNS(namespaceURI, attribute.localName);
      if (old === attribute) return old;
      if (old) {{
        old.__glassSetOwner(null);
        attributeNodes.delete(attributeNodeKey(old.namespaceURI, old.name));
      }}
      if (namespaceURI === null) element.setAttribute(key, attribute.value);
      else element.setAttributeNS(namespaceURI, key, attribute.value);
      attributeNodes.set(attributeNodeKey(namespaceURI, key), attribute);
      attribute.__glassSetOwner(element);
      return old;
    }};
    const removeAttributeNode = (attribute) => {{
      if (!attribute || attribute.__glassAttribute !== true) throw new TypeError("removeAttributeNode requires an Attr");
      if (attribute.ownerElement !== element) throw attributeNodeException("The attribute was not found", "NotFoundError");
      const namespaceURI = attribute.namespaceURI || null;
      if (namespaceURI === null) element.removeAttribute(attribute.name);
      else element.removeAttributeNS(namespaceURI, attribute.localName);
      return attribute;
    }};
    const getAttributeNodeNS = (namespace, name) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const localName = String(name);
      const candidate = element.getAttributeNames().find((attributeName) => {{
        const candidateNamespace = typeof element.__glassAttributeNamespace === "function"
          ? element.__glassAttributeNamespace(attributeName)
          : null;
        return candidateNamespace === namespaceURI
          && storedAttributeLocalName(attributeName, candidateNamespace) === localName;
      }});
      if (candidate === undefined) return null;
      if (namespaceURI === null) return getAttributeNode(candidate);
      const nodeKey = attributeNodeKey(namespaceURI, candidate);
      const existing = attributeNodes.get(nodeKey);
      const value = element.getAttributeNS(namespaceURI, localName);
      if (existing) {{
        existing.__glassRefreshValue(value);
        existing.__glassSetOwner(element);
        return existing;
      }}
      const attribute = makeAttributeNode(candidate, value, ownerDocumentResolver, namespaceURI, true);
      attribute.__glassSetOwner(element);
      attributeNodes.set(nodeKey, attribute);
      return attribute;
    }};
    element.getAttributeNode = getAttributeNode;
    element.getAttributeNodeNS = getAttributeNodeNS;
    element.setAttributeNode = setAttributeNode;
    element.setAttributeNodeNS = (namespace, attribute) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      if ((attribute && (attribute.namespaceURI || null)) !== namespaceURI) {{
        throw attributeNodeException("The attribute namespace does not match", "NamespaceError");
      }}
      return setAttributeNode(attribute);
    }};
    element.removeAttributeNode = removeAttributeNode;
    const namedNodeMap = {{
      get length() {{ return element.getAttributeNames().length; }},
      item(index) {{
        const names = element.getAttributeNames();
        const name = names[Number(index)];
        return name === undefined ? null : getAttributeNode(name);
      }},
      getNamedItem(name) {{ return getAttributeNode(name); }},
      getNamedItemNS(namespace, name) {{ return getAttributeNodeNS(namespace, name); }},
      setNamedItem(attribute) {{ return setAttributeNode(attribute); }},
      setNamedItemNS(namespace, attribute) {{ return element.setAttributeNodeNS(namespace, attribute); }},
      removeNamedItem(name) {{
        const attribute = getAttributeNode(name);
        if (!attribute) throw attributeNodeException("The attribute was not found", "NotFoundError");
        return removeAttributeNode(attribute);
      }},
      removeNamedItemNS(namespace, name) {{
        const attribute = getAttributeNodeNS(namespace, name);
        if (!attribute) throw attributeNodeException("The attribute was not found", "NotFoundError");
        return removeAttributeNode(attribute);
      }},
    }};
    Object.defineProperty(namedNodeMap, Symbol.iterator, {{
      configurable: false,
      value() {{
        return Array.from({{ length: namedNodeMap.length }}, (_value, index) => namedNodeMap.item(index))[Symbol.iterator]();
      }},
    }});
    const namedNodeMapView = new Proxy(namedNodeMap, {{
      get(target, property, receiver) {{
        if (typeof property === "string" && /^\d+$/.test(property)) return target.item(Number(property));
        return Reflect.get(target, property, receiver);
      }},
      has(target, property) {{
        if (typeof property === "string" && /^\d+$/.test(property)) return Number(property) < target.length;
        return Reflect.has(target, property);
      }},
    }});
    const namedNodeMapConstructor = globalThis.NamedNodeMap;
    const syncNamedNodeMapPrototype = () => {{
      const constructor = globalThis.NamedNodeMap || namedNodeMapConstructor;
      if (typeof constructor === "function" && constructor.prototype
          && Object.getPrototypeOf(namedNodeMap) !== constructor.prototype) {{
        try {{ Object.setPrototypeOf(namedNodeMap, constructor.prototype); }} catch (_error) {{}}
      }}
    }};
    Object.defineProperty(element, "attributes", {{
      enumerable: true,
      configurable: false,
      get() {{
        syncNamedNodeMapPrototype();
        syncAttributeNodes();
        return namedNodeMapView;
      }},
    }});
  }};
