  const ShadowRootNative = ensureNativeConstructor("ShadowRoot", DocumentFragmentNative);
  const shadowHostByRoot = new WeakMap();
  const shadowModeByRoot = new WeakMap();
  const shadowRootByHost = new WeakMap();
  const restoredShadowRoots = globalThis.__glassPendingShadowRoots;
  if (restoredShadowRoots instanceof Map) {{
    for (const [root, pair] of restoredShadowRoots) {{
      shadowHostByRoot.set(root, pair.host);
      shadowModeByRoot.set(root, pair.mode);
      shadowRootByHost.set(pair.host, root);
      try {{ Object.setPrototypeOf(root, ShadowRootNative.prototype); }} catch (_error) {{}}
    }}
  }}
  try {{ delete globalThis.__glassPendingShadowRoots; }} catch (_error) {{}}
  if (globalThis.__glassHostNodes instanceof Map) {{
    for (const node of globalThis.__glassHostNodes.values()) {{
      if (!node || !shadowHostByRoot.has(node)) continue;
      try {{ Object.setPrototypeOf(node, ShadowRootNative.prototype); }} catch (_error) {{}}
    }}
  }}
  const shadowRootHost = function() {{ return shadowHostByRoot.get(this) || null; }};
  Object.defineProperties(ShadowRootNative.prototype, {{
    mode: {{ configurable: true, enumerable: true, get() {{ return shadowModeByRoot.get(this) || "open"; }} }},
    host: {{ configurable: true, enumerable: true, get: shadowRootHost }},
    querySelector: {{
      configurable: true,
      value(selector) {{ return collectShadowElements(this).find((element) => matches(element, selector, this)) || null; }},
    }},
    querySelectorAll: {{
      configurable: true,
      value(selector) {{ return asNodeList(collectShadowElements(this).filter((element) => matches(element, selector, this))); }},
    }},
    getElementById: {{
      configurable: true,
      value(value) {{
        const wanted = String(value);
        return collectShadowElements(this).find((element) => element.getAttribute("id") === wanted) || null;
      }},
    }},
  }});
  const collectShadowElements = (root) => {{
    const result = [];
    const pending = Array.isArray(root && root.__glassChildren)
      ? root.__glassChildren.slice().reverse()
      : [];
    let visited = 0;
    while (pending.length > 0 && visited < {native_dom_max_nodes}) {{
      const node = pending.pop();
      visited += 1;
      if (!node) continue;
      if (Number(node.nodeType) === 1) result.push(node);
      if (Array.isArray(node.__glassChildren))
        for (let index = node.__glassChildren.length - 1; index >= 0; index -= 1)
          pending.push(node.__glassChildren[index]);
    }}
    return result;
  }};
  Object.defineProperty(ElementNative.prototype, "attachShadow", {{
    configurable: true,
    enumerable: true,
    value(init) {{
      if (!init || (init.mode !== "open" && init.mode !== "closed"))
        throw new TypeError("ShadowRootInit.mode must be open or closed");
      if (shadowRootByHost.has(this))
        throw new DOMExceptionNative("This element already has a shadow root", "NotSupportedError");
      if (!Number.isSafeInteger(Number(this.nodeIndex)))
        throw new DOMExceptionNative("The shadow host is not attached to the native document", "NotSupportedError");
      const root = makeDocumentFragment();
      const nodeIndex = allocateTemporaryNodeIndex();
      root.nodeIndex = nodeIndex;
      root.parentIndex = null;
      root.__glassCreated = true;
      try {{ Object.setPrototypeOf(root, ShadowRootNative.prototype); }} catch (_error) {{}}
      shadowHostByRoot.set(root, this);
      shadowModeByRoot.set(root, init.mode);
      shadowRootByHost.set(this, root);
      mutationCreatedNodes.set(nodeIndex, root);
      scriptNodeObjects.set(nodeIndex, root);
      pushCommand({{
        kind: "createShadowRoot",
        host_index: Number(this.nodeIndex),
        node_index: nodeIndex,
        mode: init.mode,
      }});
      return root;
    }},
  }});
  Object.defineProperty(ElementNative.prototype, "shadowRoot", {{
    configurable: true,
    enumerable: true,
    get() {{
      const root = shadowRootByHost.get(this) || null;
      return root && shadowModeByRoot.get(root) === "open" ? root : null;
    }},
  }});
