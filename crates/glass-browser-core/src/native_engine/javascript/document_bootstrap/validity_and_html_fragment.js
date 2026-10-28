  const validityFlags = (entry) => {{
    const source = entry.validity || {{}};
    const customError = String(entry.customValidity || "").length > 0;
    const validity = {{
      badInput: Boolean(source.badInput),
      customError,
      patternMismatch: Boolean(source.patternMismatch),
      rangeOverflow: Boolean(source.rangeOverflow),
      rangeUnderflow: Boolean(source.rangeUnderflow),
      stepMismatch: Boolean(source.stepMismatch),
      tooLong: Boolean(source.tooLong),
      tooShort: Boolean(source.tooShort),
      typeMismatch: Boolean(source.typeMismatch),
      valueMissing: Boolean(source.valueMissing),
      valid: false,
    }};
    validity.valid = String(entry.tagName).toUpperCase() === "FORM"
      ? Boolean(source.valid)
      : !validity.badInput && !validity.customError
        && !validity.patternMismatch && !validity.rangeOverflow
        && !validity.rangeUnderflow && !validity.stepMismatch
        && !validity.tooLong && !validity.tooShort
        && !validity.typeMismatch && !validity.valueMissing;
    return validity;
  }};
  const validationMessageFor = (entry, validity) => {{
    const custom = String(entry.customValidity || "");
    if (custom.length > 0) return custom;
    if (validity.valueMissing) return "Please fill out this field.";
    if (validity.typeMismatch || validity.badInput) return "Please enter a valid value.";
    if (validity.tooShort) return "Value is too short.";
    if (validity.tooLong) return "Value is too long.";
    if (validity.rangeUnderflow) return "Value is below the minimum.";
    if (validity.rangeOverflow) return "Value is above the maximum.";
    if (validity.stepMismatch) return "Value does not match the required step.";
    if (validity.patternMismatch) return "Please match the requested format.";
    return "";
  }};
  const escapeHtmlText = (value) => String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
  const htmlRawTextElements = new Set([
    "script", "style", "xmp", "iframe", "noembed", "noframes", "noscript",
  ]);
  const htmlLiteralTextSerializationElements = new Set([
    ...htmlRawTextElements, "plaintext",
  ]);
  const serializeHtmlTextNode = (node, value) => {{
    const parent = node.__glassParent || null;
    const name = String(parent && parent.localName || "").toLowerCase();
    return parent && parent.namespaceURI === HTML_NAMESPACE
        && htmlLiteralTextSerializationElements.has(name)
      ? String(value)
      : escapeHtmlText(value);
  }};
  const textFromHtml = (value) => String(value)
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;/gi, "\u00a0")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&quot;/gi, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&amp;/gi, "&");
  const decodeHtmlEntities = (value) => String(value)
    .replace(/&nbsp;/gi, "\u00a0")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&quot;/gi, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&#x([0-9a-f]+);/gi, (match, digits) => {{
      const codePoint = Number.parseInt(digits, 16);
      if (codePoint === 0) return "\ufffd";
      return Number.isFinite(codePoint) && codePoint <= 0x10ffff
        && !(codePoint >= 0xd800 && codePoint <= 0xdfff)
        ? String.fromCodePoint(codePoint)
        : match;
    }})
    .replace(/&#([0-9]+);/g, (match, digits) => {{
      const codePoint = Number.parseInt(digits, 10);
      if (codePoint === 0) return "\ufffd";
      return Number.isFinite(codePoint) && codePoint <= 0x10ffff
        && !(codePoint >= 0xd800 && codePoint <= 0xdfff)
        ? String.fromCodePoint(codePoint)
        : match;
    }})
    .replace(/&amp;/gi, "&");
  const populateDetachedFragment = (
    fragment,
    markup,
    createElement,
    createText,
    createComment,
    contextElement = fragment,
    suppressMutations = false,
  ) => {{
    // Parser output is already a complete, detached tree. Link it into the
    // JavaScript mirror directly instead of calling the public appendChild
    // methods: some native element implementations have additional insertion
    // effects (and may enqueue host commands) even while host-command
    // suppression is active. The later SetInnerHtml + identity-bind pair is
    // the single authoritative native mutation for this tree.
    const appendParsedChild = (parent, child) => {{
      if (!parent || !Array.isArray(parent.__glassChildren)
          || !child || typeof child !== "object") {{
        throw new Error("native HTML fragment parser could not attach a node");
      }}
      const previousSibling = parent.__glassChildren.length > 0
        ? parent.__glassChildren[parent.__glassChildren.length - 1]
        : null;
      parent.__glassChildren.push(child);
      child.__glassParent = parent;
      child.parentIndex = parent.nodeIndex !== null && parent.nodeIndex !== undefined
          && Number.isSafeInteger(Number(parent.nodeIndex))
        ? Number(parent.nodeIndex)
        : null;
      if (typeof parent.__glassSyncContent === "function") parent.__glassSyncContent();
      if (!suppressMutations) {{
        queueMutation({{
          type: "childList",
          target: parent,
          addedNodes: [child],
          removedNodes: [],
          previousSibling,
          nextSibling: null,
        }});
      }}
    }};
    const source = String(markup);
    if (source.length > storageValueLimit) {{
      throw new RangeError("native HTML fragment exceeds its source limit");
    }}
    const nativeParser = globalThis.__glassParseHtmlFragmentHost;
    if (typeof nativeParser !== "function") {{
      throw new Error("native HTML fragment parser is unavailable");
    }}
    const isElement = contextElement && Number(contextElement.nodeType) === 1;
    const contextName = isElement ? String(contextElement.localName || "") : "div";
    const contextNamespace = isElement
      ? String(contextElement.namespaceURI || HTML_NAMESPACE)
      : HTML_NAMESPACE;
    const contextAttributes = isElement
      ? Array.from(contextElement.attributes || [], (attribute) => ({{
          name: String(attribute.name),
          value: String(attribute.value),
          namespace: attribute.namespaceURI ? String(attribute.namespaceURI) : null,
        }}))
      : [];
    let contextDepth = 0;
    for (let current = isElement ? contextElement : null;
        current && Number(current.nodeType) === 1;
        current = current.parentElement || null) {{
      contextDepth += 1;
      if (contextDepth > nativeXmlMaxDepth) {{
        throw new RangeError("native HTML fragment context exceeds its depth limit");
      }}
    }}
    let response;
    try {{
      response = JSON.parse(nativeParser(
        source,
        contextNamespace,
        contextName,
        JSON.stringify(contextAttributes),
        contextDepth,
      ));
    }} catch (_error) {{
      throw new Error("native HTML fragment parser failed");
    }}
    if (!response || response.ok !== true || !response.fragment
        || !Array.isArray(response.fragment.nodes)) {{
      const kind = response && response.error;
      if (kind === "byteLimit") throw new RangeError("native HTML fragment exceeds its source limit");
      if (kind === "nodeLimit") throw new RangeError("native HTML fragment exceeds its node limit");
      if (kind === "depthLimit") throw new RangeError("native HTML fragment exceeds its depth limit");
      if (kind === "invalidContext") throw new TypeError("native HTML fragment context is invalid");
      throw new Error("native HTML fragment parser failed");
    }}
    const nodes = response.fragment.nodes;
    const root = nodes[0];
    if (!root || !root.kind || root.kind.type !== "documentFragment"
        || !Array.isArray(root.children)) {{
      throw new Error("native HTML fragment parser returned an invalid tree");
    }}
    const buildNode = (index) => {{
      if (!Number.isInteger(index) || index <= 0 || index >= nodes.length) {{
        throw new Error("native HTML fragment parser returned an invalid node reference");
      }}
      const record = nodes[index];
      const kind = record && record.kind;
      if (!kind || !Array.isArray(record.children)) {{
        throw new Error("native HTML fragment parser returned an invalid node");
      }}
      if (kind.type === "text") return createText(String(kind.data));
      if (kind.type === "comment") return createComment(String(kind.data));
      if (kind.type !== "element" || !kind.data || !Array.isArray(kind.data.attributes)) {{
        throw new Error("native HTML fragment parser returned an unsupported node");
      }}
      const element = createElement(String(kind.data.name), String(kind.data.namespace));
      if (String(kind.data.namespace) === HTML_NAMESPACE) {{
        const isAttribute = kind.data.attributes.find((attribute) =>
          attribute && !attribute.namespace && attribute.name === "is");
        if (isAttribute && Object.prototype.hasOwnProperty.call(element, "__glassCustomElementIs")) {{
          element.__glassCustomElementIs = String(isAttribute.value);
        }}
      }}
      for (const attribute of kind.data.attributes) {{
        if (!attribute || typeof attribute.name !== "string"
            || typeof attribute.value !== "string") {{
          throw new Error("native HTML fragment parser returned an invalid attribute");
        }}
        if (typeof element.__glassSetParsedAttribute === "function") {{
          element.__glassSetParsedAttribute(
            attribute.name,
            attribute.value,
            attribute.namespace || null,
          );
        }} else if (attribute.namespace) {{
          element.setAttributeNS(
            String(attribute.namespace),
            attribute.name,
            attribute.value,
          );
        }} else {{
          element.setAttribute(attribute.name, attribute.value);
        }}
      }}
      const childIndices = record.children.slice();
      let templateContentChildren = null;
      if (record.templateContents !== null && record.templateContents !== undefined) {{
        const templateContents = nodes[record.templateContents];
        if (!templateContents || !templateContents.kind
            || templateContents.kind.type !== "documentFragment"
            || !Array.isArray(templateContents.children)) {{
          throw new Error("native HTML fragment parser returned invalid template contents");
        }}
        templateContentChildren = templateContents.children;
      }}
      for (const childIndex of childIndices) appendParsedChild(element, buildNode(childIndex));
      if (templateContentChildren) {{
        for (const childIndex of templateContentChildren) {{
          appendParsedChild(element.content, buildNode(childIndex));
        }}
      }}
      return element;
    }};
    for (const childIndex of root.children) appendParsedChild(fragment, buildNode(childIndex));
  }};

  const installReflectedAttributeProperties = (element) => {{
    Object.defineProperty(element, "id", {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute("id");
        return value === null ? "" : value;
      }},
      set(next) {{ element.setAttribute("id", String(next)); }},
    }});
    Object.defineProperty(element, "className", {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute("class");
        return value === null ? "" : value;
      }},
      set(next) {{ element.setAttribute("class", String(next)); }},
    }});
  }};
  const parsedNativeTabIndexAttribute = (value) => {{
    if (value === null || value === undefined) return null;
    const source = String(value);
    if (!/^[\t\n\f\r ]*[+-]?[0-9]+[\t\n\f\r ]*$/.test(source)) return null;
    const parsed = Number(source);
    return Number.isInteger(parsed) && parsed >= -2147483648 && parsed <= 2147483647
      ? parsed
      : null;
  }};
  const defaultNativeTabIndex = (element) => {{
    const tagName = String(element.tagName || "").toUpperCase();
    if (["A", "AREA", "BUTTON", "FRAME", "IFRAME", "INPUT", "OBJECT", "SELECT", "TEXTAREA"].includes(tagName)) return 0;
    if (tagName === "SUMMARY") {{
      const parent = element.parentElement;
      if (parent && parent.tagName === "DETAILS") {{
        const firstSummary = Array.isArray(parent.__glassChildren)
          ? parent.__glassChildren.find((child) => Number(child.nodeType) === 1 && child.tagName === "SUMMARY")
          : null;
        if (firstSummary === element) return 0;
      }}
    }}
    return -1;
  }};
  const installTabIndexProperty = (element) => {{
    Object.defineProperty(element, "tabIndex", {{
      enumerable: true,
      configurable: false,
      get() {{
        const parsed = parsedNativeTabIndexAttribute(element.getAttribute("tabindex"));
        return parsed === null ? defaultNativeTabIndex(element) : parsed;
      }},
      set(next) {{
        const converted = (+next) | 0;
        element.setAttribute("tabindex", String(converted));
      }},
    }});
  }};
  const installBooleanAttributeProperty = (element, property, attribute, read) => {{
    Object.defineProperty(element, property, {{
      enumerable: true,
      configurable: false,
      get() {{ return read(); }},
      set(next) {{
        if (Boolean(next)) element.setAttribute(attribute, "");
        else element.removeAttribute(attribute);
      }},
    }});
  }};
  const installStringAttributeProperty = (element, property, attribute) => {{
    Object.defineProperty(element, property, {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute(attribute);
        return value === null ? "" : value;
      }},
      set(next) {{ element.setAttribute(attribute, String(next)); }},
    }});
  }};
  const referrerPolicyAttributeValues = new Set([
    "", "no-referrer", "no-referrer-when-downgrade", "same-origin",
    "origin", "strict-origin", "origin-when-cross-origin",
    "strict-origin-when-cross-origin", "unsafe-url",
  ]);
  const asciiLowercaseAttributeValue = (value) => String(value).replace(/[A-Z]/g, (character) =>
    String.fromCharCode(character.charCodeAt(0) + 32));
  const installLimitedKnownValuesStringAttributeProperty = (element, property, attribute, values) => {{
    Object.defineProperty(element, property, {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute(attribute);
        return value !== null && values.has(asciiLowercaseAttributeValue(value)) ? value : "";
      }},
      set(next) {{ element.setAttribute(attribute, String(next)); }},
    }});
  }};
  const nativeMediaTypeSupported = (value) => {{
    const mediaType = String(value === undefined || value === null ? "" : value)
      .split(";", 1)[0]
      .trim()
      .toLowerCase();
    return mediaType === ""
      || [
        "audio/mpeg", "audio/mp3", "audio/ogg", "application/ogg",
        "audio/wav", "audio/wave", "audio/x-wav", "audio/webm", "audio/mp4",
        "video/mp4", "video/ogg", "video/webm",
        "application/vnd.apple.mpegurl", "application/x-mpegurl",
      ].includes(mediaType);
  }};
  const effectiveDocumentBaseUrl = () => {{
    const fallback = String(host.base_fallback_url || host.url || "");
    const ownerDocument = globalThis.document;
    const pending = Array.isArray(ownerDocument && ownerDocument.__glassChildren)
      ? ownerDocument.__glassChildren.slice().reverse()
      : [];
    while (pending.length > 0) {{
      const element = pending.pop();
      if (!element || element.nodeType !== 1) continue;
      if (element.namespaceURI === HTML_NAMESPACE
          && element.tagName === "BASE"
          && element.hasAttribute("href")) {{
        try {{ return new URLNative(element.getAttribute("href") || "", host.url || fallback).href; }}
        catch (_error) {{ return fallback; }}
      }}
      const children = Array.isArray(element.__glassChildren) ? element.__glassChildren : [];
      for (let index = children.length - 1; index >= 0; index -= 1)
        pending.push(children[index]);
    }}
    return fallback;
  }};
  const installUrlAttributeProperty = (element, property, attribute, baseUrl, onSet = null) => {{
    Object.defineProperty(element, property, {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute(attribute);
        if (value === null || value === "") return "";
        try {{ return new URLNative(value, baseUrl).href; }} catch (_error) {{ return value; }}
      }},
      set(next) {{
        const value = String(next);
        element.setAttribute(attribute, value);
        if (typeof onSet === "function") onSet(value);
      }},
    }});
  }};
  const nativeElementSharedPrototype = Object.create(Object.prototype);
  const sharedCommonAttributePrototypes = new WeakSet();
  const sharedSpecialAttributeProperties = new WeakMap();
  const defineSharedSpecialAttribute = (prototype, property, descriptor) => {{
    let properties = sharedSpecialAttributeProperties.get(prototype);
    if (!properties) {{
      properties = new Set();
      sharedSpecialAttributeProperties.set(prototype, properties);
    }}
    if (properties.has(property)) return;
    Object.defineProperty(prototype, property, descriptor);
    properties.add(property);
  }};
  const installSharedStringAttribute = (prototype, property, attribute) =>
    defineSharedSpecialAttribute(prototype, property, {{
      enumerable: true,
      configurable: true,
      get() {{ const value = this.getAttribute(attribute); return value === null ? "" : value; }},
      set(next) {{ this.setAttribute(attribute, String(next)); }},
    }});
  const installSharedUrlAttribute = (prototype, property, attribute) =>
    defineSharedSpecialAttribute(prototype, property, {{
      enumerable: true,
      configurable: true,
      get() {{
        const value = this.getAttribute(attribute);
        if (value === null || value === "") return "";
        try {{ return new URLNative(value, effectiveDocumentBaseUrl()).href; }} catch (_error) {{ return value; }}
      }},
      set(next) {{ this.setAttribute(attribute, String(next)); }},
    }});
  const installSharedReferrerPolicy = (prototype) =>
    defineSharedSpecialAttribute(prototype, "referrerPolicy", {{
      enumerable: true,
      configurable: true,
      get() {{
        const value = this.getAttribute("referrerpolicy");
        return value !== null && referrerPolicyAttributeValues.has(asciiLowercaseAttributeValue(value)) ? value : "";
      }},
      set(next) {{ this.setAttribute("referrerpolicy", String(next)); }},
    }});
  // ElementNative's interface prototypes copy the shared descriptors during
  // bootstrap. Install this reflected attribute before those prototypes are
  // constructed so created SCRIPT/LINK/IMG nodes inherit the setter.
  installSharedReferrerPolicy(nativeElementSharedPrototype);
  const installSharedCommonAttributeProperties = (prototype) => {{
    if (sharedCommonAttributePrototypes.has(prototype)) return;
    sharedCommonAttributePrototypes.add(prototype);
    const reflectedString = (attribute) => ({{
      enumerable: true,
      configurable: true,
      get() {{ const value = this.getAttribute(attribute); return value === null ? "" : value; }},
      set(next) {{ this.setAttribute(attribute, String(next)); }},
    }});
    Object.defineProperties(prototype, {{
      id: reflectedString("id"),
      className: reflectedString("class"),
      tabIndex: {{
        enumerable: true,
        configurable: true,
        get() {{
          const parsed = parsedNativeTabIndexAttribute(this.getAttribute("tabindex"));
          return parsed === null ? defaultNativeTabIndex(this) : parsed;
        }},
        set(next) {{ this.setAttribute("tabindex", String((+next) | 0)); }},
      }},
    }});
    for (const [property, attribute] of [
      ["disabled", "disabled"], ["hidden", "hidden"], ["multiple", "multiple"],
      ["inert", "inert"], ["required", "required"], ["readOnly", "readonly"],
      ["autofocus", "autofocus"], ["open", "open"], ["controls", "controls"],
      ["loop", "loop"], ["muted", "muted"], ["autoplay", "autoplay"],
      ["reversed", "reversed"], ["formNoValidate", "formnovalidate"], ["noValidate", "novalidate"],
    ]) Object.defineProperty(prototype, property, {{
      enumerable: true,
      configurable: true,
      get() {{ return this.hasAttribute(attribute); }},
      set(next) {{
        if (Boolean(next)) this.setAttribute(attribute, "");
        else this.removeAttribute(attribute);
      }},
    }});
    for (const [property, attribute] of [
      ["name", "name"], ["title", "title"], ["lang", "lang"], ["dir", "dir"],
      ["slot", "slot"], ["htmlFor", "for"], ["accept", "accept"], ["alt", "alt"],
      ["placeholder", "placeholder"], ["pattern", "pattern"], ["min", "min"],
      ["max", "max"], ["step", "step"], ["method", "method"],
      ["target", "target"], ["rel", "rel"], ["download", "download"],
    ]) Object.defineProperty(prototype, property, reflectedString(attribute));
  }};
  const installCommonAttributeProperties = (element, state = {{}}, baseUrl = host.url) => {{
    const prototype = nativeElementSharedPrototype;
    installSharedCommonAttributeProperties(prototype);
    if (element.namespaceURI === HTML_NAMESPACE && element.tagName === "META")
      installStringAttributeProperty(element, "content", "content");
    if (["A", "AREA", "BASE", "LINK"].includes(element.tagName))
      installSharedUrlAttribute(prototype, "href", "href");
    if (["IMG", "SCRIPT", "IFRAME", "FRAME", "EMBED", "SOURCE", "TRACK", "AUDIO", "VIDEO"].includes(element.tagName)) {{
      installSharedUrlAttribute(prototype, "src", "src");
    }}
    if (element.tagName === "VIDEO") {{
      defineSharedSpecialAttribute(prototype, "poster", {{
        enumerable: true,
        configurable: true,
        get() {{
          const value = this.getAttribute("poster");
          if (value === null || value === "") return "";
          try {{ return new URLNative(value, effectiveDocumentBaseUrl()).href; }} catch (_error) {{ return ""; }}
        }},
        set(next) {{ this.setAttribute("poster", String(next)); }},
      }});
    }}
    if (["LINK", "SCRIPT", "AUDIO", "VIDEO"].includes(element.tagName)) {{
      installSharedStringAttribute(prototype, "integrity", "integrity");
      installSharedStringAttribute(prototype, "crossOrigin", "crossorigin");
    }}
    if (element.tagName === "FORM")
      installSharedUrlAttribute(prototype, "action", "action");
    if (["IMG", "SOURCE"].includes(element.tagName)) {{
      installSharedStringAttribute(prototype, "srcset", "srcset");
      installSharedStringAttribute(prototype, "sizes", "sizes");
    }}
    if (element.tagName === "SOURCE")
      installSharedStringAttribute(prototype, "media", "media");
    if (element.tagName === "IMG") {{
      if (typeof state.imageComplete === "function") {{
        Object.defineProperty(element, "__glassImageState", {{
          enumerable: false,
          configurable: false,
          value: state,
        }});
      }}
      defineSharedSpecialAttribute(prototype, "complete", {{
        enumerable: true, configurable: true,
        get() {{
          const imageState = this.__glassImageState;
          if (imageState && typeof imageState.imageComplete === "function")
            return Boolean(imageState.imageComplete());
          const entry = this.__glassEntry;
          return entry.imageComplete === undefined ? !this.hasAttribute("src") : Boolean(entry.imageComplete);
        }},
      }});
      defineSharedSpecialAttribute(prototype, "naturalWidth", {{
        enumerable: true, configurable: true,
        get() {{
          const imageState = this.__glassImageState;
          return typeof imageState?.imageNaturalWidth === "function"
            ? Number(imageState.imageNaturalWidth()) || 0
            : Number(this.__glassEntry.imageNaturalWidth) || 0;
        }},
      }});
      defineSharedSpecialAttribute(prototype, "naturalHeight", {{
        enumerable: true, configurable: true,
        get() {{
          const imageState = this.__glassImageState;
          return typeof imageState?.imageNaturalHeight === "function"
            ? Number(imageState.imageNaturalHeight()) || 0
            : Number(this.__glassEntry.imageNaturalHeight) || 0;
        }},
      }});
      defineSharedSpecialAttribute(prototype, "currentSrc", {{
        enumerable: true, configurable: true,
        get() {{
          const imageState = this.__glassImageState;
          if (imageState && typeof imageState.imageCurrentSrc === "function") {{
            const selected = String(imageState.imageCurrentSrc() || "");
            const value = selected || this.getAttribute("src");
            if (value === null || value === "") return "";
            try {{ return new URLNative(value, effectiveDocumentBaseUrl()).href; }} catch (_error) {{ return value; }}
          }}
          const entry = this.__glassEntry;
          const selected = String(entry.imageCurrentSrc || "");
          const value = selected || this.getAttribute("src");
          if (value === null || value === "") return "";
          try {{ return new URLNative(value, effectiveDocumentBaseUrl()).href; }} catch (_error) {{ return value; }}
        }},
      }});
    }}
    if (["AUDIO", "VIDEO"].includes(element.tagName)) {{
      const mediaState = () => ({{
        readyState: typeof state.mediaReadyState === "function"
          ? Number(state.mediaReadyState()) || 0
          : 0,
        networkState: typeof state.mediaNetworkState === "function"
          ? Number(state.mediaNetworkState()) || 0
          : (element.getAttribute("src") ? 2 : 0),
        currentSrc: typeof state.mediaCurrentSrc === "function"
          ? String(state.mediaCurrentSrc() || "")
          : "",
        durationMillis: typeof state.mediaDurationMillis === "function"
          ? state.mediaDurationMillis()
          : null,
        videoWidth: typeof state.mediaVideoWidth === "function"
          ? Number(state.mediaVideoWidth()) || 0
          : 0,
        videoHeight: typeof state.mediaVideoHeight === "function"
          ? Number(state.mediaVideoHeight()) || 0
          : 0,
        errorCode: typeof state.mediaError === "function"
          ? state.mediaError()
          : null,
      }});
      const mediaDuration = () => {{
        const millis = mediaState().durationMillis;
        return millis === null || millis === undefined
          ? Number.NaN
          : Math.max(0, Number(millis) / 1000);
      }};
      const mediaSelectedSource = () => {{
        const own = element.getAttribute("src");
        if (own !== null && own !== "") return own;
        const children = Array.isArray(element.__glassChildren) ? element.__glassChildren : [];
        for (const child of children) {{
          if (!child || child.localName !== "source") continue;
          if (!nativeMediaTypeSupported(child.getAttribute("type"))) continue;
          const source = child.getAttribute("src");
          if (source !== null && source !== "") return source;
        }}
        return "";
      }};
      const mediaCurrentSource = () => {{
        const selected = mediaState().currentSrc || mediaSelectedSource();
        if (!selected) return "";
        try {{ return new URLNative(selected, effectiveDocumentBaseUrl()).href; }} catch (_error) {{ return selected; }}
      }};
      let mediaCurrentTime = 0;
      let mediaPaused = true;
      let mediaEnded = false;
      let mediaPlaybackRate = 1;
      let mediaDefaultPlaybackRate = 1;
      let mediaVolume = 1;
      let mediaSeeking = false;
      let mediaPlaybackStartedAt = null;
      let mediaPlaybackStartTime = 0;
      let mediaPlayedRanges = [];
      let mediaPlaybackTimer = null;
      const mediaNow = () => {{
        const now = Number(globalThis.performance && globalThis.performance.now && globalThis.performance.now());
        return Number.isFinite(now) && now >= 0 ? now : 0;
      }};
      const publishNativeVideoTime = () => {{
        if (element.localName !== "video" || typeof state.mediaFrameTime !== "function") return;
        state.mediaFrameTime(mediaCurrentTime * 1000, !mediaPaused, mediaPlaybackRate);
      }};
      const recordMediaPlayedRange = (start, end) => {{
        if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return;
        const ranges = [];
        let nextStart = start;
        let nextEnd = end;
        let inserted = false;
        for (const range of mediaPlayedRanges) {{
          if (range[1] < nextStart) {{
            ranges.push(range);
            continue;
          }}
          if (nextEnd < range[0]) {{
            if (!inserted) {{
              ranges.push([nextStart, nextEnd]);
              inserted = true;
            }}
            ranges.push(range);
            continue;
          }}
          nextStart = Math.min(nextStart, range[0]);
          nextEnd = Math.max(nextEnd, range[1]);
        }}
        if (!inserted) ranges.push([nextStart, nextEnd]);
        mediaPlayedRanges = ranges;
      }};
      const stopMediaTicker = () => {{
        if (mediaPlaybackTimer === null) return;
        if (typeof globalThis.clearInterval === "function") globalThis.clearInterval(mediaPlaybackTimer);
        mediaPlaybackTimer = null;
      }};
      const startMediaTicker = () => {{
        if (mediaPlaybackTimer !== null || typeof globalThis.setInterval !== "function") return;
        mediaPlaybackTimer = globalThis.setInterval(() => syncMediaClock(), 100);
      }};
      const dispatchMediaEvent = (type) => {{
        try {{ dispatchTarget(element, createEvent(type)); }} catch (_error) {{}}
      }};
      const syncMediaClock = () => {{
        if (mediaPaused || mediaPlaybackStartedAt === null || mediaEnded) return;
        const duration = mediaDuration();
        if (!Number.isFinite(duration)) {{
          mediaPaused = true;
          mediaPlaybackStartedAt = null;
          return;
        }}
        const elapsed = Math.max(0, mediaNow() - mediaPlaybackStartedAt)
          * mediaPlaybackRate / 1000;
        const previous = mediaCurrentTime;
        const next = Math.max(0, Math.min(duration, mediaPlaybackStartTime + elapsed));
        if (next > previous) {{
          mediaCurrentTime = next;
          recordMediaPlayedRange(previous, next);
          publishNativeVideoTime();
          dispatchMediaEvent("timeupdate");
        }}
        if (next >= duration && mediaPlaybackRate > 0) {{
          mediaCurrentTime = duration;
          mediaEnded = true;
          mediaPaused = true;
          mediaPlaybackStartedAt = null;
          stopMediaTicker();
          publishNativeVideoTime();
          dispatchMediaEvent("ended");
        }}
      }};
      const resetMediaState = () => {{
        stopMediaTicker();
        mediaCurrentTime = 0;
        mediaPaused = true;
        mediaEnded = false;
        mediaSeeking = false;
        mediaPlaybackStartedAt = null;
        mediaPlaybackStartTime = 0;
        mediaPlayedRanges = [];
        publishNativeVideoTime();
        if (typeof state.mediaReset === "function") state.mediaReset();
      }};
      const mediaTimeRanges = (kind = "buffered") => {{
        syncMediaClock();
        const duration = mediaDuration();
        const ranges = kind === "played"
          ? mediaPlayedRanges.map(range => [range[0], range[1]])
          : (Number.isFinite(duration) && duration >= 0 ? [[0, duration]] : []);
        return {{
          length: ranges.length,
          start(index) {{
            const position = Number(index);
            if (!Number.isInteger(position) || position < 0 || position >= ranges.length) throw new DOMExceptionNative("The time range index is invalid", "IndexSizeError");
            return ranges[position][0];
          }},
          end(index) {{
            const position = Number(index);
            if (!Number.isInteger(position) || position < 0 || position >= ranges.length) throw new DOMExceptionNative("The time range index is invalid", "IndexSizeError");
            return ranges[position][1];
          }},
        }};
      }};
      Object.defineProperties(element, {{
        currentSrc: {{ enumerable: true, configurable: false, get() {{ return mediaCurrentSource(); }} }},
        readyState: {{ enumerable: true, configurable: false, get() {{ return mediaState().readyState; }} }},
        networkState: {{ enumerable: true, configurable: false, get() {{ return mediaState().networkState; }} }},
        duration: {{ enumerable: true, configurable: false, get() {{ return mediaDuration(); }} }},
        ...(element.tagName === "VIDEO" ? {{
          videoWidth: {{ enumerable: true, configurable: false, get() {{ return mediaState().videoWidth; }} }},
          videoHeight: {{ enumerable: true, configurable: false, get() {{ return mediaState().videoHeight; }} }},
        }} : {{}}),
        currentTime: {{
          enumerable: true,
          configurable: false,
          get() {{ syncMediaClock(); return mediaCurrentTime; }},
          set(next) {{
            syncMediaClock();
            const numeric = Number(next);
            if (!Number.isFinite(numeric) || numeric < 0) throw new DOMExceptionNative("The media time is invalid", "InvalidStateError");
            const duration = mediaDuration();
            mediaCurrentTime = Number.isFinite(duration) ? Math.min(numeric, duration) : numeric;
            publishNativeVideoTime();
            mediaEnded = Number.isFinite(duration) && mediaCurrentTime >= duration;
            if (!mediaPaused) {{
              mediaPlaybackStartedAt = mediaNow();
              mediaPlaybackStartTime = mediaCurrentTime;
            }}
            mediaSeeking = true;
            dispatchMediaEvent("seeking");
            mediaSeeking = false;
            dispatchMediaEvent("timeupdate");
            dispatchMediaEvent("seeked");
          }},
        }},
        paused: {{ enumerable: true, configurable: false, get() {{ syncMediaClock(); return mediaPaused; }} }},
        ended: {{ enumerable: true, configurable: false, get() {{ syncMediaClock(); return mediaEnded; }} }},
        seeking: {{ enumerable: true, configurable: false, get() {{ return mediaSeeking; }} }},
        playbackRate: {{
          enumerable: true,
          configurable: false,
          get() {{ return mediaPlaybackRate; }},
          set(next) {{
            syncMediaClock();
            const numeric = Number(next);
            if (!Number.isFinite(numeric) || numeric === 0) throw new DOMExceptionNative("The playback rate is invalid", "NotSupportedError");
            mediaPlaybackRate = numeric;
            if (!mediaPaused) {{
              mediaPlaybackStartedAt = mediaNow();
              mediaPlaybackStartTime = mediaCurrentTime;
            }}
            publishNativeVideoTime();
            dispatchMediaEvent("ratechange");
          }},
        }},
        defaultPlaybackRate: {{
          enumerable: true,
          configurable: false,
          get() {{ return mediaDefaultPlaybackRate; }},
          set(next) {{
            const numeric = Number(next);
            if (!Number.isFinite(numeric) || numeric === 0) throw new DOMExceptionNative("The default playback rate is invalid", "NotSupportedError");
            mediaDefaultPlaybackRate = numeric;
          }},
        }},
        volume: {{
          enumerable: true,
          configurable: false,
          get() {{ return mediaVolume; }},
          set(next) {{
            const numeric = Number(next);
            if (!Number.isFinite(numeric) || numeric < 0 || numeric > 1) throw new DOMExceptionNative("The media volume is invalid", "IndexSizeError");
            if (mediaVolume !== numeric) {{
              mediaVolume = numeric;
              dispatchMediaEvent("volumechange");
            }}
          }},
        }},
        error: {{
          enumerable: true,
          configurable: false,
          get() {{
            const code = mediaState().errorCode;
            return code === null || code === undefined
              ? null
              : {{ code: Number(code), message: "The media resource could not be loaded" }};
          }},
        }},
        buffered: {{ enumerable: true, configurable: false, get() {{ return mediaTimeRanges("buffered"); }} }},
        seekable: {{ enumerable: true, configurable: false, get() {{ return mediaTimeRanges("seekable"); }} }},
        played: {{ enumerable: true, configurable: false, get() {{ return mediaTimeRanges("played"); }} }},
        srcObject: {{
          enumerable: true,
          configurable: false,
          get() {{ return null; }},
          set(_next) {{ throw new DOMExceptionNative("srcObject is unsupported", "NotSupportedError"); }},
        }},
      }});
      element.load = () => {{
        resetMediaState();
        if (typeof state.mediaLoad === "function") state.mediaLoad();
        dispatchMediaEvent("loadstart");
      }};
      element.play = () => {{
        syncMediaClock();
        const state = mediaState();
        if (state.errorCode !== null && state.errorCode !== undefined
          || state.readyState < 1
          || !Number.isFinite(mediaDuration())) {{
          return Promise.reject(new DOMExceptionNative(
            "The native media resource cannot be played",
            "NotSupportedError",
          ));
        }}
        if (mediaEnded || mediaCurrentTime >= mediaDuration()) {{
          mediaCurrentTime = 0;
          mediaEnded = false;
        }}
        if (mediaPaused) {{
          mediaPlaybackStartedAt = mediaNow();
          mediaPlaybackStartTime = mediaCurrentTime;
          mediaPaused = false;
          publishNativeVideoTime();
          dispatchMediaEvent("play");
          dispatchMediaEvent("playing");
          startMediaTicker();
        }}
        return Promise.resolve();
      }};
      element.pause = () => {{
        if (!mediaPaused) {{
          syncMediaClock();
          mediaPaused = true;
          mediaPlaybackStartedAt = null;
          stopMediaTicker();
          publishNativeVideoTime();
          dispatchMediaEvent("pause");
        }}
      }};
      element.fastSeek = (next) => {{ element.currentTime = next; }};
      element.canPlayType = (type) => {{
        const text = String(type || "").trim();
        if (!text || !nativeMediaTypeSupported(text)) return "";
        if (element.localName === "video") {{
          return /^video\/webm(?:\s*;\s*codecs\s*=\s*["']?vp8["']?)?$/i.test(text)
            ? "maybe"
            : "";
        }}
        return /^audio\/(?:wav|wave|x-wav)$/i.test(text.split(";", 1)[0].trim())
          ? "maybe"
          : "";
      }};
      element.__glassMediaReset = resetMediaState;
    }}
    Object.defineProperty(element, "type", {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute("type");
        if (value !== null) return value.toLowerCase();
        if (element.localName === "input") return "text";
        if (element.localName === "button") return "submit";
        return "";
      }},
      set(next) {{ element.setAttribute("type", String(next)); }},
    }});
  }};
  // Resource attributes must be inherited by dynamically created nodes too.
  // The native element constructors copy these shared descriptors later in
  // custom_elements.js, after the initial host snapshot is created. Without
  // eager installation, an image, script, or link created on a page that has
  // no static element of that kind gets an ordinary JS property instead of a
  // reflected URL attribute.
  for (const tagName of ["IMG", "SCRIPT", "LINK"]) {{
    installCommonAttributeProperties({{
      namespaceURI: HTML_NAMESPACE,
      tagName,
    }});
  }}
  if (!Number.isSafeInteger(globalThis.__glassNextTemporaryNodeIndex)) {{
    globalThis.__glassNextTemporaryNodeIndex = 4294967294;
  }}
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("module_validity_html_fragment");
