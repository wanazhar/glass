  const splitSelectorList = (selector) => {{
    const value = String(selector).trim();
    if (!value) throw new SyntaxError("selector cannot be empty");
    const result = [];
    let part = "";
    let depth = 0;
    let quote = "";
    for (let index = 0; index < value.length; index += 1) {{
      const character = value[index];
      if (quote) {{
        part += character;
        if (character === quote && value[index - 1] !== "\\") quote = "";
        continue;
      }}
      if (character === "'" || character === '"') {{
        quote = character;
        part += character;
      }} else if (character === "[" || character === "(") {{
        depth += 1;
        part += character;
      }} else if (character === "]" || character === ")") {{
        depth -= 1;
        if (depth < 0) throw new SyntaxError("unbalanced selector");
        part += character;
      }} else if (character === "," && depth === 0) {{
        if (!part.trim()) throw new SyntaxError("empty selector list member");
        result.push(part.trim());
        part = "";
      }} else {{
        part += character;
      }}
    }}
    if (quote || depth !== 0 || !part.trim()) throw new SyntaxError("invalid selector");
    result.push(part.trim());
    return result;
  }};
  const parseSelectorChain = (selector) => {{
    const tokens = [];
    let part = "";
    let depth = 0;
    let quote = "";
    let pendingSpace = false;
    const pushSimple = () => {{
      if (!part.trim()) return;
      tokens.push({{ type: "simple", value: part.trim() }});
      part = "";
    }};
    for (let index = 0; index < selector.length; index += 1) {{
      const character = selector[index];
      if (quote) {{
        part += character;
        if (character === quote && selector[index - 1] !== "\\") quote = "";
        continue;
      }}
      if (character === "'" || character === '"') {{
        quote = character;
        part += character;
      }} else if (character === "[" || character === "(") {{
        depth += 1;
        pendingSpace = false;
        part += character;
      }} else if (character === "]" || character === ")") {{
        depth -= 1;
        if (depth < 0) throw new SyntaxError("unbalanced selector");
        part += character;
      }} else if (depth === 0 && [">", "+", "~"].includes(character)) {{
        pushSimple();
        if (tokens[tokens.length - 1] && tokens[tokens.length - 1].type === "combinator") {{
          throw new SyntaxError("invalid selector combinator");
        }}
        tokens.push({{ type: "combinator", value: character }});
        pendingSpace = false;
      }} else if (depth === 0 && /\s/.test(character)) {{
        pushSimple();
        pendingSpace = true;
      }} else {{
        if (pendingSpace && tokens.length > 0
            && tokens[tokens.length - 1].type === "simple") {{
          tokens.push({{ type: "combinator", value: " " }});
        }}
        pendingSpace = false;
        part += character;
      }}
    }}
    pushSimple();
    if (quote || depth !== 0 || tokens.length === 0
        || tokens[0].type !== "simple"
        || tokens[tokens.length - 1].type !== "simple") {{
      throw new SyntaxError("invalid selector chain");
    }}
    return tokens;
  }};
  const attributeNamespaceForSelectorPrefix = (prefix) => {{
    switch (String(prefix).toLowerCase()) {{
      case "html": return HTML_NAMESPACE;
      case "svg": return SVG_NAMESPACE;
      case "math": return MATHML_NAMESPACE;
      case "xml": return XML_NAMESPACE;
      case "xmlns": return XMLNS_NAMESPACE;
      case "xlink": return XLINK_NAMESPACE;
      default: throw new SyntaxError("unsupported attribute namespace");
    }}
  }};
  const selectorAttributeCaseInsensitiveGlobalNames = new Set([
    "contenteditable", "dir", "draggable", "hidden", "spellcheck", "translate",
  ]);
  const selectorAttributeCaseInsensitiveTagNames = Object.freeze({{
    accept: new Set(["input"]),
    autocomplete: new Set(["form", "input", "select", "textarea"]),
    charset: new Set(["meta", "script"]),
    crossorigin: new Set(["audio", "img", "link", "script", "video"]),
    decoding: new Set(["img"]),
    enctype: new Set(["form"]),
    fetchpriority: new Set(["img", "link", "script"]),
    formenctype: new Set(["button", "input"]),
    formmethod: new Set(["button", "input"]),
    formtarget: new Set(["button", "input"]),
    "http-equiv": new Set(["meta"]),
    inputmode: new Set(["input", "textarea"]),
    kind: new Set(["track"]),
    loading: new Set(["iframe", "img", "link", "script"]),
    method: new Set(["form"]),
    preload: new Set(["audio", "video"]),
    rel: new Set(["a", "area", "link"]),
    referrerpolicy: new Set(["a", "area", "iframe", "img", "link", "script"]),
    scope: new Set(["th"]),
    shape: new Set(["a", "area"]),
    target: new Set(["a", "area", "base", "form"]),
    type: new Set(["button", "input", "link", "object", "ol", "script", "source", "style"]),
    wrap: new Set(["textarea"]),
  }});
  const defaultAttributeCaseInsensitiveForSelector = (element, localName, namespaced) => {{
    if (namespaced) return false;
    const name = String(localName).toLowerCase();
    if (selectorAttributeCaseInsensitiveGlobalNames.has(name)) return true;
    const tags = selectorAttributeCaseInsensitiveTagNames[name];
    const tagName = String(element.localName || element.tagName || "").toLowerCase();
    return Boolean(tags && tags.has(tagName));
  }};
  const selectorAttribute = (element, expression) => {{
    const match = String(expression).trim().match(
      /^((?:(?:\*|[^\s~|^$!=]+)?\|(?:\*|[^\s~|^$!=]+)|[^\s~|^$!=]+))\s*(?:(!=|[~|^$*]?=)\s*(.*?)\s*)?$/
    );
    if (!match) throw new SyntaxError("invalid attribute selector");
    const qualifiedName = match[1];
    const operator = match[2] || null;
    let expected = match[3] === undefined ? "" : match[3].trim();
    let caseMode = null;
    const modifier = expected.match(/\s+([A-Za-z]+)$/);
    if (modifier) {{
      const value = modifier[1].toLowerCase();
      if (value !== "i" && value !== "s") throw new SyntaxError("invalid attribute selector");
      caseMode = value;
      expected = expected.slice(0, modifier.index).trim();
    }}
    if (expected && ((expected.startsWith('"') && expected.endsWith('"'))
        || (expected.startsWith("'") && expected.endsWith("'")))) {{
      expected = expected.slice(1, -1);
    }}
    const separator = qualifiedName.indexOf("|");
    let localName = qualifiedName;
    let namespaceMode = "name";
    let namespaceURI = null;
    if (separator >= 0) {{
      const prefix = qualifiedName.slice(0, separator);
      localName = qualifiedName.slice(separator + 1);
      if (!localName) throw new SyntaxError("invalid attribute selector");
      if (prefix === "*") namespaceMode = "any";
      else namespaceURI = prefix
        ? attributeNamespaceForSelectorPrefix(prefix)
        : null;
      if (!prefix) namespaceMode = "exact";
    }} else if (qualifiedName.includes("*")) {{
      throw new SyntaxError("invalid attribute selector");
    }}
    const qualifiedLookup = qualifiedName.toLowerCase();
    const localLookup = localName.toLowerCase();
    const attributeNames = typeof element.getAttributeNames === "function"
      ? element.getAttributeNames()
      : Object.keys(element.attributes || {{}});
    const candidate = attributeNames.find((attributeName) => {{
      const candidateNamespace = typeof element.__glassAttributeNamespace === "function"
        ? element.__glassAttributeNamespace(attributeName)
        : null;
      if (separator < 0) return attributeName.toLowerCase() === qualifiedLookup;
      const candidateLocal = attributeName.includes(":")
        ? attributeName.slice(attributeName.indexOf(":") + 1)
        : attributeName;
      if (candidateLocal.toLowerCase() !== localLookup) return false;
      return namespaceMode === "any" || candidateNamespace === namespaceURI;
    }});
    const actual = candidate === undefined ? null : element.getAttribute(candidate);
    if (!operator) return actual !== null;
    if (actual === null) return operator === "!=";
    const insensitive = caseMode === "i"
      || (caseMode === null
        && defaultAttributeCaseInsensitiveForSelector(element, localName, separator >= 0));
    const left = insensitive ? actual.toLowerCase() : actual;
    const right = insensitive ? expected.toLowerCase() : expected;
    switch (operator) {{
      case "=": return left === right;
      case "!=": return left !== right;
      case "~=": return left.split(/\s+/).includes(right);
      case "|=": return left === right || left.startsWith(right + "-");
      case "^=": return left.startsWith(right);
      case "$=": return left.endsWith(right);
      case "*=": return left.includes(right);
      default: throw new SyntaxError("unsupported attribute operator");
    }}
  }};
  const elementChildren = (element) => element && Array.isArray(element.__glassChildren)
    ? element.__glassChildren.filter((child) => child && child.nodeType === 1)
    : [];
  const descendantsForSelector = (element) => {{
    const result = [];
    const visit = (node) => {{
      for (const child of elementChildren(node)) {{
        result.push(child);
        visit(child);
      }}
    }};
    visit(element);
    return result;
  }};
  const hasForSelector = (element, argument) => {{
    const value = String(argument || "").trim();
    if (!value) throw new SyntaxError(":has() requires a selector");
    const relation = value[0];
    const selector = relation === ">" || relation === "+" || relation === "~"
      ? value.slice(1).trim()
      : value;
    if (!selector) throw new SyntaxError(":has() requires a selector");
    let candidates;
    if (relation === ">") {{
      candidates = elementChildren(element);
    }} else if (relation === "+" || relation === "~") {{
      const siblings = elementChildren(element && element.parentElement);
      const position = siblings.indexOf(element);
      candidates = position < 0
        ? []
        : relation === "+" ? siblings.slice(position + 1, position + 2)
        : siblings.slice(position + 1);
    }} else {{
      candidates = descendantsForSelector(element);
    }}
    return candidates.some((candidate) => matchesSelector(candidate, selector, element));
  }};
  const languageMatchesForSelector = (element, argument) => {{
    let range = String(argument || "").trim().toLowerCase();
    if ((range.startsWith('"') && range.endsWith('"'))
        || (range.startsWith("'") && range.endsWith("'"))) {{
      range = range.slice(1, -1);
    }}
    if (!range) return false;
    let current = element;
    while (current) {{
      const language = current.getAttribute("lang");
      const xmlLanguage = current.getAttribute("xml:lang");
      const value = language !== null ? language : xmlLanguage;
      if (value !== null) {{
        const normalized = value.trim().toLowerCase();
        if (!normalized) return false;
        if (range === "*") return true;
        const base = range.endsWith("-*") ? range.slice(0, -2) : range;
        return normalized === base || normalized.startsWith(base + "-");
      }}
      current = current.parentElement;
    }}
    return false;
  }};
  const directionMatchesForSelector = (element, argument) => {{
    const expected = String(argument || "").trim().toLowerCase();
    if (expected !== "ltr" && expected !== "rtl") return false;
    let current = element;
    while (current) {{
      const value = current.getAttribute("dir");
      if (value !== null) return value.trim().toLowerCase() === expected;
      current = current.parentElement;
    }}
    return expected === "ltr";
  }};
  const sameTypeSiblingsForSelector = (element) => {{
    const siblings = elementChildren(element && element.parentElement);
    return siblings.filter((candidate) => candidate.tagName === element.tagName);
  }};
  const nthFormulaMatchesForSelector = (position, argument) => {{
    const value = String(argument || "").replace(/\s+/g, "").toLowerCase();
    if (value === "odd") return position % 2 === 1;
    if (value === "even") return position % 2 === 0;
    if (/^[+-]?\d+$/.test(value)) return position === Number(value);
    const nIndex = value.indexOf("n");
    if (nIndex < 0 || value.indexOf("n", nIndex + 1) >= 0) return false;
    const coefficientText = value.slice(0, nIndex);
    const offsetText = value.slice(nIndex + 1);
    const coefficient = coefficientText === "" || coefficientText === "+"
      ? 1
      : coefficientText === "-" ? -1 : Number(coefficientText);
    const offset = offsetText === "" ? 0 : Number(offsetText);
    if (!Number.isInteger(coefficient) || !Number.isInteger(offset)) return false;
    if (coefficient === 0) return position === offset;
    const distance = position - offset;
    return distance / coefficient >= 0 && distance % Math.abs(coefficient) === 0;
  }};
  const nthArgumentForSelector = (argument) => {{
    const value = String(argument || "").trim();
    let depth = 0;
    let quote = "";
    for (let index = 0; index <= value.length - 2; index += 1) {{
      const character = value[index];
      if (quote) {{
        if (character === quote && value[index - 1] !== "\\") quote = "";
        continue;
      }}
      if (character === "'" || character === '"') {{
        quote = character;
        continue;
      }}
      if (character === "[") {{
        depth += 1;
        continue;
      }}
      if (character === "]") {{
        depth -= 1;
        continue;
      }}
      const isOfKeyword = value.slice(index, index + 2).toLowerCase() === "of";
      const hasBeforeBoundary = index === 0 || /\s/.test(value[index - 1]);
      const hasAfterBoundary = index + 2 === value.length || /\s/.test(value[index + 2]);
      if (depth !== 0 || !isOfKeyword || !hasBeforeBoundary || !hasAfterBoundary) continue;
      const formula = value.slice(0, index).trim();
      const filter = value.slice(index + 2).trim();
      if (!formula || !filter) throw new SyntaxError("invalid nth-child selector");
      return {{ formula, filter }};
    }}
    return {{ formula: value, filter: null }};
  }};
  const nthSiblingsForSelector = (element, argument) => {{
    const parsed = nthArgumentForSelector(argument);
    const siblings = elementChildren(element && element.parentElement);
    if (!parsed.filter) return {{ siblings, formula: parsed.formula }};
    return {{
      siblings: siblings.filter((candidate) =>
        matchesSelector(candidate, parsed.filter, candidate)),
      formula: parsed.formula,
    }};
  }};
  const formValidityForSelector = (element) => {{
    if (!element) return null;
    if (element.tagName === "FORM") return Boolean(element.validity && element.validity.valid);
    if (!element.willValidate) return null;
    return Boolean(element.validity && element.validity.valid);
  }};
  const editableForSelector = (element) => {{
    if (!element) return false;
    const contentEditable = element.getAttribute("contenteditable");
    if (contentEditable !== null) {{
      if (contentEditable === "" || contentEditable.toLowerCase() === "true") return true;
      if (contentEditable.toLowerCase() === "false") return false;
    }}
    if (element.disabled || element.getAttribute("readonly") !== null) return false;
    if (element.tagName === "TEXTAREA" || element.tagName === "SELECT") return true;
    if (element.tagName !== "INPUT") return false;
    const type = String(element.getAttribute("type") || "text").toLowerCase();
    return !["button", "checkbox", "color", "file", "hidden", "image",
      "radio", "range", "reset", "submit"].includes(type);
  }};
  const placeholderShownForSelector = (element) =>
    (element.tagName === "INPUT" || element.tagName === "TEXTAREA")
      && element.getAttribute("placeholder") !== null
      && String(element.value || "").length === 0;
  const defaultForSelector = (element) =>
    ((element.tagName === "INPUT"
      && ["checkbox", "radio"].includes(String(element.getAttribute("type") || "text").toLowerCase())
      && element.getAttribute("checked") !== null)
      || (element.tagName === "OPTION" && element.getAttribute("selected") !== null));
  const focusedWithinForSelector = (element) =>
    Boolean(element.focused) || descendantsInTree(element, (candidate) => Boolean(candidate.focused)).length > 0;
  const linkForSelector = (element) =>
    ["A", "AREA"].includes(element && element.tagName)
      && element.getAttribute("href") !== null;
  const targetForSelector = (element) => {{
    if (!element || !element.ownerDocument) return false;
    const source = String(element.ownerDocument.URL || "");
    const hash = source.indexOf("#");
    if (hash < 0 || hash === source.length - 1) return false;
    let fragment = source.slice(hash + 1);
    try {{ fragment = decodeURIComponent(fragment); }} catch (_error) {{ return false; }}
    return element.id === fragment
      || (["A", "AREA"].includes(element.tagName) && element.getAttribute("name") === fragment);
  }};
  const rangeForSelector = (element) => {{
    if (!element || element.tagName !== "INPUT" || !element.willValidate) return null;
    const type = String(element.getAttribute("type") || "text").toLowerCase();
    if (!["number", "range", "date", "month", "time", "datetime-local"].includes(type)) return null;
    if (String(element.value || "").length === 0) return null;
    const min = element.getAttribute("min");
    const max = element.getAttribute("max");
    const numericConstraint = (value) => value !== null && value.trim() !== ""
      && Number.isFinite(Number(value));
    const temporalConstraint = (value) => value !== null && value.trim() !== ""
      && /^[-0-9T:.]+$/.test(value);
    const validConstraint = type === "number" || type === "range"
      ? numericConstraint(min) || numericConstraint(max)
      : temporalConstraint(min) || temporalConstraint(max);
    if (!validConstraint) return null;
    const validity = element.validity || {{}};
    return !validity.rangeUnderflow && !validity.rangeOverflow;
  }};
  const indeterminateForSelector = (element) => {{
    if (!element) return false;
    if (element.tagName === "PROGRESS") {{
      const value = element.getAttribute("value");
      if (value === null || value.trim() === "") return true;
      const parsed = Number(value);
      return !Number.isFinite(parsed) || parsed < 0;
    }}
    if (element.tagName !== "INPUT"
        || String(element.getAttribute("type") || "text").toLowerCase() !== "radio") return false;
    const name = element.getAttribute("name");
    return !liveDocumentElements().some((candidate) =>
      candidate.tagName === "INPUT"
      && String(candidate.getAttribute("type") || "text").toLowerCase() === "radio"
      && candidate.formOwnerIndex === element.formOwnerIndex
      && candidate.getAttribute("name") === name
      && candidate.checked);
  }};
  const parsePseudoSelector = (selector) => {{
    const nameMatch = String(selector).match(/^:([A-Za-z-]+)/);
    if (!nameMatch) return null;
    const name = nameMatch[1];
    const opening = nameMatch[0].length;
    if (selector[opening] !== "(") {{
      return {{ name, argument: undefined, length: opening }};
    }}
    let depth = 1;
    let bracketDepth = 0;
    let quote = "";
    for (let index = opening + 1; index < selector.length; index += 1) {{
      const character = selector[index];
      if (quote) {{
        if (character === quote && selector[index - 1] !== "\\") quote = "";
        continue;
      }}
      if (character === "'" || character === '"') {{
        quote = character;
        continue;
      }}
      if (character === "[") {{
        bracketDepth += 1;
        continue;
      }}
      if (character === "]" && bracketDepth > 0) {{
        bracketDepth -= 1;
        continue;
      }}
      if (bracketDepth !== 0) continue;
      if (character === "(") {{
        depth += 1;
      }} else if (character === ")") {{
        depth -= 1;
        if (depth === 0) {{
          return {{
            name,
            argument: selector.slice(opening + 1, index),
            length: index + 1,
          }};
        }}
      }}
    }}
    throw new SyntaxError("unclosed pseudo-class");
  }};
  const matchesSimpleSelector = (element, selector, scope = element) => {{
    if (!element || element.nodeType !== 1) return false;
    let rest = String(selector).trim();
    const tag = rest.match(/^([A-Za-z][A-Za-z0-9_-]*|\*)/);
    if (tag) {{
      if (tag[1] !== "*" && element.tagName.toLowerCase() !== tag[1].toLowerCase()) return false;
      rest = rest.slice(tag[0].length);
    }}
    while (rest.length > 0) {{
      if (rest[0] === "#") {{
        const match = rest.slice(1).match(/^[A-Za-z0-9_\-:]+/);
        if (!match || element.id !== match[0]) return false;
        rest = rest.slice(match[0].length + 1);
      }} else if (rest[0] === ".") {{
        const match = rest.slice(1).match(/^[A-Za-z0-9_\-]+/);
        if (!match || !String(element.className).split(/\s+/).includes(match[0])) return false;
        rest = rest.slice(match[0].length + 1);
      }} else if (rest[0] === "[") {{
        const end = rest.indexOf("]");
        if (end < 0) throw new SyntaxError("unclosed attribute selector");
        if (!selectorAttribute(element, rest.slice(1, end))) return false;
        rest = rest.slice(end + 1);
      }} else if (rest[0] === ":") {{
        const pseudo = parsePseudoSelector(rest);
        if (!pseudo) throw new SyntaxError("invalid pseudo-class");
        const name = pseudo.name.toLowerCase();
        const argument = pseudo.argument;
        const siblings = elementChildren(element.parentElement);
        const position = siblings.indexOf(element) + 1;
        const typeSiblings = sameTypeSiblingsForSelector(element);
        const typePosition = typeSiblings.indexOf(element) + 1;
        const nthFiltered = name === "nth-child" || name === "nth-last-child"
          ? nthSiblingsForSelector(element, argument)
          : null;
        const nthSiblings = nthFiltered ? nthFiltered.siblings : siblings;
        const nthPosition = nthSiblings.indexOf(element) + 1;
        if (name === "has") {{
          if (argument === undefined || !hasForSelector(element, argument)) return false;
        }} else if (name === "lang" && !languageMatchesForSelector(element, argument)) return false;
        else if (name === "dir" && !directionMatchesForSelector(element, argument)) return false;
        else if (name === "not") {{
          if (argument === undefined || matchesSelector(element, argument, scope)) return false;
        }} else if (name === "is" || name === "where") {{
          if (argument === undefined || !matchesSelector(element, argument, scope)) return false;
        }} else if (name === "first-child" && position !== 1) return false;
        else if (name === "last-child" && position !== siblings.length) return false;
        else if (name === "only-child" && siblings.length !== 1) return false;
        else if (name === "nth-child"
            && (nthPosition < 1
              || !nthFormulaMatchesForSelector(nthPosition, nthFiltered.formula))) return false;
        else if (name === "first-of-type" && typePosition !== 1) return false;
        else if (name === "last-of-type" && typePosition !== typeSiblings.length) return false;
        else if (name === "only-of-type" && typeSiblings.length !== 1) return false;
        else if (name === "nth-of-type"
            && !nthFormulaMatchesForSelector(typePosition, argument)) return false;
        else if (name === "nth-last-child"
            && (nthPosition < 1
              || !nthFormulaMatchesForSelector(
                nthSiblings.length - nthPosition + 1,
                nthFiltered.formula))) return false;
        else if (name === "nth-last-of-type"
            && !nthFormulaMatchesForSelector(typeSiblings.length - typePosition + 1, argument)) return false;
        else if (name === "scope" && element !== scope) return false;
        else if (name === "root") {{
          if (!element.ownerDocument || element.ownerDocument.documentElement !== element) return false;
        }} else if (name === "empty") {{
          if (Array.isArray(element.__glassChildren)
              && element.__glassChildren.some((child) => child.nodeType === 1
                || String(child.__glassTextValue || "").length > 0)) return false;
        }} else if (name === "checked" && !element.checked) return false;
        else if (name === "selected" && !element.selected) return false;
        else if (name === "disabled" && !element.disabled) return false;
        else if (name === "enabled" && element.disabled) return false;
        else if ((name === "link" || name === "any-link") && !linkForSelector(element)) return false;
        else if (name === "target" && !targetForSelector(element)) return false;
        else if (name === "in-range" && rangeForSelector(element) !== true) return false;
        else if (name === "out-of-range" && rangeForSelector(element) !== false) return false;
        else if (name === "indeterminate" && !indeterminateForSelector(element)) return false;
        else if (name === "hover" && !element.hovered) return false;
        else if (name === "focus" && !element.focused) return false;
        else if (name === "focus-visible" && !element.focused) return false;
        else if (name === "focus-within" && !focusedWithinForSelector(element)) return false;
        else if (name === "valid" && formValidityForSelector(element) !== true) return false;
        else if (name === "invalid" && formValidityForSelector(element) !== false) return false;
        else if (name === "required"
            && (!element.willValidate || element.getAttribute("required") === null)) return false;
        else if (name === "optional"
            && (!element.willValidate || element.getAttribute("required") !== null)) return false;
        else if (name === "read-only" && editableForSelector(element)) return false;
        else if (name === "read-write" && !editableForSelector(element)) return false;
        else if (name === "placeholder-shown" && !placeholderShownForSelector(element)) return false;
        else if (name === "default" && !defaultForSelector(element)) return false;
        else if (name === "user-valid"
            && (!element.userInteracted || formValidityForSelector(element) !== true)) return false;
        else if (name === "user-invalid"
            && (!element.userInteracted || formValidityForSelector(element) !== false)) return false;
        else if (name !== "has" && name !== "lang" && name !== "dir"
            && name !== "not" && name !== "is" && name !== "where"
            && name !== "only-child" && name !== "nth-child"
            && name !== "first-of-type" && name !== "last-of-type"
            && name !== "only-of-type" && name !== "nth-of-type"
            && name !== "nth-last-child" && name !== "nth-last-of-type"
            && name !== "scope" && name !== "root"
            && name !== "empty" && name !== "checked" && name !== "selected"
            && name !== "disabled" && name !== "enabled"
            && name !== "link" && name !== "any-link" && name !== "target"
            && name !== "in-range" && name !== "out-of-range" && name !== "indeterminate"
            && name !== "hover"
            && name !== "focus" && name !== "focus-visible" && name !== "focus-within"
            && name !== "valid" && name !== "invalid"
            && name !== "required" && name !== "optional"
            && name !== "read-only" && name !== "read-write"
            && name !== "placeholder-shown" && name !== "default"
            && name !== "user-valid" && name !== "user-invalid") {{
          throw new SyntaxError("unsupported pseudo-class");
        }}
        rest = rest.slice(pseudo.length);
      }} else {{
        throw new SyntaxError("invalid selector token");
      }}
    }}
    return true;
  }};
  const matchesSelector = (element, selector, scope = element) => {{
    return splitSelectorList(selector).some((member) => {{
    const chain = parseSelectorChain(member);
    const visit = (candidate, index) => {{
      if (!candidate || !matchesSimpleSelector(candidate, chain[index].value, scope)) return false;
      if (index === 0) return true;
      const combinator = chain[index - 1].value;
      if (combinator === ">") return visit(candidate.parentElement, index - 2);
      const siblings = elementChildren(candidate.parentElement);
      const position = siblings.indexOf(candidate);
      if (combinator === "+") {{
        return position > 0 && visit(siblings[position - 1], index - 2);
      }}
      if (combinator === "~") {{
        for (let previous = position - 1; previous >= 0; previous -= 1) {{
          if (visit(siblings[previous], index - 2)) return true;
        }}
        return false;
      }}
      if (combinator !== " ") return false;
      let ancestor = candidate.parentElement;
      while (ancestor) {{
        if (visit(ancestor, index - 2)) return true;
        ancestor = ancestor.parentElement;
      }}
      return false;
    }};
    return visit(element, chain.length - 1);
  }});
  }};
  const isDescendantOf = (element, owner) => {{
    let parent = element && element.parentElement;
    while (parent) {{
      if (parent === owner) return true;
      parent = parent.parentElement;
    }}
    return false;
  }};
  const descendantsMatching = (owner, candidates, selector) =>
    candidates.filter((element) => isDescendantOf(element, owner) && matchesSelector(element, selector, owner));
  const classToken = (value) => {{
    const token = String(value);
    if (!token || /\s/.test(token)) throw new TypeError("class token must be non-empty and whitespace-free");
    return token;
  }};
  const makeClassList = (element) => {{
    const tokens = () => String(element.className || "").split(/\s+/).filter(Boolean);
    const write = (next) => element.setAttribute("class", Array.from(new Set(next)).join(" "));
    const api = {{
      get length() {{ return tokens().length; }},
      item(index) {{
        const value = tokens()[Number(index)];
        return value === undefined ? null : value;
      }},
      contains(value) {{ return tokens().includes(classToken(value)); }},
      add(...values) {{
        const next = tokens();
        for (const value of values) {{
          const token = classToken(value);
          if (!next.includes(token)) next.push(token);
        }}
        write(next);
      }},
      remove(...values) {{
        const removed = new Set(values.map(classToken));
        write(tokens().filter((token) => !removed.has(token)));
      }},
      toggle(value, force) {{
        const token = classToken(value);
        const present = tokens().includes(token);
        const shouldAdd = force === undefined ? !present : Boolean(force);
        if (shouldAdd && !present) write([...tokens(), token]);
        if (!shouldAdd && present) write(tokens().filter((candidate) => candidate !== token));
        return shouldAdd;
      }},
      replace(oldValue, newValue) {{
        const oldToken = classToken(oldValue);
        const newToken = classToken(newValue);
        const next = tokens();
        const index = next.indexOf(oldToken);
        if (index < 0) return false;
        next[index] = newToken;
        write(next);
        return true;
      }},
      toString() {{ return tokens().join(" "); }},
      get value() {{ return tokens().join(" "); }},
      set value(next) {{ write(String(next).split(/\s+/).filter(Boolean)); }},
    }};
    api[Symbol.iterator] = function() {{ return tokens()[Symbol.iterator](); }};
    const constructor = globalThis.DOMTokenList;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(api, constructor.prototype); }} catch (_error) {{}}
    }}
    return api;
  }};
  const installClassList = (element) => {{
    if (Object.prototype.hasOwnProperty.call(element, "classList")) return;
    let current = null;
    Object.defineProperty(element, "classList", {{
      enumerable: false,
      configurable: false,
      get() {{ return current || (current = makeClassList(element)); }},
    }});
  }};
  const stylePropertyName = (name) => {{
    let value = String(name).trim();
    if (!value) return "";
    if (value === "cssFloat") value = "float";
    if (!value.startsWith("--")) value = value.replace(/[A-Z]/g, (character) => "-" + character.toLowerCase()).toLowerCase();
    return value;
  }};
  const stylePropertyKey = (name) => {{
    const value = String(name);
    if (value.startsWith("--")) return value;
    return value.replace(/-([a-z])/g, (_match, character) => character.toUpperCase());
  }};
  const parseStyleDeclarations = (cssText) => {{
    const declarations = [];
    let part = "";
    let quote = "";
    let parentheses = 0;
    const append = (source) => {{
      const text = String(source).trim();
      if (!text) return;
      const separator = text.indexOf(":");
      if (separator <= 0) return;
      const name = stylePropertyName(text.slice(0, separator));
      if (!name) return;
      let value = text.slice(separator + 1).trim();
      let priority = "";
      if (/\s*!important\s*$/i.test(value)) {{
        priority = "important";
        value = value.replace(/\s*!important\s*$/i, "").trim();
      }}
      if (!value) return;
      const existing = declarations.find((declaration) => declaration.name === name);
      if (existing) {{
        existing.value = value;
        existing.priority = priority;
      }} else {{
        declarations.push({{ name, value, priority }});
      }}
    }};
    const text = String(cssText || "");
    for (let index = 0; index < text.length; index += 1) {{
      const character = text[index];
      if (quote) {{
        part += character;
        if (character === quote && text[index - 1] !== "\\") quote = "";
      }} else if (character === "'" || character === '"') {{
        quote = character;
        part += character;
      }} else if (character === "(") {{
        parentheses += 1;
        part += character;
      }} else if (character === ")" && parentheses > 0) {{
        parentheses -= 1;
        part += character;
      }} else if (character === ";" && parentheses === 0) {{
        append(part);
        part = "";
      }} else {{
        part += character;
      }}
    }}
    append(part);
    return declarations;
  }};
  const serializeStyleDeclarations = (declarations) =>
    declarations.length === 0
      ? ""
      : declarations.map((declaration) =>
        declaration.name + ": " + declaration.value
          + (declaration.priority ? " !important" : "")).join("; ") + ";";
  const makeStyleDeclaration = (element) => {{
    const declarations = () => parseStyleDeclarations(element.getAttribute("style") || "");
    const write = (next) => {{
      const cssText = serializeStyleDeclarations(next);
      if (cssText) element.setAttribute("style", cssText);
      else element.removeAttribute("style");
    }};
    const propertyValue = (name) => {{
      const property = stylePropertyName(name);
      const declaration = declarations().find((candidate) => candidate.name === property);
      return declaration ? declaration.value : "";
    }};
    const api = {{
      get length() {{ return declarations().length; }},
      item(index) {{
        const numeric = Number(index);
        const declaration = Number.isSafeInteger(numeric) && numeric >= 0
          ? declarations()[numeric]
          : null;
        return declaration ? declaration.name : "";
      }},
      getPropertyValue(name) {{ return propertyValue(name); }},
      getPropertyPriority(name) {{
        const property = stylePropertyName(name);
        const declaration = declarations().find((candidate) => candidate.name === property);
        return declaration ? declaration.priority : "";
      }},
      setProperty(name, value, priority = "") {{
        const property = stylePropertyName(name);
        if (!property) return;
        const text = String(value);
        if (!text) {{
          this.removeProperty(property);
          return;
        }}
        const normalizedPriority = String(priority).trim().toLowerCase();
        if (normalizedPriority !== "" && normalizedPriority !== "important") return;
        const next = declarations();
        const existing = next.find((candidate) => candidate.name === property);
        if (existing) {{
          existing.value = text;
          existing.priority = normalizedPriority;
        }} else {{
          next.push({{ name: property, value: text, priority: normalizedPriority }});
        }}
        write(next);
      }},
      removeProperty(name) {{
        const property = stylePropertyName(name);
        const next = declarations();
        const index = next.findIndex((candidate) => candidate.name === property);
        if (index < 0) return "";
        const previous = next[index].value;
        next.splice(index, 1);
        write(next);
        return previous;
      }},
      get cssText() {{ return element.getAttribute("style") || ""; }},
      set cssText(next) {{ write(parseStyleDeclarations(next)); }},
      toString() {{ return this.cssText; }},
    }};
    const proxy = new Proxy(api, {{
      get(target, property, receiver) {{
        const index = collectionIndex(property);
        if (index !== null) return target.item(index) || undefined;
        if (typeof property === "string"
            && !Object.prototype.hasOwnProperty.call(target, property)
            && !property.startsWith("__")) {{
          return propertyValue(property);
        }}
        return Reflect.get(target, property, receiver);
      }},
      set(target, property, value, receiver) {{
        if (property === "cssText") {{
          target.cssText = value;
          return true;
        }}
        if (typeof property === "string"
            && !Object.prototype.hasOwnProperty.call(target, property)
            && !property.startsWith("__")) {{
          target.setProperty(property, value);
          return true;
        }}
        return Reflect.set(target, property, value, receiver);
      }},
      getOwnPropertyDescriptor(target, property) {{
        const index = collectionIndex(property);
        if (index !== null && index < target.length) {{
          return {{ enumerable: true, configurable: true, value: target.item(index), writable: false }};
        }}
        return Reflect.getOwnPropertyDescriptor(target, property);
      }},
    }});
    const constructor = globalThis.CSSStyleDeclaration;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(proxy, constructor.prototype); }} catch (_error) {{}}
    }}
    return proxy;
  }};
  const datasetAttributeName = (key) =>
    "data-" + String(key).replace(/[A-Z]/g, (character) => "-" + character.toLowerCase());
  const datasetKeyFromAttribute = (name) =>
    String(name).slice(5).replace(/-([a-z])/g, (_match, character) => character.toUpperCase());
  const makeDataset = (element) => {{
    const target = Object.create(null);
    const dataNames = () => (typeof element.getAttributeNames === "function"
      ? element.getAttributeNames()
      : []).filter((name) => String(name).toLowerCase().startsWith("data-")
        && String(name).length > 5);
    const proxy = new Proxy(target, {{
      get(_target, property) {{
        if (typeof property !== "string") return undefined;
        const value = element.getAttribute(datasetAttributeName(property));
        return value === null ? undefined : value;
      }},
      set(_target, property, value) {{
        if (typeof property !== "string") return false;
        element.setAttribute(datasetAttributeName(property), String(value));
        return true;
      }},
      has(_target, property) {{
        return typeof property === "string"
          && dataNames().some((name) => datasetKeyFromAttribute(name) === property);
      }},
      ownKeys() {{
        return dataNames().map(datasetKeyFromAttribute);
      }},
      getOwnPropertyDescriptor(_target, property) {{
        if (typeof property !== "string" || !dataNames().some((name) => datasetKeyFromAttribute(name) === property)) {{
          return undefined;
        }}
        return {{
          enumerable: true,
          configurable: true,
          value: element.getAttribute(datasetAttributeName(property)),
          writable: true,
        }};
      }},
      deleteProperty(_target, property) {{
        if (typeof property !== "string") return true;
        element.removeAttribute(datasetAttributeName(property));
        return true;
      }},
      defineProperty(_target, property, descriptor) {{
        if (typeof property !== "string") return false;
        element.setAttribute(datasetAttributeName(property), descriptor.value === undefined ? "" : String(descriptor.value));
        return true;
      }},
    }});
    return proxy;
  }};
  const installElementStyleAndDataset = (element) => {{
    if (!Object.prototype.hasOwnProperty.call(element, "style")) {{
      let current = null;
      Object.defineProperty(element, "style", {{
        enumerable: true,
        configurable: false,
        get() {{ return current || (current = makeStyleDeclaration(element)); }},
      }});
    }}
    if (!Object.prototype.hasOwnProperty.call(element, "dataset")) {{
      let current = null;
      Object.defineProperty(element, "dataset", {{
        enumerable: true,
        configurable: false,
        get() {{ return current || (current = makeDataset(element)); }},
      }});
    }}
  }};
  for (const element of elements) {{
    installClassList(element);
    installElementStyleAndDataset(element);
  }}
  const computedStyleEnumName = (value, fallback = "") => {{
    const kebab = (name) => String(name).replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
    if (typeof value === "string") return kebab(value);
    if (!value || typeof value !== "object") return fallback;
    const keys = Object.keys(value);
    if (keys.length === 0) return fallback;
    const key = keys[0];
    const payload = value[key];
    if (typeof payload === "string") return kebab(payload);
    return kebab(key);
  }};
  const computedStyleFontPalette = (value) => {{
    if (!value || typeof value !== "object") return computedStyleEnumName(value, "normal");
    if (Object.prototype.hasOwnProperty.call(value, "Named")) {{
      const named = value.Named;
      const bytes = named && Array.isArray(named.bytes) ? named.bytes.map((byte) => Number(byte)) : [];
      const length = named ? Number(named.len) : -1;
      if (Number.isInteger(length) && length >= 0 && length <= bytes.length
          && bytes.slice(0, length).every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 127)) {{
        return String.fromCharCode(...bytes.slice(0, length));
      }}
      return "normal";
    }}
    return computedStyleEnumName(value, "normal");
  }};
  const computedStyleFontWeight = (value) => {{
    if (typeof value === "number" && Number.isInteger(value) && value >= 1 && value <= 1000) return String(value);
    if (!value || typeof value !== "object") return computedStyleEnumName(value, "normal");
    const keys = Object.keys(value);
    if (keys.length === 1 && keys[0] === "Numeric") {{
      const number = Number(value.Numeric);
      if (Number.isInteger(number) && number >= 1 && number <= 1000) return String(number);
    }}
    return computedStyleEnumName(value, "normal");
  }};
  const computedStyleEnumPayload = (value) => {{
    if (!value || typeof value !== "object") return value;
    const keys = Object.keys(value);
    return keys.length === 0 ? undefined : value[keys[0]];
  }};
  const computedStyleNumber = (value, fallback = null) => {{
    const payload = computedStyleEnumPayload(value);
    return typeof payload === "number" && Number.isFinite(payload) ? payload : fallback;
  }};
  const computedStyleStretch = (value, fallback = "100%") => {{
    const number = value && typeof value === "object" ? Number(value.min) : Number(value);
    if (!Number.isFinite(number) || number < 500 || number > 2000) return fallback;
    return String(number / 10).replace(/\.0$/, "") + "%";
  }};
  const computedStyleLigatures = (value) => {{
    if (!value || typeof value !== "object") return "normal";
    const common = value.common === true;
    const discretionary = value.discretionary === true;
    const historical = value.historical === true;
    const contextual = value.contextual === true;
    if (common && !discretionary && !historical && contextual) return "normal";
    if (!common && !discretionary && !historical && !contextual) return "none";
    return [
      common ? "common-ligatures" : "no-common-ligatures",
      discretionary ? "discretionary-ligatures" : "no-discretionary-ligatures",
      historical ? "historical-ligatures" : "no-historical-ligatures",
      contextual ? "contextual" : "no-contextual",
    ].join(" ");
  }};
    const computedStyleFontVariantCaps = (value) => computedStyleEnumName(value, "normal");
  const computedStyleFontVariantAlternates = (value) => computedStyleEnumName(value, "normal");
  const computedStyleFontLanguageOverride = (value) => {{
    if (!value || typeof value !== "object" || !Array.isArray(value.tag) || value.tag.length !== 4) return "normal";
    const bytes = value.tag.map((byte) => Number(byte));
    if (!bytes.every((byte) => Number.isInteger(byte) && byte >= 32 && byte <= 126)) return "normal";
    return '"' + String.fromCharCode(...bytes) + '"';
  }};
  const computedStyleFontVariantEastAsian = (value) => {{
    if (!value || typeof value !== "object") return "normal";
    const values = [];
    const form = computedStyleEnumName(value.form, "normal");
    const width = computedStyleEnumName(value.width, "normal");
    if (form !== "normal") values.push(form);
    if (width === "full") values.push("full-width");
    if (width === "proportional") values.push("proportional-width");
    if (value.ruby === true) values.push("ruby");
    return values.length === 0 ? "normal" : values.join(" ");
  }};
  const computedStyleFontVariantNumeric = (value) => {{
    if (!value || typeof value !== "object") return "normal";
    const values = [];
    const figure = computedStyleEnumName(value.figure, "normal");
    const spacing = computedStyleEnumName(value.spacing, "normal");
    const fraction = computedStyleEnumName(value.fraction, "normal");
    if (figure === "lining") values.push("lining-nums");
    if (figure === "oldstyle") values.push("oldstyle-nums");
    if (spacing === "proportional") values.push("proportional-nums");
    if (spacing === "tabular") values.push("tabular-nums");
    if (fraction === "diagonal") values.push("diagonal-fractions");
    if (fraction === "stacked") values.push("stacked-fractions");
    if (value.ordinal === true) values.push("ordinal");
    if (value.slashed_zero === true) values.push("slashed-zero");
    return values.length === 0 ? "normal" : values.join(" ");
  }};
  const computedStyleFontVariant = (raw) => {{
    const values = [];
    const ligatures = computedStyleLigatures(raw.font_variant_ligatures);
    if (ligatures !== "normal") values.push(ligatures);
    const caps = computedStyleFontVariantCaps(raw.font_variant_caps);
    if (caps !== "normal") values.push(caps);
    const position = computedStyleEnumName(raw.font_variant_position, "normal");
    if (position !== "normal") values.push(position);
    const alternates = computedStyleFontVariantAlternates(raw.font_variant_alternates);
    if (alternates !== "normal") values.push(alternates);
    const eastAsian = computedStyleFontVariantEastAsian(raw.font_variant_east_asian);
    if (eastAsian !== "normal") values.push(eastAsian);
    const numeric = computedStyleFontVariantNumeric(raw.font_variant_numeric);
    if (numeric !== "normal") values.push(numeric);
    return values.length === 0 ? "normal" : values.join(" ");
  }};
  const computedStyleFeatureSettings = (value) => {{
    if (!value || typeof value !== "object" || !Array.isArray(value.values)) return "normal";
    const count = Math.max(0, Math.min(value.values.length, Number(value.count) || 0));
    const features = [];
    for (let index = 0; index < count; index += 1) {{
      const feature = value.values[index];
      if (!feature || !Array.isArray(feature.tag) || feature.tag.length !== 4) continue;
      const tag = feature.tag.map((byte) => Number(byte)).every((byte) => Number.isInteger(byte) && byte >= 32 && byte <= 126)
        ? String.fromCharCode(...feature.tag.map((byte) => Number(byte)))
        : null;
      const featureValue = Number(feature.value);
      if (tag === null || !Number.isInteger(featureValue) || featureValue < 0) continue;
      features.push('"' + tag + '" ' + String(featureValue));
    }}
    return features.length === 0 ? "normal" : features.join(", ");
  }};
  const computedStyleVariationSettings = (value) => {{
    if (!value || typeof value !== "object" || !Array.isArray(value.values)) return "normal";
    const count = Math.max(0, Math.min(value.values.length, Number(value.count) || 0));
    const variations = [];
    for (let index = 0; index < count; index += 1) {{
      const variation = value.values[index];
      if (!variation || !Array.isArray(variation.tag) || variation.tag.length !== 4) continue;
      const tagValues = variation.tag.map((byte) => Number(byte));
      const tag = tagValues.every((byte) => Number.isInteger(byte) && byte >= 32 && byte <= 126
        && byte !== 34 && byte !== 39 && byte !== 92 && byte !== 44)
        ? String.fromCharCode(...tagValues)
        : null;
      const valueMilli = Number(variation.value_milli);
      if (tag === null || !Number.isInteger(valueMilli) || !Number.isFinite(valueMilli)
          || Math.abs(valueMilli) > 32768000) continue;
      variations.push('"' + tag + '" ' + String(valueMilli / 1000));
    }}
    return variations.length === 0 ? "normal" : variations.join(", ");
  }};
  const computedStyleColor = (value, fallback = "rgba(0, 0, 0, 0)") => {{
    if (!value || typeof value !== "object") return fallback;
    const red = Number(value.red);
    const green = Number(value.green);
    const blue = Number(value.blue);
    const alpha = Number(value.alpha);
    if (![red, green, blue, alpha].every(Number.isFinite)) return fallback;
    if (alpha >= 255) return "rgb(" + red + ", " + green + ", " + blue + ")";
    return "rgba(" + red + ", " + green + ", " + blue + ", "
      + (alpha / 255).toFixed(3).replace(/0+$/, "").replace(/\.$/, "") + ")";
  }};
  const computedStylePixels = (value, fallback = "auto") => {{
    const number = computedStyleNumber(value);
    return number === null ? fallback : String(number) + "px";
  }};
  const computedStyleEdge = (value, side, fallback = "0px") => {{
    if (!value || typeof value !== "object") return fallback;
    const number = Number(value[side]);
    return Number.isFinite(number) ? String(number) + "px" : fallback;
  }};
  const computedStyleDefaultDisplay = (element) =>
    ["HTML", "BODY", "ADDRESS", "ARTICLE", "ASIDE", "BLOCKQUOTE", "DD", "DIV", "DL", "DT",
      "FIELDSET", "FIGCAPTION", "FIGURE", "FOOTER", "FORM", "H1", "H2", "H3", "H4", "H5",
      "H6", "HEADER", "HR", "LI", "MAIN", "NAV", "OL", "P", "PRE", "SECTION", "TABLE",
      "TR", "UL"].includes(element.tagName)
      ? "block"
      : "inline";
  const computedStyleProperties = [
    "display", "position", "visibility", "opacity", "pointer-events", "z-index",
    "top", "right", "bottom", "left", "width", "height", "min-width", "max-width",
    "min-height", "max-height", "box-sizing", "margin-top", "margin-right", "margin-bottom",
    "margin-left", "padding-top", "padding-right", "padding-bottom", "padding-left",
    "color", "background-color", "background-image", "background-repeat", "background-position",
    "background-size", "border-top-width", "border-right-width", "border-bottom-width",
    "border-left-width", "border-top-style", "border-right-style", "border-bottom-style",
    "border-left-style", "border-top-color", "border-right-color", "border-bottom-color",
    "border-left-color", "border-radius", "overflow", "overflow-x", "overflow-y", "white-space",
    "text-align", "text-align-last", "text-justify", "text-indent", "text-transform", "text-overflow",
    "text-decoration", "text-decoration-style", "text-decoration-thickness", "text-underline-offset",
    "font-weight", "font-style", "font-stretch", "font-variant", "font-variant-ligatures", "font-variant-caps", "font-variant-position", "font-variant-alternates", "font-language-override", "font-variant-east-asian", "font-variant-numeric", "font-feature-settings", "font-variation-settings", "font-kerning", "font-optical-sizing", "line-height", "word-break", "word-spacing", "letter-spacing",
    "font-palette",
    "vertical-align", "flex-direction", "flex-wrap", "flex-grow", "flex-shrink", "flex-basis",
    "justify-content", "align-items", "align-self", "align-content", "gap", "row-gap", "column-gap",
    "order", "grid-template-columns", "grid-template-rows"
  ];
  const computedStyleValue = (element, property, inline) => {{
    const name = stylePropertyName(property);
    const override = inline.find((declaration) => declaration.name === name);
    if (override) return override.value;
    const raw = typeof element.__glassComputedStyle === "function"
      ? element.__glassComputedStyle()
      : {{}};
    const geometry = geometryForNode(element);
    const rawMargins = raw.margin || {{}};
    const rawPadding = raw.padding || {{}};
    const rawMarginAuto = raw.margin_auto || {{}};
    const sideIndex = {{ top: 0, right: 1, bottom: 2, left: 3 }};
    const side = name.match(/(?:margin|padding|border)-(top|right|bottom|left)/);
    if (name === "display") {{
      const display = computedStyleEnumName(raw.display, "auto");
      return display === "auto" ? computedStyleDefaultDisplay(element) : display;
    }}
    if (name === "position") return computedStyleEnumName(raw.position, "static");
    if (name === "visibility") return raw.visibility_hidden ? "hidden" : "visible";
    if (name === "opacity") {{
      const opacity = raw.opacity === null || raw.opacity === undefined ? 1 : Number(raw.opacity) / 255;
      return Number.isFinite(opacity) ? String(Number(opacity.toFixed(3))) : "1";
    }}
    if (name === "pointer-events") return computedStyleEnumName(raw.pointer_events, "auto");
    if (name === "z-index") {{
      const value = computedStyleNumber(raw.z_index);
      return value === null ? "auto" : String(value);
    }}
    if (["top", "right", "bottom", "left"].includes(name)) return computedStylePixels(raw[name]);
    if (name === "width") return String(Number(geometry.contentWidth) || 0) + "px";
    if (name === "height") return String(Number(geometry.contentHeight) || 0) + "px";
    if (name === "min-width") return computedStylePixels(raw.min_width, "0px");
    if (name === "max-width") return computedStylePixels(raw.max_width, "none");
    if (name === "min-height") return computedStylePixels(raw.min_height, "0px");
    if (name === "max-height") return computedStylePixels(raw.max_height, "none");
    if (name === "box-sizing") return computedStyleEnumName(raw.box_sizing, "content-box");
    if (side && side[0].startsWith("margin-")) {{
      const direction = side[1];
      return rawMarginAuto[direction] ? "auto" : computedStyleEdge(rawMargins, direction);
    }}
    if (side && side[0].startsWith("padding-")) return computedStyleEdge(rawPadding, side[1]);
    if (side && name.startsWith("border-") && name.endsWith("-width")) {{
      const values = Array.isArray(raw.border_widths) ? raw.border_widths : [];
      return String(Number(values[sideIndex[side[1]]]) || 0) + "px";
    }}
    if (side && name.startsWith("border-") && name.endsWith("-style")) {{
      const values = Array.isArray(raw.border_styles) ? raw.border_styles : [];
      return computedStyleEnumName(values[sideIndex[side[1]]], "none");
    }}
    if (side && name.startsWith("border-") && name.endsWith("-color")) {{
      const values = Array.isArray(raw.border_colors) ? raw.border_colors : [];
      return computedStyleColor(values[sideIndex[side[1]]]);
    }}
    if (name === "color") return computedStyleColor(raw.color, "rgb(0, 0, 0)");
    if (name === "background-color") return computedStyleColor(raw.background_color);
    if (name === "background-image") return raw.background_image === null || raw.background_image === undefined ? "none" : "url(\"\")";
    if (name === "background-repeat") return computedStyleEnumName(raw.background_repeat, "repeat");
    if (["background-position", "background-size", "border-radius"].includes(name)) return "0px";
    if (name === "overflow") return computedStyleEnumName(raw.overflow_x, "visible") + " " + computedStyleEnumName(raw.overflow_y, "visible");
    if (name === "overflow-x") return computedStyleEnumName(raw.overflow_x, "visible");
    if (name === "overflow-y") return computedStyleEnumName(raw.overflow_y, "visible");
    if (name === "white-space") return computedStyleEnumName(raw.white_space, "normal");
    if (["text-align", "text-align-last", "text-justify", "text-transform", "word-break", "text-overflow", "vertical-align",
      "flex-direction", "flex-wrap", "justify-content", "align-items", "align-self", "align-content", "flex-basis"].includes(name)) {{
      const fields = {{
        "text-align": "text_align", "text-align-last": "text_align_last", "text-justify": "text_justify",
        "text-transform": "text_transform", "word-break": "word_break", "text-overflow": "text_overflow",
        "vertical-align": "vertical_align", "flex-direction": "flex_direction", "flex-wrap": "flex_wrap",
        "justify-content": "justify_content", "align-items": "align_items", "align-self": "align_self",
        "align-content": "align_content", "flex-basis": "flex_basis",
      }};
      return computedStyleEnumName(raw[fields[name]], name === "flex-basis" ? "auto" : "normal");
    }}
    if (name === "text-indent") return String(Number(raw.text_indent) || 0) + "px";
    if (name === "text-decoration") {{
      const flags = computedStyleNumber(raw.text_decoration, 0);
      const lines = [];
      if ((flags & 1) !== 0) lines.push("underline");
      if ((flags & 2) !== 0) lines.push("overline");
      if ((flags & 4) !== 0) lines.push("line-through");
      return lines.length === 0 ? "none" : lines.join(" ");
    }}
    if (name === "text-decoration-style") return computedStyleEnumName(raw.text_decoration_style, "solid");
    if (name === "text-decoration-thickness") return String(Number(raw.text_decoration_thickness) || 0) + "px";
    if (name === "text-underline-offset") return String(Number(raw.text_underline_offset) || 0) + "px";
    if (name === "font-weight") return computedStyleFontWeight(raw.font_weight);
    if (name === "font-style") return computedStyleEnumName(raw.font_style, "normal");
    if (name === "font-stretch") return computedStyleStretch(raw.font_stretch);
    if (name === "font-variant") return computedStyleFontVariant(raw);
    if (name === "font-variant-ligatures") return computedStyleLigatures(raw.font_variant_ligatures);
    if (name === "font-variant-caps") return computedStyleFontVariantCaps(raw.font_variant_caps);
    if (name === "font-variant-position") return computedStyleEnumName(raw.font_variant_position, "normal");
    if (name === "font-variant-alternates") return computedStyleFontVariantAlternates(raw.font_variant_alternates);
    if (name === "font-language-override") return computedStyleFontLanguageOverride(raw.font_language_override);
    if (name === "font-variant-east-asian") return computedStyleFontVariantEastAsian(raw.font_variant_east_asian);
    if (name === "font-variant-numeric") return computedStyleFontVariantNumeric(raw.font_variant_numeric);
    if (name === "font-feature-settings") return computedStyleFeatureSettings(raw.font_feature_settings);
    if (name === "font-variation-settings") return computedStyleVariationSettings(raw.font_variation_settings);
    if (name === "font-kerning") return computedStyleEnumName(raw.font_kerning, "auto");
    if (name === "font-optical-sizing") return computedStyleEnumName(raw.font_optical_sizing, "auto");
    if (name === "font-palette") return computedStyleFontPalette(raw.font_palette);
    if (name === "line-height") return computedStylePixels(raw.line_height, "normal");
    if (name === "word-spacing") return String(Number(raw.word_spacing) || 0) + "px";
    if (name === "letter-spacing") return String(Number(raw.letter_spacing) || 0) + "px";
    if (name === "flex-grow") return String(Number(raw.flex_grow) || 0);
    if (name === "flex-shrink") return String(Number(raw.flex_shrink) || 0);
    if (name === "gap" || name === "row-gap" || name === "column-gap") {{
      const field = name === "gap" ? "gap" : name.replace(/-([a-z])/g, (_match, character) => character.toUpperCase());
      return String(Number(raw[field]) || 0) + "px";
    }}
    if (name === "order") {{
      const order = computedStyleNumber(raw.flex_item_order);
      return order === null ? "0" : String(order);
    }}
    return "";
  }};
  const makeComputedStyle = (element) => {{
    const inline = () => typeof element.__glassInlineStyleAllowed !== "function"
      || element.__glassInlineStyleAllowed()
      ? parseStyleDeclarations(element.getAttribute("style") || "")
      : [];
    const api = {{
      get length() {{ return computedStyleProperties.length; }},
      item(index) {{
        const numeric = Number(index);
        return Number.isSafeInteger(numeric) && numeric >= 0
          ? computedStyleProperties[numeric] || ""
          : "";
      }},
      getPropertyValue(name) {{ return computedStyleValue(element, name, inline()); }},
      getPropertyPriority(_name) {{ return ""; }},
      get cssText() {{ return ""; }},
      toString() {{ return ""; }},
    }};
    return new Proxy(api, {{
      get(target, property, receiver) {{
        const index = collectionIndex(property);
        if (index !== null) return target.item(index);
        if (typeof property === "string"
            && !Object.prototype.hasOwnProperty.call(target, property)
            && !property.startsWith("__")) {{
          return computedStyleValue(element, property, inline());
        }}
        return Reflect.get(target, property, receiver);
      }},
      set(_target, _property, _value) {{ return false; }},
      ownKeys() {{ return computedStyleProperties.slice(); }},
      getOwnPropertyDescriptor(_target, property) {{
        if (typeof property === "string" && computedStyleProperties.includes(property)) {{
          return {{ enumerable: true, configurable: true, value: computedStyleValue(element, property, inline()), writable: false }};
        }}
        return undefined;
      }},
    }});
  }};
  globalThis.getComputedStyle = (element, _pseudoElement) => {{
    if (!element || Number(element.nodeType) !== 1) throw new TypeError("getComputedStyle requires an element");
    return makeComputedStyle(element);
  }};
  const matches = (element, selector, scope = element) => matchesSelector(element, selector, scope);
  const findAll = (selector) => asNodeList(liveDocumentElements().filter((element) => matches(element, selector, documentElement)));
