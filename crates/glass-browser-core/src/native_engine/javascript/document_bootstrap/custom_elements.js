  const NodeNative = ensureNativeConstructor("Node", null);
  const CharacterDataNative = ensureNativeConstructor("CharacterData", NodeNative);
  const TextNative = ensureNativeConstructor("Text", CharacterDataNative);
  const CommentNative = ensureNativeConstructor("Comment", CharacterDataNative);
  const AttrNative = ensureNativeConstructor("Attr", NodeNative);
  const DocumentTypeNative = ensureNativeConstructor("DocumentType", NodeNative);
  const DocumentNative = ensureNativeConstructor("Document", NodeNative);
  const DocumentFragmentNative = ensureNativeConstructor("DocumentFragment", NodeNative);
  for (const fragment of fragmentNodes) {{
    try {{ Object.setPrototypeOf(fragment, DocumentFragmentNative.prototype); }} catch (_error) {{}}
  }}
  const ElementNative = ensureNativeConstructor("Element", NodeNative);
  const HTMLElementNative = ensureNativeConstructor("HTMLElement", ElementNative);
  const HTMLMediaElementNative = ensureNativeConstructor("HTMLMediaElement", HTMLElementNative);
  const nativeGlobalEventHandlerTypes = [
    "abort", "auxclick", "beforeinput", "beforematch", "beforetoggle", "blur",
    "cancel", "canplay", "canplaythrough", "change", "click", "close", "command",
    "contextlost", "contextmenu", "contextrestored", "copy", "cuechange", "cut",
    "dblclick", "drag", "dragend", "dragenter", "dragleave", "dragover", "dragstart",
    "drop", "durationchange", "emptied", "ended", "error", "focus", "formdata",
    "input", "invalid", "keydown", "keypress", "keyup", "load", "loadeddata",
    "loadedmetadata", "loadstart", "mousedown", "mouseenter", "mouseleave", "mousemove",
    "mouseout", "mouseover", "mouseup", "paste", "pause", "play", "playing",
    "progress", "ratechange", "reset", "resize", "scroll", "scrollend",
    "securitypolicyviolation", "seeked", "seeking", "select", "slotchange", "stalled",
    "submit", "suspend", "timeupdate", "toggle", "volumechange", "waiting",
    "webkitanimationend", "webkitanimationiteration", "webkitanimationstart",
    "webkittransitionend", "wheel",
  ];
  for (const type of nativeGlobalEventHandlerTypes) {{
    if (!Object.prototype.hasOwnProperty.call(HTMLElementNative.prototype, "on" + type))
      installEventHandlerProperty(HTMLElementNative.prototype, type, true);
  }}
  const WindowNative = ensureNativeConstructor("Window", null);
  for (const type of [...nativeGlobalEventHandlerTypes, ...nativeWindowEventHandlerTypes]) {{
    if (!Object.prototype.hasOwnProperty.call(WindowNative.prototype, "on" + type))
      installEventHandlerProperty(WindowNative.prototype, type, true);
  }}
  for (const type of [...nativeGlobalEventHandlerTypes, "readystatechange", "visibilitychange"]) {{
    if (!Object.prototype.hasOwnProperty.call(DocumentNative.prototype, "on" + type))
      installEventHandlerProperty(DocumentNative.prototype, type, true);
  }}
  const LocationNative = ensureNativeConstructor("Location", null);
  const NodeListNative = ensureNativeConstructor("NodeList", null);
  const HtmlCollectionNative = ensureNativeConstructor("HTMLCollection", null);
  const NamedNodeMapNative = ensureNativeConstructor("NamedNodeMap", null);
  for (const [name, value] of [["HAVE_NOTHING", 0], ["HAVE_METADATA", 1], ["HAVE_CURRENT_DATA", 2], ["HAVE_FUTURE_DATA", 3], ["HAVE_ENOUGH_DATA", 4], ["NETWORK_EMPTY", 0], ["NETWORK_IDLE", 1], ["NETWORK_LOADING", 2], ["NETWORK_NO_SOURCE", 3]]) {{
    if (!(name in HTMLMediaElementNative)) Object.defineProperty(HTMLMediaElementNative, name, {{ configurable: false, enumerable: true, value }});
  }}
  globalThis.Attr = AttrNative;
  globalThis.NamedNodeMap = NamedNodeMapNative;
  const characterDataTarget = (target) => {{
    if (!target || ![3, 8].includes(Number(target.nodeType))) throw new TypeError("CharacterData method called on a non-character-data node");
    return target;
  }};
  const characterDataText = (target) => {{
    const text = characterDataTarget(target);
    return String(Number(text.nodeType) === 8 ? text.nodeValue ?? "" : text.__glassTextValue ?? text.nodeValue ?? "");
  }};
  const characterDataOffset = (target, value) => {{
    const numeric = Number(value);
    const offset = Number.isNaN(numeric) ? 0 : Math.trunc(numeric);
    if (!Number.isFinite(numeric) || numeric < 0 || offset > characterDataText(target).length) {{
      throw new DOMExceptionNative("The character data offset is out of range", "IndexSizeError");
    }}
    return offset;
  }};
  const characterDataCount = (value) => {{
    const numeric = Number(value);
    if (Number.isNaN(numeric)) return 0;
    if (numeric < 0 || Number.isNaN(Math.trunc(numeric))) {{
      throw new DOMExceptionNative("The character data count is out of range", "IndexSizeError");
    }}
    return Number.isFinite(numeric) ? Math.max(0, Math.trunc(numeric)) : Number.MAX_SAFE_INTEGER;
  }};
  const defineCharacterDataProperty = (name, descriptor) => {{
    if (!Object.prototype.hasOwnProperty.call(CharacterDataNative.prototype, name)) {{
      Object.defineProperty(CharacterDataNative.prototype, name, descriptor);
    }}
  }};
  defineCharacterDataProperty("data", {{
    configurable: true,
    enumerable: true,
    get() {{ return characterDataText(this); }},
    set(next) {{ characterDataTarget(this).textContent = String(next); }},
  }});
  defineCharacterDataProperty("length", {{
    configurable: true,
    enumerable: true,
    get() {{ return characterDataText(this).length; }},
  }});
  defineCharacterDataProperty("substringData", {{
    configurable: true,
    value(offset, count) {{
      const target = characterDataTarget(this);
      const text = characterDataText(target);
      const start = characterDataOffset(target, offset);
      return text.slice(start, start + characterDataCount(count));
    }},
  }});
  defineCharacterDataProperty("appendData", {{
    configurable: true,
    value(value) {{
      const target = characterDataTarget(this);
      target.textContent = characterDataText(target) + String(value);
    }},
  }});
  defineCharacterDataProperty("insertData", {{
    configurable: true,
    value(offset, value) {{
      const target = characterDataTarget(this);
      const text = characterDataText(target);
      const start = characterDataOffset(target, offset);
      target.textContent = text.slice(0, start) + String(value) + text.slice(start);
    }},
  }});
  defineCharacterDataProperty("deleteData", {{
    configurable: true,
    value(offset, count) {{
      const target = characterDataTarget(this);
      const text = characterDataText(target);
      const start = characterDataOffset(target, offset);
      target.textContent = text.slice(0, start) + text.slice(start + characterDataCount(count));
    }},
  }});
  defineCharacterDataProperty("replaceData", {{
    configurable: true,
    value(offset, count, value) {{
      const target = characterDataTarget(this);
      const text = characterDataText(target);
      const start = characterDataOffset(target, offset);
      target.textContent = text.slice(0, start) + String(value) + text.slice(start + characterDataCount(count));
    }},
  }});
  for (const constructor of [NodeListNative, HtmlCollectionNative]) {{
    if (constructor.prototype && Object.getPrototypeOf(constructor.prototype) !== Array.prototype) {{
      try {{ Object.setPrototypeOf(constructor.prototype, Array.prototype); }} catch (_error) {{}}
    }}
  }}
  const elementConstructors = {{
    HTMLUnknownElement: HTMLElementNative,
    HTMLHtmlElement: HTMLElementNative,
    HTMLBodyElement: HTMLElementNative,
    HTMLFrameSetElement: HTMLElementNative,
    HTMLFormElement: HTMLElementNative,
    HTMLInputElement: HTMLElementNative,
    HTMLTextAreaElement: HTMLElementNative,
    HTMLSelectElement: HTMLElementNative,
    HTMLOptionElement: HTMLElementNative,
    HTMLButtonElement: HTMLElementNative,
    HTMLAnchorElement: HTMLElementNative,
    HTMLCanvasElement: HTMLElementNative,
    HTMLTemplateElement: HTMLElementNative,
    HTMLMediaElement: HTMLMediaElementNative,
    HTMLAudioElement: HTMLMediaElementNative,
    HTMLVideoElement: HTMLMediaElementNative,
    HTMLIFrameElement: HTMLElementNative,
    HTMLFrameElement: HTMLElementNative,
  }};
  for (const name of Object.keys(elementConstructors)) {{
    elementConstructors[name] = ensureNativeConstructor(name, elementConstructors[name]);
  }}
  for (const name of ["HTMLBodyElement", "HTMLFrameSetElement"]) {{
    const prototype = elementConstructors[name].prototype;
    for (const type of nativeWindowEventHandlerTypes) {{
      if (!Object.prototype.hasOwnProperty.call(prototype, "on" + type))
        installEventHandlerProperty(prototype, type, true);
    }}
  }}
  const elementInterfaceNameFor = (tagName) => ({{
      HTML: "HTMLHtmlElement",
      BODY: "HTMLBodyElement",
      FRAMESET: "HTMLFrameSetElement",
      FORM: "HTMLFormElement",
      INPUT: "HTMLInputElement",
      TEXTAREA: "HTMLTextAreaElement",
      SELECT: "HTMLSelectElement",
      OPTION: "HTMLOptionElement",
      BUTTON: "HTMLButtonElement",
      A: "HTMLAnchorElement",
      CANVAS: "HTMLCanvasElement",
      TEMPLATE: "HTMLTemplateElement",
      AUDIO: "HTMLAudioElement",
      VIDEO: "HTMLVideoElement",
      IFRAME: "HTMLIFrameElement",
      FRAME: "HTMLFrameElement",
    }})[String(tagName || "").toUpperCase()] || "HTMLUnknownElement";
  const elementPrototypeFor = (tagName) =>
    elementConstructors[elementInterfaceNameFor(tagName)].prototype;
  const nativeCustomElementLimits = {{
    definitions: 1024,
    pendingWhenDefined: 1024,
    observedAttributes: 256,
    reactions: 4096,
    reactionsPerCheckpoint: 10000,
    nameLength: 256,
    treeDepth: {native_dom_max_depth},
    treeNodes: {native_dom_max_nodes},
  }};
  if (!Number.isSafeInteger(globalThis.__glassNativeCustomElementReactionSuppression)
      || globalThis.__glassNativeCustomElementReactionSuppression < 0) {{
    globalThis.__glassNativeCustomElementReactionSuppression = 0;
  }}
  globalThis.__glassWithNativeCustomElementReactionsSuppressed = (callback) => {{
    const suppression = Number(globalThis.__glassNativeCustomElementReactionSuppression) || 0;
    globalThis.__glassNativeCustomElementReactionSuppression = suppression + 1;
    try {{ return callback(); }}
    finally {{
      globalThis.__glassNativeCustomElementReactionSuppression = Math.max(
        0,
        Number(globalThis.__glassNativeCustomElementReactionSuppression) - 1,
      );
    }}
  }};
  globalThis.__glassNativeInnerHtmlNodeIndexes = (root) => {{
    const indexes = [];
    const pending = [];
    const childrenForIdentity = (node) => {{
      if (Number(node && node.nodeType) === 1
          && node.namespaceURI === HTML_NAMESPACE
          && node.localName === "template") {{
        const content = node.content;
        return content && Array.isArray(content.__glassChildren) ? content.__glassChildren : [];
      }}
      return node && Array.isArray(node.__glassChildren) ? node.__glassChildren : [];
    }};
    const children = childrenForIdentity(root);
    for (let index = children.length - 1; index >= 0; index -= 1) {{
      pending.push({{ node: children[index], depth: 1 }});
    }}
    let visited = 0;
    while (pending.length > 0) {{
      const item = pending.pop();
      const node = item && item.node;
      if (!node || typeof node !== "object") continue;
      if (++visited > nativeCustomElementLimits.treeNodes
          || item.depth > nativeCustomElementLimits.treeDepth) {{
        throw new RangeError("native innerHTML identity walk exceeded its limit");
      }}
      const nodeType = Number(node.nodeType);
      if (nodeType === 1 || nodeType === 3 || nodeType === 8) {{
        const nodeIndex = Number(node.nodeIndex);
        if (!Number.isSafeInteger(nodeIndex) || nodeIndex < 0) {{
          throw new TypeError("native innerHTML node identity is invalid");
        }}
        indexes.push(nodeIndex);
      }}
      const descendants = childrenForIdentity(node);
      for (let index = descendants.length - 1; index >= 0; index -= 1) {{
        pending.push({{ node: descendants[index], depth: item.depth + Number(nodeType === 1) }});
      }}
    }}
    return indexes;
  }};
  const customElementContextId = String(host.context_id || "native");
  const customElementGeneration = Number(host.generation) || 0;
  let nativeCustomElementState = globalThis.__glassNativeCustomElementState;
  if (!nativeCustomElementState
      || nativeCustomElementState.contextId !== customElementContextId
      || nativeCustomElementState.generation !== customElementGeneration) {{
    nativeCustomElementState = {{
      contextId: customElementContextId,
      generation: customElementGeneration,
      definitionsByName: new Map(),
      definitionsByConstructor: new Map(),
      whenDefined: new Map(),
      constructorStack: [],
      reactions: [],
      reactionHead: 0,
      reactionErrors: [],
      formOwners: new WeakMap(),
      formDisabledStates: new WeakMap(),
      formResetting: new WeakSet(),
      defining: false,
      upgrading: 0,
      directConstructions: 0,
      draining: false,
      registry: null,
    }};
    globalThis.__glassNativeCustomElementState = nativeCustomElementState;
  }}
  if (!Array.isArray(nativeCustomElementState.reactions)) nativeCustomElementState.reactions = [];
  if (!(nativeCustomElementState.formOwners instanceof WeakMap)) {{
    nativeCustomElementState.formOwners = new WeakMap();
  }}
  if (!(nativeCustomElementState.formDisabledStates instanceof WeakMap)) {{
    nativeCustomElementState.formDisabledStates = new WeakMap();
  }}
  nativeFormAssociatedDisabledStatesForFocus = nativeCustomElementState.formDisabledStates;
  if (!(nativeCustomElementState.formResetting instanceof WeakSet)) {{
    nativeCustomElementState.formResetting = new WeakSet();
  }}
  if (!Number.isSafeInteger(nativeCustomElementState.reactionHead)
      || nativeCustomElementState.reactionHead < 0
      || nativeCustomElementState.reactionHead > nativeCustomElementState.reactions.length) {{
    nativeCustomElementState.reactionHead = 0;
  }}
  let CustomElementRegistryNative = globalThis.CustomElementRegistry;
  if (typeof CustomElementRegistryNative !== "function") {{
    CustomElementRegistryNative = function CustomElementRegistry() {{
      if (!new.target) throw new TypeError("CustomElementRegistry requires new");
      throw new DOMExceptionNative(
        "Scoped custom element registries are not supported by this native profile",
        "NotSupportedError",
      );
    }};
    globalThis.CustomElementRegistry = CustomElementRegistryNative;
  }}
  const reservedCustomElementNames = new Set([
    "annotation-xml", "color-profile", "font-face", "font-face-src",
    "font-face-uri", "font-face-format", "font-face-name", "missing-glyph",
  ]);
  const isPcenCharacter = (codePoint) => codePoint === 0x2d || codePoint === 0x2e
    || (codePoint >= 0x30 && codePoint <= 0x39)
    || (codePoint >= 0x61 && codePoint <= 0x7a)
    || codePoint === 0x5f || codePoint === 0xb7
    || (codePoint >= 0xc0 && codePoint <= 0xd6)
    || (codePoint >= 0xd8 && codePoint <= 0xf6)
    || (codePoint >= 0xf8 && codePoint <= 0x37d)
    || (codePoint >= 0x37f && codePoint <= 0x1fff)
    || (codePoint >= 0x200c && codePoint <= 0x200d)
    || (codePoint >= 0x203f && codePoint <= 0x2040)
    || (codePoint >= 0x2070 && codePoint <= 0x218f)
    || (codePoint >= 0x2c00 && codePoint <= 0x2fef)
    || (codePoint >= 0x3001 && codePoint <= 0xd7ff)
    || (codePoint >= 0xf900 && codePoint <= 0xfdcf)
    || (codePoint >= 0xfdf0 && codePoint <= 0xfffd)
    || (codePoint >= 0x10000 && codePoint <= 0xeffff);
  const isValidNativeCustomElementName = (name) => {{
    const value = String(name);
    if (value.length === 0 || value.length > nativeCustomElementLimits.nameLength
        || value.charCodeAt(0) < 0x61 || value.charCodeAt(0) > 0x7a
        || !value.includes("-") || reservedCustomElementNames.has(value)) return false;
    for (const character of value) {{
      if (!isPcenCharacter(character.codePointAt(0))) return false;
    }}
    return true;
  }};
  const nativeCustomElementReportError = (error) => {{
    const errors = nativeCustomElementState.reactionErrors;
    if (errors.length >= 128) errors.shift();
    errors.push(error);
    try {{
      if (typeof globalThis.reportError === "function") globalThis.reportError(error);
      else if (globalThis.console && typeof globalThis.console.error === "function") globalThis.console.error(error);
    }} catch (_reportError) {{}}
  }};
  const nativeCustomElementIsValue = (element) => {{
    if (!element) return null;
    const value = element.__glassCustomElementIs;
    if (value === null) return null;
    if (value !== undefined) return String(value);
    return typeof element.getAttribute === "function" ? element.getAttribute("is") : null;
  }};
  const nativeCustomElementDefinitionFor = (element) =>
    element && element.__glassCustomElementDefinition || null;
  const nativeElementInternalsConstructor = ensureNativeConstructor("ElementInternals", null);
  if (!nativeElementInternalsConstructor.prototype[Symbol.toStringTag]) {{
    Object.defineProperty(nativeElementInternalsConstructor.prototype, Symbol.toStringTag, {{
      configurable: true,
      value: "ElementInternals",
    }});
  }}
  const nativeAttachedInternals = globalThis.__glassNativeAttachedInternals instanceof WeakMap
    ? globalThis.__glassNativeAttachedInternals
    : new WeakMap();
  const nativeElementInternalsTargets = globalThis.__glassNativeElementInternalsTargets instanceof WeakMap
    ? globalThis.__glassNativeElementInternalsTargets
    : new WeakMap();
  const nativeElementInternalsStates = globalThis.__glassNativeElementInternalsStates instanceof WeakMap
    ? globalThis.__glassNativeElementInternalsStates
    : new WeakMap();
  const nativeFormAssociatedValues = globalThis.__glassNativeFormAssociatedValues instanceof WeakMap
    ? globalThis.__glassNativeFormAssociatedValues
    : new WeakMap();
  globalThis.__glassNativeAttachedInternals = nativeAttachedInternals;
  globalThis.__glassNativeElementInternalsTargets = nativeElementInternalsTargets;
  globalThis.__glassNativeElementInternalsStates = nativeElementInternalsStates;
  globalThis.__glassNativeFormAssociatedValues = nativeFormAssociatedValues;
  const nativeFormAssociatedDefinitionFor = (element) => {{
    const definition = nativeCustomElementDefinitionFor(element);
    return definition && definition.formAssociated && !definition.extends ? definition : null;
  }};
  const nativeFormAssociatedIsWithin = (element, ancestor) => {{
    let current = element;
    while (current) {{
      if (current.nodeIndex === ancestor.nodeIndex) return true;
      current = current.parentElement;
    }}
    return false;
  }};
  const nativeFormAssociatedIsDisabled = (element, firstLegendByFieldset) => {{
    if (element.hasAttribute("disabled")) return true;
    let ancestor = element.parentElement;
    while (ancestor) {{
      if (ancestor.tagName === "FIELDSET" && ancestor.hasAttribute("disabled")) {{
        const firstLegend = firstLegendByFieldset.get(ancestor.nodeIndex) || null;
        if (!firstLegend || !nativeFormAssociatedIsWithin(element, firstLegend)) return true;
      }}
      ancestor = ancestor.parentElement;
    }}
    return false;
  }};
  const nativeFormAssociatedRecordDisabledState = (element, disabled) => {{
    const nodeIndex = Number(element.nodeIndex);
    if (!Number.isSafeInteger(nodeIndex) || nodeIndex < 0) {{
      throw new RangeError("form-associated custom-element node index is invalid");
    }}
    pushCommand({{
      kind: "setFormAssociatedDisabledState",
      node_index: nodeIndex,
      disabled: Boolean(disabled),
    }});
  }};
  const nativeFormAssociatedTreeRootFor = (element) => {{
    let root = element;
    while (root && root.__glassParent) root = root.__glassParent;
    return root || element;
  }};
  const nativeFormAssociatedFirstElementById = (treeElements) => {{
    const firstElementById = new Map();
    for (const candidate of treeElements) {{
      if (!candidate.hasAttribute("id")) continue;
      const id = String(candidate.id);
      if (!firstElementById.has(id)) firstElementById.set(id, candidate);
    }}
    return firstElementById;
  }};
  const nativeFormAssociatedOwnerFor = (element, treeElements = null, firstElementById = null) => {{
    if (!nativeFormAssociatedDefinitionFor(element)) return null;
    const formReference = element.getAttribute("form");
    if (formReference !== null) {{
      if (formReference === "") return null;
      const candidates = treeElements || nativeCustomElementElementsInTree(
        nodeIsConnected(element) ? document : nativeFormAssociatedTreeRootFor(element),
      );
      const firstMatch = firstElementById
        ? firstElementById.get(formReference)
        : candidates.find((candidate) =>
          candidate.hasAttribute("id") && candidate.id === formReference
        );
      return firstMatch && firstMatch.tagName === "FORM" ? firstMatch : null;
    }}
    let parent = element.parentElement;
    while (parent) {{
      if (parent.tagName === "FORM") return parent;
      parent = parent.parentElement;
    }}
    return null;
  }};
  const nativeFormControlOwnerFor = (element, treeElements, firstElementById) => {{
    if (!element || !["BUTTON", "INPUT", "SELECT", "TEXTAREA"].includes(element.tagName)) return null;
    const formReference = element.getAttribute("form");
    if (formReference !== null) {{
      if (formReference === "") return null;
      const firstMatch = firstElementById.get(formReference);
      return firstMatch && firstMatch.tagName === "FORM" ? firstMatch : null;
    }}
    let parent = element.parentElement;
    while (parent) {{
      if (parent.tagName === "FORM") return parent;
      parent = parent.parentElement;
    }}
    return null;
  }};
  const nativeSelectDisplaySizeIsOne = select => {{
    const rawSize = select.getAttribute("size");
    if (rawSize === null) return true;
    const digits = /^[\t\n\f\r ]*([0-9]+)/.exec(rawSize)?.[1];
    return digits === undefined || /^0*1$/.test(digits);
  }};
  const nativeSelectOptionIsDisabled = option => {{
    if (option.hasAttribute("disabled")) return true;
    let parent = option.parentElement;
    while (parent && parent.tagName !== "SELECT") {{
      if (parent.tagName === "OPTGROUP" && parent.hasAttribute("disabled")) return true;
      parent = parent.parentElement;
    }}
    return false;
  }};
  const nativeFormReset = (form, enqueueCommand) => {{
    if (!form || form.tagName !== "FORM") throw new TypeError("reset requires a form");
    const resetting = nativeCustomElementState.formResetting;
    if (resetting.has(form)) return undefined;
    resetting.add(form);
    try {{
      const event = createEvent("reset", {{ bubbles: true, cancelable: true }});
      const performReset = dispatchTarget(form, event);
      const root = nodeIsConnected(form)
        ? form.ownerDocument || document
        : nativeFormAssociatedTreeRootFor(form);
      const elements = nativeCustomElementElementsInTree(root);
      const firstElementById = nativeFormAssociatedFirstElementById(elements);
      if (performReset) {{
        for (const control of elements) {{
          if (nativeFormControlOwnerFor(control, elements, firstElementById) !== form) continue;
          if (control.tagName === "INPUT") {{
            const type = String(control.getAttribute("type") || "text").toLowerCase();
            if (typeof control.__glassSetFormResetState === "function") {{
              control.__glassSetFormResetState({{
                value: type === "file" ? "" : control.getAttribute("value") || "",
                checked: control.hasAttribute("checked"),
                clearFiles: type === "file",
              }});
            }}
          }} else if (control.tagName === "TEXTAREA") {{
            if (typeof control.__glassSetFormResetState === "function") {{
              control.__glassSetFormResetState({{ value: control.textContent || "" }});
            }}
          }} else if (control.tagName === "SELECT") {{
            const options = elements.filter((option) =>
              option.tagName === "OPTION" && nativeFormAssociatedIsWithin(option, control)
            );
            const selected = options.filter((option) => option.hasAttribute("selected"));
            const multiple = control.hasAttribute("multiple");
            let selectedOptions = selected;
            if (!multiple) {{
              selectedOptions = selected.length > 0
                ? [selected[selected.length - 1]]
                : nativeSelectDisplaySizeIsOne(control)
                  ? options.filter(option => !nativeSelectOptionIsDisabled(option)).slice(0, 1)
                  : [];
            }}
            const selectedSet = new Set(selectedOptions);
            for (const option of options) {{
              if (typeof option.__glassSetFormResetState === "function") {{
                option.__glassSetFormResetState({{ selected: selectedSet.has(option) }});
              }}
            }}
            if (typeof control.__glassSetFormResetState === "function") {{
              control.__glassSetFormResetState({{ value: selectedOptions.length > 0 ? selectedOptions[0].value : "" }});
            }}
          }}
        }}
        const checkedRadioNames = new Set();
        for (let index = elements.length - 1; index >= 0; index -= 1) {{
          const control = elements[index];
          if (nativeFormControlOwnerFor(control, elements, firstElementById) !== form
              || control.tagName !== "INPUT"
              || String(control.getAttribute("type") || "text").toLowerCase() !== "radio"
              || !control.hasAttribute("checked")) continue;
          const name = control.getAttribute("name");
          if (name === null || name === "") continue;
          if (checkedRadioNames.has(name)) {{
            if (typeof control.__glassSetFormResetState === "function") {{
              control.__glassSetFormResetState({{ checked: false }});
            }}
          }} else {{
            checkedRadioNames.add(name);
          }}
        }}
      }}
      enqueueCommand({{
        kind: "resetForm",
        node_index: Number(form.nodeIndex),
        perform_reset: performReset,
      }});
      if (performReset) {{
        nativeCustomElementState.upgrading += 1;
        try {{
          for (const element of elements) {{
            if (nativeFormAssociatedOwnerFor(element, elements, firstElementById) === form) {{
              nativeCustomElementQueueCallback(element, "formResetCallback", []);
            }}
          }}
        }} finally {{
          nativeCustomElementState.upgrading = Math.max(0, nativeCustomElementState.upgrading - 1);
          nativeCustomElementDrainReactions();
        }}
      }}
      return undefined;
    }} finally {{
      resetting.delete(form);
    }}
  }};
  const nativeActivateResetButton = (control, enqueueCommand) => {{
    if (!control || !["BUTTON", "INPUT"].includes(control.tagName)) return;
    if (String(control.getAttribute("type") || "").toLowerCase() !== "reset") return;
    const root = nodeIsConnected(control)
      ? control.ownerDocument || document
      : nativeFormAssociatedTreeRootFor(control);
    const elements = nativeCustomElementElementsInTree(root);
    const firstElementById = nativeFormAssociatedFirstElementById(elements);
    const form = nativeFormControlOwnerFor(control, elements, firstElementById);
    if (form) nativeFormReset(form, enqueueCommand);
  }};
  const nativeFormAssociatedValue = (value) => {{
    if (value === null) return {{ kind: "null" }};
    if (nativeFormDataInstances.has(value)) {{
      if (!Array.isArray(value._entries) || value._entries.length > storageEntryLimit) {{
        throw new RangeError("native ElementInternals FormData entry limit exceeded");
      }}
      const entries = value._entries.map((entry) => {{
        if (!Array.isArray(entry) || entry.length < 2) {{
          throw new TypeError("native ElementInternals FormData entry is invalid");
        }}
        const name = String(entry[0]);
        if (name.length > storageKeyLimit) {{
          throw new RangeError("native ElementInternals FormData name exceeds its limit");
        }}
        const source = entry[1];
        if (source && source.kind === "file"
            && nativeFileInstances.has(source.value)) {{
          return [name, {{
            kind: "file",
            value: source.value,
            filename: String(source.filename === undefined ? source.value.name : source.filename),
          }}];
        }}
        const text = String(source);
        if (text.length > storageValueLimit) {{
          throw new RangeError("native ElementInternals FormData value exceeds its limit");
        }}
        return [name, text];
      }});
      return {{ kind: "formdata", entries }};
    }}
    if (nativeFileInstances.has(value)) {{
      return {{ kind: "file", value, filename: String(value.name || "") }};
    }}
    if (typeof value === "symbol") throw new TypeError("native ElementInternals value cannot be a Symbol");
    const text = String(value);
    if (text.length > storageValueLimit) {{
      throw new RangeError("native ElementInternals form value exceeds its limit");
    }}
    return {{ kind: "string", value: text }};
  }};
  Object.defineProperty(nativeElementInternalsConstructor.prototype, "form", {{
    configurable: true,
    enumerable: true,
    get() {{
      const target = nativeElementInternalsTargets.get(this);
      if (!target) throw new TypeError("Illegal ElementInternals receiver");
      if (!nativeFormAssociatedDefinitionFor(target)) {{
        throw new DOMExceptionNative("ElementInternals.form requires a form-associated custom element", "NotSupportedError");
      }}
      return nativeFormAssociatedOwnerFor(target);
    }},
  }});
  Object.defineProperty(nativeElementInternalsConstructor.prototype, "setFormValue", {{
    configurable: true,
    writable: true,
    value(value, state) {{
      const target = nativeElementInternalsTargets.get(this);
      if (!target) throw new TypeError("Illegal ElementInternals receiver");
      if (arguments.length === 0) throw new TypeError("setFormValue requires a value");
      const submission = nativeFormAssociatedValue(value);
      const retainedState = arguments.length < 2 ? submission : nativeFormAssociatedValue(state);
      if (!nativeFormAssociatedDefinitionFor(target)) {{
        throw new DOMExceptionNative("setFormValue requires a form-associated custom element", "NotSupportedError");
      }}
      nativeFormAssociatedValues.set(target, submission);
      nativeElementInternalsStates.set(this, retainedState);
    }},
  }});
  Object.defineProperty(HTMLElementNative.prototype, "attachInternals", {{
    configurable: true,
    writable: true,
    value() {{
      const element = this;
      const definition = nativeCustomElementDefinitionFor(element);
      const customState = element && element.__glassCustomElementState;
      if (!element || typeof element !== "object" || !definition
          || nativeCustomElementIsValue(element) !== null
          || definition.disableInternals
          || !["precustomized", "custom"].includes(customState)
          || nativeAttachedInternals.has(element)) {{
        throw new DOMExceptionNative("attachInternals is not available for this element", "NotSupportedError");
      }}
      const internals = Object.create(nativeElementInternalsConstructor.prototype);
      nativeAttachedInternals.set(element, internals);
      nativeElementInternalsTargets.set(internals, element);
      return internals;
    }},
  }});
  const nativeCustomElementPendingReactionCount = () =>
    Math.max(0, nativeCustomElementState.reactions.length - nativeCustomElementState.reactionHead);
  const nativeCustomElementEnqueueReaction = (reaction) => {{
    if (nativeCustomElementPendingReactionCount() >= nativeCustomElementLimits.reactions) {{
      nativeCustomElementReportError(new RangeError("native custom element reaction queue exceeded its limit"));
      return false;
    }}
    nativeCustomElementState.reactions.push(reaction);
    return true;
  }};
  const nativeCustomElementDrainReactions = () => {{
    if (nativeCustomElementState.draining || nativeCustomElementState.upgrading > 0) return;
    nativeCustomElementState.draining = true;
    let work = 0;
    try {{
      while (nativeCustomElementState.reactionHead < nativeCustomElementState.reactions.length) {{
        if (++work > nativeCustomElementLimits.reactionsPerCheckpoint) {{
          nativeCustomElementState.reactions.length = 0;
          nativeCustomElementState.reactionHead = 0;
          nativeCustomElementReportError(new RangeError("native custom element reaction checkpoint exceeded its work limit"));
          break;
        }}
        const reaction = nativeCustomElementState.reactions[nativeCustomElementState.reactionHead];
        nativeCustomElementState.reactionHead += 1;
        try {{ reaction.callback.apply(reaction.element, reaction.arguments); }}
        catch (error) {{ nativeCustomElementReportError(error); }}
        if (nativeCustomElementState.reactionHead >= 256
            && nativeCustomElementState.reactionHead * 2 >= nativeCustomElementState.reactions.length) {{
          nativeCustomElementState.reactions.splice(0, nativeCustomElementState.reactionHead);
          nativeCustomElementState.reactionHead = 0;
        }}
      }}
    }} finally {{
      if (nativeCustomElementState.reactionHead >= nativeCustomElementState.reactions.length) {{
        nativeCustomElementState.reactions.length = 0;
        nativeCustomElementState.reactionHead = 0;
      }}
      nativeCustomElementState.draining = false;
    }}
  }};
  const nativeCustomElementQueueCallback = (element, callbackName, argumentsList, observedOnly = false) => {{
    const definition = nativeCustomElementDefinitionFor(element);
    const status = element && element.__glassCustomElementState;
    if (!definition || (status !== "custom" && status !== "precustomized")) return;
    if (observedOnly && !definition.observedAttributes.includes(String(argumentsList[0]))) return;
    const callback = definition.callbacks[callbackName];
    if (typeof callback !== "function") return;
    if (element.__glassDirectCustomConstructionPending) {{
      const pending = Array.isArray(element.__glassDirectCustomElementReactions)
        ? element.__glassDirectCustomElementReactions
        : (element.__glassDirectCustomElementReactions = []);
      if (pending.length >= nativeCustomElementLimits.reactions) {{
        nativeCustomElementReportError(new RangeError("native custom element construction reactions exceeded their limit"));
        return;
      }}
      pending.push({{ element, callback, arguments: argumentsList }});
      return;
    }}
    if (!nativeCustomElementEnqueueReaction({{ element, callback, arguments: argumentsList }})) return;
    nativeCustomElementDrainReactions();
  }};
  const nativeCustomElementElementsInTree = (root) => {{
    const result = [];
    const pending = [{{ node: root, depth: 0 }}];
    let visited = 0;
    while (pending.length > 0) {{
      const item = pending.pop();
      const node = item && item.node;
      if (!node || typeof node !== "object") continue;
      if (++visited > nativeCustomElementLimits.treeNodes
          || item.depth > nativeCustomElementLimits.treeDepth) {{
        throw new RangeError("native custom element tree walk exceeded its limit");
      }}
      if (Number(node.nodeType) === 1) result.push(node);
      if (Number(node.nodeType) === 1 && node.namespaceURI === HTML_NAMESPACE
          && node.localName === "template") continue;
      const children = Array.isArray(node.__glassChildren) ? node.__glassChildren : [];
      for (let index = children.length - 1; index >= 0; index -= 1) {{
        pending.push({{ node: children[index], depth: item.depth + Number(Number(node.nodeType) === 1) }});
      }}
    }}
    return result;
  }};
  const nativeFormAssociatedReconcile = (extraRoot = null) => {{
    if (suppressHostCommands > 0
        || Number(globalThis.__glassNativeCustomElementReactionSuppression) > 0) return;
    const documentElements = nativeCustomElementElementsInTree(document);
    const documentFirstElementById = nativeFormAssociatedFirstElementById(documentElements);
    const candidates = documentElements.slice();
    const seen = new Set(documentElements);
    if (extraRoot) {{
      for (const element of nativeCustomElementElementsInTree(extraRoot)) {{
        if (!seen.has(element)) {{
          seen.add(element);
          candidates.push(element);
        }}
      }}
    }}
    const firstLegendByFieldset = new Map();
    for (const element of candidates) {{
      if (element.tagName !== "LEGEND") continue;
      const parent = element.parentElement;
      if (parent && parent.tagName === "FIELDSET"
          && !firstLegendByFieldset.has(parent.nodeIndex)) {{
        firstLegendByFieldset.set(parent.nodeIndex, element);
      }}
    }}
    const detachedTrees = new Map();
    const treeStateFor = (element) => {{
      if (nodeIsConnected(element)) {{
        return {{ elements: documentElements, firstElementById: documentFirstElementById }};
      }}
      const root = nativeFormAssociatedTreeRootFor(element);
      if (!detachedTrees.has(root)) {{
        const elements = nativeCustomElementElementsInTree(root);
        detachedTrees.set(root, {{
          elements,
          firstElementById: nativeFormAssociatedFirstElementById(elements),
        }});
      }}
      return detachedTrees.get(root);
    }};
    nativeCustomElementState.upgrading += 1;
    try {{
      for (const element of candidates) {{
        if (!nativeFormAssociatedDefinitionFor(element)
            || !["custom", "precustomized"].includes(element.__glassCustomElementState)) {{
          continue;
        }}
        const treeState = treeStateFor(element);
        const owner = nativeFormAssociatedOwnerFor(
          element, treeState.elements, treeState.firstElementById,
        );
        const owners = nativeCustomElementState.formOwners;
        if (!owners.has(element)) {{
          owners.set(element, owner);
          if (owner) nativeCustomElementQueueCallback(element, "formAssociatedCallback", [owner]);
        }} else if (owners.get(element) !== owner) {{
          owners.set(element, owner);
          nativeCustomElementQueueCallback(element, "formAssociatedCallback", [owner]);
        }}
        const disabled = nativeFormAssociatedIsDisabled(element, firstLegendByFieldset);
        const disabledStates = nativeCustomElementState.formDisabledStates;
        if (!disabledStates.has(element)) {{
          disabledStates.set(element, disabled);
          nativeFormAssociatedRecordDisabledState(element, disabled);
          if (disabled) nativeCustomElementQueueCallback(element, "formDisabledCallback", [true]);
        }} else if (disabledStates.get(element) !== disabled) {{
          disabledStates.set(element, disabled);
          nativeFormAssociatedRecordDisabledState(element, disabled);
          nativeCustomElementQueueCallback(element, "formDisabledCallback", [disabled]);
        }}
      }}
    }} finally {{
      nativeCustomElementState.upgrading = Math.max(0, nativeCustomElementState.upgrading - 1);
    }}
    nativeCustomElementDrainReactions();
  }};
  globalThis.__glassReconcileNativeFormAssociatedLifecycle = nativeFormAssociatedReconcile;
  const nativeCustomElementTryUpgrade = (element, enqueueConnection = true) => {{
    if (!element || Number(element.nodeType) !== 1
        || element.namespaceURI !== HTML_NAMESPACE) return element;
    const localName = String(element.localName || "");
    const isValue = nativeCustomElementIsValue(element);
    const name = isValue === null ? localName : isValue;
    const definition = nativeCustomElementState.definitionsByName.get(name);
    if (!definition || definition.localName !== localName) return element;
    const currentState = element.__glassCustomElementState;
    if (currentState && currentState !== "undefined") {{
      const existingDefinition = nativeCustomElementDefinitionFor(element) || definition;
      if (currentState === "custom" || currentState === "failed"
          || currentState === "precustomized") {{
        try {{ Object.setPrototypeOf(element, existingDefinition.prototype); }}
        catch (error) {{ nativeCustomElementReportError(error); }}
      }}
      return element;
    }}
    if (element.__glassCustomElementRegistry
        && element.__glassCustomElementRegistry !== nativeCustomElementState.registry) return element;
    Object.defineProperty(element, "__glassCustomElementDefinition", {{
      configurable: true, enumerable: false, writable: true, value: definition,
    }});
    Object.defineProperty(element, "__glassCustomElementState", {{
      configurable: true, enumerable: false, writable: true, value: "precustomized",
    }});
    try {{ Object.setPrototypeOf(element, definition.prototype); }}
    catch (error) {{
      element.__glassCustomElementState = "failed";
      nativeCustomElementReportError(error);
      return element;
    }}
    const stackEntry = {{ element, definition, constructed: false }};
    nativeCustomElementState.constructorStack.push(stackEntry);
    nativeCustomElementState.upgrading += 1;
    let result;
    let failure = null;
    try {{ result = Reflect.construct(definition.constructor, []); }}
    catch (error) {{ failure = error; }}
    finally {{
      const index = nativeCustomElementState.constructorStack.lastIndexOf(stackEntry);
      if (index >= 0) nativeCustomElementState.constructorStack.splice(index, 1);
    }}
    if (!failure && (result !== element || !stackEntry.constructed)) {{
      failure = new TypeError("custom element constructor did not return its native element");
    }}
    if (failure) {{
      element.__glassCustomElementState = "failed";
      nativeCustomElementReportError(failure);
      nativeCustomElementState.upgrading -= 1;
      nativeCustomElementDrainReactions();
      return element;
    }}
    element.__glassCustomElementState = "custom";
    let attributes = [];
    try {{
      attributes = typeof element.getAttributeNames === "function" ? element.getAttributeNames() : [];
    }} catch (error) {{ nativeCustomElementReportError(error); }}
    for (const attributeName of attributes) {{
      const value = element.getAttribute(attributeName);
      if (value !== null) {{
        const namespace = typeof element.__glassAttributeNamespace === "function"
          ? element.__glassAttributeNamespace(attributeName)
          : null;
        nativeCustomElementQueueCallback(
          element,
          "attributeChangedCallback",
          [String(attributeName), null, String(value), namespace],
          true,
        );
      }}
    }}
    if (enqueueConnection && typeof nodeIsConnected === "function" && nodeIsConnected(element)) {{
      nativeCustomElementQueueCallback(element, "connectedCallback", []);
    }}
    nativeCustomElementState.upgrading -= 1;
    nativeCustomElementDrainReactions();
    return element;
  }};
  const nativeCustomElementUpgradeTree = (root, connectedOnly = false) => {{
    for (const element of nativeCustomElementElementsInTree(root)) {{
      if (!connectedOnly || (typeof nodeIsConnected === "function" && nodeIsConnected(element))) {{
        nativeCustomElementTryUpgrade(element);
      }}
    }}
  }};
  globalThis.__glassQueueNativeCustomElementReaction = nativeCustomElementQueueCallback;
  globalThis.__glassConstructNativeCustomElement = (newTarget, interfaceName = "HTMLElement") => {{
    const stack = nativeCustomElementState.constructorStack;
    const active = stack.length > 0 ? stack[stack.length - 1] : null;
    if (active) {{
      if (newTarget !== active.definition.constructor || active.constructed
          || interfaceName !== active.definition.interfaceName) {{
        throw new TypeError("custom element constructor stack is invalid");
      }}
      active.constructed = true;
      return active.element;
    }}
    const definition = nativeCustomElementState.definitionsByConstructor.get(newTarget);
    if (!definition || newTarget !== definition.constructor
        || interfaceName !== definition.interfaceName) {{
      throw new TypeError("Illegal constructor");
    }}
    const element = makeDetachedElement(definition.localName, HTML_NAMESPACE);
    element.__glassCustomElementIs = definition.extends ? definition.name : null;
    Object.defineProperty(element, "__glassCustomElementDefinition", {{
      configurable: true, enumerable: false, writable: true, value: definition,
    }});
    Object.defineProperty(element, "__glassCustomElementState", {{
      configurable: true, enumerable: false, writable: true, value: "precustomized",
    }});
    Object.defineProperty(element, "__glassDirectCustomConstructionPending", {{
      configurable: true, enumerable: false, writable: true, value: true,
    }});
    Object.defineProperty(element, "__glassDirectCustomElementReactions", {{
      configurable: true, enumerable: false, writable: true, value: [],
    }});
    Object.setPrototypeOf(element, definition.prototype);
    Promise.resolve().then(() => {{
      if (!element.__glassDirectCustomConstructionPending) return;
      element.__glassDirectCustomConstructionPending = false;
      if (element.__glassCustomElementState === "precustomized") {{
        element.__glassCustomElementState = "custom";
      }}
      for (const reaction of element.__glassDirectCustomElementReactions) {{
        if (!nativeCustomElementEnqueueReaction(reaction)) break;
      }}
      element.__glassDirectCustomElementReactions.length = 0;
      nativeCustomElementDrainReactions();
    }});
    return element;
  }};
  globalThis.__glassCreateNativeCustomElement = (targetDocument, tagName, options, namespace) => {{
    const namespaceURI = normalizeElementNamespace(namespace);
    let creationOptions = {{}};
    let hasIsValue = false;
    let isValue = null;
    if (typeof options === "string") {{
      hasIsValue = true;
      isValue = options;
    }} else if (options !== undefined && options !== null) {{
      if (typeof options !== "object" && typeof options !== "function") {{
        throw new TypeError("createElement options must be a dictionary");
      }}
      creationOptions = Object(options);
      if (creationOptions.is !== undefined) {{
        hasIsValue = true;
        isValue = String(creationOptions.is);
      }}
    }}
    const selectedRegistry = creationOptions.customElementRegistry;
    const registryWasProvided = selectedRegistry !== undefined;
    if (hasIsValue && registryWasProvided) {{
      throw new DOMExceptionNative("A custom element registry and is value cannot both be supplied", "NotSupportedError");
    }}
    if (selectedRegistry !== undefined && selectedRegistry !== null
        && selectedRegistry !== nativeCustomElementState.registry) {{
      throw new DOMExceptionNative("The requested custom element registry is unavailable", "NotSupportedError");
    }}
    const element = makeDetachedElement(tagName, namespaceURI);
    if (hasIsValue) element.__glassCustomElementIs = isValue;
    if (targetDocument === document && namespaceURI === HTML_NAMESPACE) {{
      nativeCustomElementTryUpgrade(element);
    }}
    return element;
  }};
  globalThis.__glassRunNativeCustomElementInsertionReactions = (root, wasConnected, isConnected) => {{
    if (!root || suppressHostCommands > 0
        || Number(globalThis.__glassNativeCustomElementReactionSuppression) > 0) return;
    const elements = nativeCustomElementElementsInTree(root);
    nativeCustomElementState.upgrading += 1;
    try {{
      if (isConnected) {{
        for (const element of elements) nativeCustomElementTryUpgrade(element, false);
      }}
      if (wasConnected) {{
        for (const element of elements) {{
          nativeCustomElementQueueCallback(element, "disconnectedCallback", []);
        }}
      }}
      if (isConnected) {{
        for (const element of elements) {{
          nativeCustomElementQueueCallback(element, "connectedCallback", []);
        }}
      }}
      nativeFormAssociatedReconcile(root);
    }} finally {{
      nativeCustomElementState.upgrading = Math.max(0, nativeCustomElementState.upgrading - 1);
      nativeCustomElementDrainReactions();
    }}
  }};
  globalThis.__glassRunNativeCustomElementRemovalReactions = (root) => {{
    nativeCustomElementState.upgrading += 1;
    try {{
      for (const element of nativeCustomElementElementsInTree(root)) {{
        nativeCustomElementQueueCallback(element, "disconnectedCallback", []);
      }}
      nativeFormAssociatedReconcile(root);
    }} finally {{
      nativeCustomElementState.upgrading = Math.max(0, nativeCustomElementState.upgrading - 1);
    }}
    nativeCustomElementDrainReactions();
  }};
  const nativeCustomRegistry = nativeCustomElementState.registry
    || Object.create(CustomElementRegistryNative.prototype);
  nativeCustomElementState.registry = nativeCustomRegistry;
  Object.defineProperties(nativeCustomRegistry, {{
    define: {{ configurable: true, writable: true, value(name, constructor, options = undefined) {{
      const elementName = String(name);
      if (typeof constructor !== "function") throw new TypeError("custom element constructor must be a function");
      try {{ Reflect.construct(function NativeConstructorCheck() {{}}, [], constructor); }}
      catch (_error) {{ throw new TypeError("custom element constructor must be constructible"); }}
      if (!isValidNativeCustomElementName(elementName)) {{
        throw new DOMExceptionNative("The custom element name is invalid", "SyntaxError");
      }}
      if (nativeCustomElementState.definitionsByName.has(elementName)
          || nativeCustomElementState.definitionsByConstructor.has(constructor)) {{
        throw new DOMExceptionNative("The custom element name or constructor is already defined", "NotSupportedError");
      }}
      if (nativeCustomElementState.defining) {{
        throw new DOMExceptionNative("A custom element definition is already running", "NotSupportedError");
      }}
      const definitionOptions = options === undefined || options === null ? {{}} : Object(options);
      const extendedTag = definitionOptions.extends === undefined
        ? null
        : String(definitionOptions.extends);
      const interfaceName = extendedTag === null
        ? "HTMLElement"
        : elementInterfaceNameFor(extendedTag);
      if (extendedTag !== null
          && (extendedTag !== extendedTag.toLowerCase()
            || isValidNativeCustomElementName(extendedTag)
            || interfaceName === "HTMLUnknownElement")) {{
        throw new DOMExceptionNative("The extends value is not a supported built-in HTML element", "NotSupportedError");
      }}
      const localName = extendedTag === null ? elementName : extendedTag;
      if (nativeCustomElementState.definitionsByName.size >= nativeCustomElementLimits.definitions) {{
        throw new RangeError("native custom element definition limit exceeded");
      }}
      nativeCustomElementState.defining = true;
      let definition;
      try {{
        const prototype = constructor.prototype;
        if (!prototype || (typeof prototype !== "object" && typeof prototype !== "function")
            || !HTMLElementNative.prototype.isPrototypeOf(prototype)) {{
          throw new TypeError("custom element constructor must extend HTMLElement");
        }}
        const interfaceConstructor = interfaceName === "HTMLElement"
          ? HTMLElementNative
          : elementConstructors[interfaceName];
        const interfacePrototype = interfaceConstructor.prototype;
        if (extendedTag !== null && !interfacePrototype.isPrototypeOf(prototype)) {{
          throw new DOMExceptionNative(
            "The custom element constructor must extend " + interfaceName,
            "NotSupportedError",
          );
        }}
        const callbacks = {{}};
        for (const callbackName of [
          "connectedCallback", "disconnectedCallback", "adoptedCallback", "attributeChangedCallback",
        ]) {{
          const callback = prototype[callbackName];
          if (callback !== undefined && typeof callback !== "function") {{
            throw new TypeError(callbackName + " must be callable");
          }}
          callbacks[callbackName] = callback === undefined ? null : callback;
        }}
        let observedAttributes = [];
        if (callbacks.attributeChangedCallback) {{
          const observed = constructor.observedAttributes;
          if (observed !== undefined) {{
            for (const value of observed) {{
              if (observedAttributes.length >= nativeCustomElementLimits.observedAttributes) {{
                throw new RangeError("native custom element observed-attribute limit exceeded");
              }}
              observedAttributes.push(String(value));
            }}
          }}
        }}
        const disabledFeatures = new Set();
        const disabledFeaturesIterable = constructor.disabledFeatures;
        if (disabledFeaturesIterable !== undefined) {{
          let disabledFeatureCount = 0;
          for (const feature of disabledFeaturesIterable) {{
            if (disabledFeatureCount >= 32) {{
              throw new RangeError("native custom element disabled-feature limit exceeded");
            }}
            disabledFeatureCount += 1;
            disabledFeatures.add(String(feature));
          }}
        }}
        const formAssociated = Boolean(constructor.formAssociated);
        if (formAssociated) {{
          for (const callbackName of [
            "formAssociatedCallback", "formDisabledCallback",
            "formResetCallback", "formStateRestoreCallback",
          ]) {{
            const callback = prototype[callbackName];
            if (callback !== undefined && typeof callback !== "function") {{
              throw new TypeError(callbackName + " must be callable");
            }}
            if (callbackName === "formStateRestoreCallback"
                && callback !== undefined) {{
              throw new DOMExceptionNative(
                callbackName + " is not supported by this native profile",
                "NotSupportedError",
              );
            }}
            callbacks[callbackName] = callback === undefined ? null : callback;
          }}
        }}
        definition = {{
          name: elementName,
          localName,
          extends: extendedTag,
          interfaceName,
          constructor,
          prototype,
          callbacks,
          observedAttributes,
          formAssociated,
          disableInternals: disabledFeatures.has("internals"),
        }};
      }} finally {{ nativeCustomElementState.defining = false; }}
      nativeCustomElementState.definitionsByName.set(elementName, definition);
      nativeCustomElementState.definitionsByConstructor.set(constructor, definition);
      nativeCustomElementState.upgrading += 1;
      try {{
        nativeCustomElementUpgradeTree(document, true);
        nativeFormAssociatedReconcile(document);
      }}
      finally {{
        nativeCustomElementState.upgrading = Math.max(0, nativeCustomElementState.upgrading - 1);
        nativeCustomElementDrainReactions();
      }}
      const pending = nativeCustomElementState.whenDefined.get(elementName);
      if (pending) {{
        nativeCustomElementState.whenDefined.delete(elementName);
        pending.resolve(constructor);
      }}
    }} }},
    get: {{ configurable: true, writable: true, value(name) {{
      const definition = nativeCustomElementState.definitionsByName.get(String(name));
      return definition ? definition.constructor : undefined;
    }} }},
    getName: {{ configurable: true, writable: true, value(constructor) {{
      const definition = nativeCustomElementState.definitionsByConstructor.get(constructor);
      return definition ? definition.name : null;
    }} }},
    whenDefined: {{ configurable: true, writable: true, value(name) {{
      const elementName = String(name);
      if (!isValidNativeCustomElementName(elementName)) {{
        return Promise.reject(new DOMExceptionNative("The custom element name is invalid", "SyntaxError"));
      }}
      const definition = nativeCustomElementState.definitionsByName.get(elementName);
      if (definition) return Promise.resolve(definition.constructor);
      if (!nativeCustomElementState.whenDefined.has(elementName)) {{
        if (nativeCustomElementState.whenDefined.size >= nativeCustomElementLimits.pendingWhenDefined) {{
          return Promise.reject(new RangeError("native whenDefined promise limit exceeded"));
        }}
        let resolve;
        const promise = new Promise((settle) => {{ resolve = settle; }});
        nativeCustomElementState.whenDefined.set(elementName, {{ promise, resolve }});
      }}
      return nativeCustomElementState.whenDefined.get(elementName).promise;
    }} }},
    upgrade: {{ configurable: true, writable: true, value(root) {{
      if (!root || typeof root !== "object") throw new TypeError("upgrade root must be a Node");
      nativeCustomElementState.upgrading += 1;
      try {{
        nativeCustomElementUpgradeTree(root, false);
        nativeFormAssociatedReconcile(root);
      }}
      finally {{
        nativeCustomElementState.upgrading = Math.max(0, nativeCustomElementState.upgrading - 1);
        nativeCustomElementDrainReactions();
      }}
    }} }},
    initialize: {{ configurable: true, writable: true, value(root) {{
      if (!root || typeof root !== "object") throw new TypeError("initialize root must be a Node");
      const ownerDocument = Number(root.nodeType) === 9 ? root : root.ownerDocument;
      if (ownerDocument !== document) {{
        throw new DOMExceptionNative("The global custom element registry cannot initialize this tree", "NotSupportedError");
      }}
      const elements = nativeCustomElementElementsInTree(root);
      for (const element of elements) {{
        if (element.__glassCustomElementRegistry
            && element.__glassCustomElementRegistry !== nativeCustomRegistry) {{
          throw new DOMExceptionNative("The element already belongs to another registry", "NotSupportedError");
        }}
      }}
      for (const element of elements) {{
        if (!element.__glassCustomElementRegistry) {{
          Object.defineProperty(element, "__glassCustomElementRegistry", {{
            configurable: true, enumerable: false, writable: true, value: nativeCustomRegistry,
          }});
        }}
      }}
      nativeCustomElementState.upgrading += 1;
      try {{
        nativeCustomElementUpgradeTree(root, false);
        nativeFormAssociatedReconcile(root);
      }}
      finally {{
        nativeCustomElementState.upgrading = Math.max(0, nativeCustomElementState.upgrading - 1);
        nativeCustomElementDrainReactions();
      }}
    }} }},
  }});
  globalThis.__glassNativeCustomElementRegistry = nativeCustomRegistry;
  globalThis.__glassNativeCustomElementErrors = nativeCustomElementState.reactionErrors;
  globalThis.customElements = nativeCustomRegistry;
  Object.defineProperty(document, "customElementRegistry", {{
    configurable: true,
    enumerable: true,
    get() {{ return nativeCustomRegistry; }},
  }});
