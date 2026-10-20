pub(in crate::native_engine::javascript) const NATIVE_XML_DOCUMENT_SCRIPT: &str = r###"
  const nativeXmlMaxBytes = __GLASS_XML_MAX_BYTES__;
  const nativeHtmlMaxBytes = __GLASS_HTML_MAX_BYTES__;
  const nativeXmlMaxNodes = __GLASS_XML_MAX_NODES__;
  const nativeXmlMaxDepth = __GLASS_XML_MAX_DEPTH__;
  const nativeXmlNamespace = "http://www.w3.org/2000/xmlns/";
  const nativeXmlNamespaceUri = "http://www.w3.org/XML/1998/namespace";
  const nativeXmlNameStart = (code) => (code >= 65 && code <= 90)
    || (code >= 97 && code <= 122) || code === 95 || code === 58;
  const nativeXmlNamePart = (code) => nativeXmlNameStart(code)
    || (code >= 48 && code <= 57) || code === 45 || code === 46;
  const nativeXmlName = (value, offset = 0) => {
    if (offset >= value.length || !nativeXmlNameStart(value.charCodeAt(offset))) return null;
    let cursor = offset + 1;
    while (cursor < value.length && nativeXmlNamePart(value.charCodeAt(cursor))) cursor += 1;
    return { value: value.slice(offset, cursor), next: cursor };
  };
  const nativeXmlWhitespace = (code) => code === 9 || code === 10 || code === 13 || code === 32;
  const nativeXmlSkipWhitespace = (value, offset) => {
    let cursor = offset;
    while (cursor < value.length && nativeXmlWhitespace(value.charCodeAt(cursor))) cursor += 1;
    return cursor;
  };
  const nativeXmlByteLength = (value) => {
    let bytes = 0;
    for (let cursor = 0; cursor < value.length; cursor += 1) {
      const code = value.codePointAt(cursor);
      if (code <= 0x7f) bytes += 1;
      else if (code <= 0x7ff) bytes += 2;
      else if (code <= 0xffff) bytes += 3;
      else { bytes += 4; cursor += 1; }
      if (bytes > nativeXmlMaxBytes) return bytes;
    }
    return bytes;
  };
  const nativeXmlValidateCharacters = (value) => {
    for (let cursor = 0; cursor < value.length; cursor += 1) {
      const code = value.charCodeAt(cursor);
      if (code >= 0xd800 && code <= 0xdbff) {
        if (cursor + 1 >= value.length) throw new Error("native XML contains an unpaired surrogate");
        const next = value.charCodeAt(cursor + 1);
        if (next < 0xdc00 || next > 0xdfff) throw new Error("native XML contains an unpaired surrogate");
        cursor += 1;
        continue;
      }
      if ((code >= 0xdc00 && code <= 0xdfff)
          || (code < 0x20 && ![9, 10, 13].includes(code))
          || code === 0xfffe || code === 0xffff) {
        throw new Error("native XML contains an invalid character");
      }
    }
  };
  const nativeXmlDecodeEntities = (value) => {
    let output = "";
    let cursor = 0;
    while (cursor < value.length) {
      const ampersand = value.indexOf("&", cursor);
      if (ampersand < 0) {
        output += value.slice(cursor);
        break;
      }
      output += value.slice(cursor, ampersand);
      const semicolon = value.indexOf(";", ampersand + 1);
      if (semicolon < 0) throw new Error("native XML entity is unterminated");
      const entity = value.slice(ampersand + 1, semicolon);
      let decoded = null;
      if (entity === "amp") decoded = "&";
      else if (entity === "lt") decoded = "<";
      else if (entity === "gt") decoded = ">";
      else if (entity === "quot") decoded = '"';
      else if (entity === "apos") decoded = "'";
      else if (/^#x[0-9A-Fa-f]+$/.test(entity)) {
        decoded = String.fromCodePoint(Number.parseInt(entity.slice(2), 16));
      } else if (/^#[0-9]+$/.test(entity)) {
        decoded = String.fromCodePoint(Number.parseInt(entity.slice(1), 10));
      } else throw new Error("native XML entity is unsupported");
      nativeXmlValidateCharacters(decoded);
      output += decoded;
      cursor = semicolon + 1;
    }
    nativeXmlValidateCharacters(output);
    return output;
  };
  const nativeXmlSplitName = (name) => {
    const separator = name.indexOf(":");
    if (separator >= 0 && (separator === 0 || separator === name.length - 1 || name.indexOf(":", separator + 1) >= 0)) {
      throw new Error("native XML qualified name is invalid");
    }
    return separator < 0
      ? { name, prefix: null, localName: name }
      : { name, prefix: name.slice(0, separator), localName: name.slice(separator + 1) };
  };
  const nativeXmlResolveName = (name, namespaces, attribute) => {
    const parts = nativeXmlSplitName(name);
    if (parts.prefix === "xmlns" || (parts.prefix === "xml" && namespaces.get("xml") !== nativeXmlNamespaceUri)) {
      throw new Error("native XML namespace declaration is invalid");
    }
    if (!parts.prefix) return {
      name,
      prefix: null,
      localName: parts.localName,
      namespaceURI: attribute ? null : (namespaces.get("") || null),
    };
    const namespaceURI = namespaces.get(parts.prefix);
    if (!namespaceURI) throw new Error("native XML prefix is undeclared");
    return { name, prefix: parts.prefix, localName: parts.localName, namespaceURI };
  };
  const nativeXmlAddNode = (state, node) => {
    state.nodes += 1;
    if (state.nodes > nativeXmlMaxNodes) throw new Error("native XML node limit exceeded");
    return node;
  };
  const nativeXmlMarkupEnd = (value, offset) => {
    let quote = null;
    let subsetDepth = 0;
    for (let cursor = offset; cursor < value.length; cursor += 1) {
      const character = value[cursor];
      if (quote) {
        if (character === quote) quote = null;
      } else if (character === '"' || character === "'") quote = character;
      else if (character === "[") subsetDepth += 1;
      else if (character === "]" && subsetDepth > 0) subsetDepth -= 1;
      else if (character === ">" && subsetDepth === 0) return cursor;
    }
    return -1;
  };
  const nativeXmlQuoted = (value, offset) => {
    const quote = value[offset];
    if (quote !== '"' && quote !== "'") throw new Error("native XML identifier is not quoted");
    const end = value.indexOf(quote, offset + 1);
    if (end < 0) throw new Error("native XML identifier is unterminated");
    return { value: value.slice(offset + 1, end), next: end + 1 };
  };
  const nativeXmlParseDocument = (source, url, contentType) => {
    try {
      const input = String(source);
      nativeXmlValidateCharacters(input);
      if (nativeXmlByteLength(input) > nativeXmlMaxBytes) throw new Error("native XML response exceeds its limit");
      const state = { input, cursor: 0, nodes: 1, root: null, doctype: null, declaration: false };
      const document = { type: "document", children: [] };
      const append = (parent, node) => {
        node.parent = parent;
        parent.children.push(node);
        return nativeXmlAddNode(state, node);
      };
      const parseDeclaration = () => {
        if (state.declaration || state.cursor !== 0) throw new Error("native XML declaration is misplaced");
        const end = state.input.indexOf("?>", state.cursor + 2);
        if (end < 0) throw new Error("native XML declaration is unterminated");
        const body = state.input.slice(state.cursor + 2, end);
        const target = nativeXmlName(body, 0);
        if (!target || target.value !== "xml") throw new Error("native XML declaration is invalid");
        const rest = body.slice(target.next);
        if (!rest || !/^(?:\s+version\s*=\s*(['"])1\.[0-9]+\1(?:\s+encoding\s*=\s*(['"])[A-Za-z][A-Za-z0-9._-]*\2)?(?:\s+standalone\s*=\s*(['"])(?:yes|no)\3)?\s*)$/.test(rest)) {
          throw new Error("native XML declaration attributes are invalid");
        }
        state.cursor = end + 2;
        state.declaration = true;
      };
      const parseComment = (parent) => {
        const end = state.input.indexOf("-->", state.cursor + 4);
        if (end < 0) throw new Error("native XML comment is unterminated");
        const value = state.input.slice(state.cursor + 4, end);
        if (value.includes("--") || value.endsWith("-")) throw new Error("native XML comment is invalid");
        nativeXmlValidateCharacters(value);
        append(parent, { type: "comment", value });
        state.cursor = end + 3;
      };
      const parseCdata = (parent) => {
        const end = state.input.indexOf("]]>", state.cursor + 9);
        if (end < 0) throw new Error("native XML CDATA is unterminated");
        const value = state.input.slice(state.cursor + 9, end);
        nativeXmlValidateCharacters(value);
        append(parent, { type: "cdata", value });
        state.cursor = end + 3;
      };
      const parseProcessingInstruction = (parent) => {
        const end = state.input.indexOf("?>", state.cursor + 2);
        if (end < 0) throw new Error("native XML processing instruction is unterminated");
        const body = state.input.slice(state.cursor + 2, end);
        const target = nativeXmlName(body, 0);
        if (!target || target.value.toLowerCase() === "xml") throw new Error("native XML processing instruction target is invalid");
        const rest = body.slice(target.next);
        if (rest && !nativeXmlWhitespace(body.charCodeAt(target.next))) {
          throw new Error("native XML processing instruction is malformed");
        }
        const data = rest.replace(/^\s+/, "");
        nativeXmlValidateCharacters(data);
        append(parent, { type: "pi", target: target.value, value: data });
        state.cursor = end + 2;
      };
      const parseDoctype = (parent) => {
        if (parent !== document || state.root || state.doctype) throw new Error("native XML doctype is misplaced");
        const start = state.cursor;
        const end = nativeXmlMarkupEnd(state.input, state.cursor + 9);
        if (end < 0) throw new Error("native XML doctype is unterminated");
        const body = state.input.slice(state.cursor + 9, end);
        let cursor = nativeXmlSkipWhitespace(body, 0);
        const name = nativeXmlName(body, cursor);
        if (!name) throw new Error("native XML doctype name is invalid");
        cursor = nativeXmlSkipWhitespace(body, name.next);
        let publicId = "";
        let systemId = "";
        let internalSubset = "";
        const subsetStart = body.indexOf("[", cursor);
        const external = (subsetStart < 0 ? body.slice(cursor) : body.slice(cursor, subsetStart)).trim();
        if (external) {
          let externalCursor = 0;
          const whitespace = external.search(/\s/);
          const keywordEnd = whitespace < 0 ? external.length : whitespace;
          const keyword = external.slice(0, keywordEnd);
          externalCursor = nativeXmlSkipWhitespace(external, keyword.length);
          if (keyword === "SYSTEM") {
            const quoted = nativeXmlQuoted(external, externalCursor);
            systemId = quoted.value;
            if (external.slice(nativeXmlSkipWhitespace(external, quoted.next)).trim()) throw new Error("native XML system identifier is malformed");
          } else if (keyword === "PUBLIC") {
            const publicValue = nativeXmlQuoted(external, externalCursor);
            publicId = publicValue.value;
            const systemValue = nativeXmlQuoted(external, nativeXmlSkipWhitespace(external, publicValue.next));
            systemId = systemValue.value;
            if (external.slice(nativeXmlSkipWhitespace(external, systemValue.next)).trim()) throw new Error("native XML public identifier is malformed");
          } else throw new Error("native XML external identifier is unsupported");
        }
        if (subsetStart >= 0) {
          const subsetEnd = body.lastIndexOf("]");
          if (subsetEnd < subsetStart || body.slice(subsetEnd + 1).trim()) throw new Error("native XML internal subset is malformed");
          internalSubset = body.slice(subsetStart + 1, subsetEnd);
        }
        nativeXmlValidateCharacters(internalSubset);
        state.doctype = append(parent, {
          type: "doctype",
          name: name.value,
          publicId,
          systemId,
          internalSubset,
          raw: state.input.slice(start, end + 1),
        });
        state.cursor = end + 1;
      };
      const parseElement = (parent, inheritedNamespaces, depth) => {
        if (depth > nativeXmlMaxDepth) throw new Error("native XML depth limit exceeded");
        if (state.input[state.cursor] !== "<") throw new Error("native XML element is malformed");
        state.cursor += 1;
        const parsedName = nativeXmlName(state.input, state.cursor);
        if (!parsedName) throw new Error("native XML element name is invalid");
        state.cursor = parsedName.next;
        const rawAttributes = [];
        const seenAttributes = new Set();
        while (true) {
          state.cursor = nativeXmlSkipWhitespace(state.input, state.cursor);
          if (state.input.startsWith("/>", state.cursor)) {
            state.cursor += 2;
            break;
          }
          if (state.input[state.cursor] === ">") {
            state.cursor += 1;
            break;
          }
          const attribute = nativeXmlName(state.input, state.cursor);
          if (!attribute || seenAttributes.has(attribute.value)) throw new Error("native XML attribute is invalid or duplicated");
          seenAttributes.add(attribute.value);
          state.cursor = nativeXmlSkipWhitespace(state.input, attribute.next);
          if (state.input[state.cursor] !== "=") throw new Error("native XML attribute is missing its equals sign");
          state.cursor = nativeXmlSkipWhitespace(state.input, state.cursor + 1);
          const quote = state.input[state.cursor];
          if (quote !== '"' && quote !== "'") throw new Error("native XML attribute is not quoted");
          const end = state.input.indexOf(quote, state.cursor + 1);
          if (end < 0) throw new Error("native XML attribute is unterminated");
          const value = nativeXmlDecodeEntities(state.input.slice(state.cursor + 1, end));
          rawAttributes.push({ name: attribute.value, value });
          nativeXmlAddNode(state, { type: "attribute" });
          state.cursor = end + 1;
        }
        const namespaces = new Map(inheritedNamespaces);
        namespaces.set("xml", nativeXmlNamespaceUri);
        for (const attribute of rawAttributes) {
          if (attribute.name === "xmlns") {
            namespaces.set("", attribute.value);
          } else if (attribute.name.startsWith("xmlns:")) {
            const prefix = attribute.name.slice(6);
            if (!prefix || prefix === "xmlns" || (prefix === "xml" && attribute.value !== nativeXmlNamespaceUri) || (prefix !== "xml" && attribute.value === nativeXmlNamespaceUri)) {
              throw new Error("native XML namespace binding is invalid");
            }
            if (attribute.value === "") throw new Error("native XML prefixed namespace cannot be empty");
            namespaces.set(prefix, attribute.value);
          }
        }
        const resolvedName = nativeXmlResolveName(parsedName.value, namespaces, false);
        const element = nativeXmlAddNode(state, {
          type: "element",
          name: resolvedName.name,
          prefix: resolvedName.prefix,
          localName: resolvedName.localName,
          namespaceURI: resolvedName.namespaceURI,
          attributes: [],
          children: [],
          parent,
        });
        parent.children.push(element);
        for (const attribute of rawAttributes) {
          const isDefaultNamespace = attribute.name === "xmlns";
          const isPrefixedNamespace = attribute.name.startsWith("xmlns:");
          const resolved = isDefaultNamespace
            ? { name: attribute.name, prefix: null, localName: "xmlns", namespaceURI: nativeXmlNamespace }
            : isPrefixedNamespace
              ? { name: attribute.name, prefix: "xmlns", localName: attribute.name.slice(6), namespaceURI: nativeXmlNamespace }
              : nativeXmlResolveName(attribute.name, namespaces, true);
          element.attributes.push({
            name: resolved.name,
            prefix: resolved.prefix,
            localName: resolved.localName,
            namespaceURI: resolved.namespaceURI,
            value: attribute.value,
            owner: element,
          });
        }
        while (true) {
          if (state.cursor >= state.input.length) throw new Error("native XML element is unclosed");
          if (state.input.startsWith("</", state.cursor)) {
            state.cursor += 2;
            const closeName = nativeXmlName(state.input, state.cursor);
            if (!closeName || closeName.value !== parsedName.value) throw new Error("native XML closing tag does not match");
            state.cursor = nativeXmlSkipWhitespace(state.input, closeName.next);
            if (state.input[state.cursor] !== ">") throw new Error("native XML closing tag is malformed");
            state.cursor += 1;
            break;
          }
          if (state.input[state.cursor] !== "<") {
            const end = state.input.indexOf("<", state.cursor);
            const textEnd = end < 0 ? state.input.length : end;
            const value = nativeXmlDecodeEntities(state.input.slice(state.cursor, textEnd));
            if (value) append(element, { type: "text", value });
            state.cursor = textEnd;
            continue;
          }
          if (state.input.startsWith("<!--", state.cursor)) parseComment(element);
          else if (state.input.startsWith("<![CDATA[", state.cursor)) parseCdata(element);
          else if (state.input.startsWith("<?", state.cursor)) parseProcessingInstruction(element);
          else if (state.input.slice(state.cursor, state.cursor + 9).toUpperCase() === "<!DOCTYPE") parseDoctype(element);
          else if (state.input.startsWith("<!", state.cursor)) throw new Error("native XML declaration is unsupported");
          else parseElement(element, namespaces, depth + 1);
        }
        return element;
      };
      while (state.cursor < input.length) {
        if (input[state.cursor] !== "<") {
          const end = input.indexOf("<", state.cursor);
          const textEnd = end < 0 ? input.length : end;
          if (nativeXmlDecodeEntities(input.slice(state.cursor, textEnd)).trim()) throw new Error("native XML text is outside the root element");
          state.cursor = textEnd;
          continue;
        }
        if (input.startsWith("<?xml", state.cursor)
            && !nativeXmlNamePart(input.charCodeAt(state.cursor + 5))) parseDeclaration();
        else if (input.startsWith("<!--", state.cursor)) parseComment(document);
        else if (input.startsWith("<![CDATA[", state.cursor)) throw new Error("native XML CDATA is outside the root element");
        else if (input.startsWith("<?", state.cursor)) parseProcessingInstruction(document);
        else if (input.slice(state.cursor, state.cursor + 9).toUpperCase() === "<!DOCTYPE") parseDoctype(document);
        else if (input.startsWith("</", state.cursor)) throw new Error("native XML closing tag is unexpected");
        else if (input.startsWith("<!", state.cursor)) throw new Error("native XML declaration is unsupported");
        else {
          if (state.root) throw new Error("native XML has more than one root element");
          state.root = parseElement(document, new Map([["xml", nativeXmlNamespaceUri]]), 1);
        }
      }
      if (!state.root) throw new Error("native XML has no root element");
      if (state.doctype && state.doctype.name !== state.root.name) throw new Error("native XML doctype name does not match the root");
      document.doctype = state.doctype;
      document.url = String(url || "");
      document.contentType = String(contentType || "application/xml").split(";", 1)[0].trim().toLowerCase() || "application/xml";
      return nativeXmlMaterializeDocument(document);
    } catch (_error) {
      return null;
    }
  };
  const nativeXmlIsMime = (value) => {
    const mime = String(value || "").split(";", 1)[0].trim().toLowerCase();
    return ["text/xml", "application/xml", "application/xhtml+xml", "image/svg+xml"].includes(mime)
      || mime.endsWith("+xml");
  };
  const nativeHtmlNamespaceUri = "http://www.w3.org/1999/xhtml";
  const nativeHtmlVoidElements = new Set([
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link",
    "meta", "param", "source", "track", "wbr",
  ]);
  const nativeHtmlRawTextElements = new Set([
    "script", "style", "xmp", "iframe", "noembed", "noframes", "plaintext",
  ]);
  const nativeHtmlParseDocument = (source, url, contentType) => {
    try {
      const sourceText = String(source);
      if (nativeXmlByteLength(sourceText) > nativeHtmlMaxBytes) return null;
      const parseHost = globalThis.__glassParseHtmlDocumentHost;
      const parseJSON = globalThis.__glassParseJSONModule;
      if (typeof parseHost !== "function" || typeof parseJSON !== "function") return null;
      const parsed = parseJSON(String(parseHost(sourceText)));
      if (!parsed || typeof parsed !== "object" || parsed.error
          || !Array.isArray(parsed.nodes) || parsed.nodes.length === 0
          || parsed.nodes.length > nativeXmlMaxNodes * 2
          || !["noQuirks", "limitedQuirks", "quirks"].includes(parsed.quirksMode)
          || !Number.isInteger(parsed.parseErrorCount)
          || parsed.parseErrorCount < 0 || parsed.parseErrorCount > 128) return null;

      const nodes = parsed.nodes;
      const rawDocument = {
        type: "document",
        html: true,
        children: [],
        parent: null,
        url: String(url || ""),
        contentType: String(contentType || "text/html")
          .split(";", 1)[0].trim().toLowerCase() || "text/html",
        compatMode: parsed.quirksMode === "quirks" ? "BackCompat" : "CSS1Compat",
      };
      const rawNodes = new Array(nodes.length);
      const isRecord = (value) => value !== null && typeof value === "object"
        && !Array.isArray(value);
      const isString = (value) => typeof value === "string";
      const validIndex = (value) => Number.isInteger(value)
        && value >= 0 && value < nodes.length;

      for (let index = 0; index < nodes.length; index += 1) {
        const node = nodes[index];
        if (!isRecord(node) || !isRecord(node.kind) || !Array.isArray(node.children)
            || !(node.templateContents === null || validIndex(node.templateContents))
            || !node.children.every(validIndex)) return null;
        const kind = node.kind;
        let raw = null;
        if (kind.type === "document") {
          if (index !== 0) return null;
          raw = rawDocument;
        } else if (kind.type === "documentFragment") {
          raw = {
            type: "fragment",
            html: true,
            htmlDocument: true,
            children: [],
            parent: null,
          };
        } else if (kind.type === "documentType") {
          const data = kind.data;
          if (!isRecord(data) || !isString(data.name) || !isString(data.publicId)
              || !isString(data.systemId)) return null;
          raw = {
            type: "doctype",
            html: true,
            name: data.name,
            publicId: data.publicId,
            systemId: data.systemId,
            internalSubset: "",
            children: [],
            parent: null,
          };
        } else if (kind.type === "element") {
          const data = kind.data;
          if (!isRecord(data) || !isString(data.name) || data.name.length === 0
              || !isString(data.namespace) || data.namespace.length === 0
              || !Array.isArray(data.attributes)) return null;
          const attributes = [];
          for (const attribute of data.attributes) {
            if (!isRecord(attribute) || !isString(attribute.name) || attribute.name.length === 0
                || !isString(attribute.value)
                || !(attribute.namespace === null || isString(attribute.namespace))) return null;
            const colon = attribute.name.indexOf(":");
            if (colon === 0 || colon === attribute.name.length - 1
                || (colon >= 0 && attribute.name.indexOf(":", colon + 1) >= 0)) return null;
            attributes.push({
              name: attribute.name,
              prefix: attribute.namespace === null || colon < 0
                ? null
                : attribute.name.slice(0, colon),
              localName: attribute.namespace === null || colon < 0
                ? attribute.name
                : attribute.name.slice(colon + 1),
              namespaceURI: attribute.namespace,
              value: attribute.value,
              children: [],
              parent: null,
            });
          }
          raw = {
            type: "element",
            html: data.namespace === nativeHtmlNamespaceUri,
            htmlDocument: true,
            name: data.name,
            prefix: null,
            localName: data.name,
            namespaceURI: data.namespace,
            attributes,
            children: [],
            parent: null,
          };
        } else if (kind.type === "text" || kind.type === "comment") {
          if (!isString(kind.data)) return null;
          raw = {
            type: kind.type,
            html: true,
            value: kind.data,
            children: [],
            parent: null,
          };
        } else if (kind.type === "processingInstruction") {
          const data = kind.data;
          if (!isRecord(data) || !isString(data.target) || !isString(data.data)) return null;
          raw = {
            type: "pi",
            html: true,
            target: data.target,
            value: data.data,
            children: [],
            parent: null,
          };
        } else {
          return null;
        }
        rawNodes[index] = raw;
      }
      if (nodes[0].kind.type !== "document" || rawNodes[0] !== rawDocument) return null;

      const visited = new Uint8Array(nodes.length);
      visited[0] = 1;
      let publishedNodeCount = 1;
      const pending = [{ sourceIndex: 0, rawParent: null, depth: 0, fragment: false }];
      while (pending.length > 0) {
        const current = pending.pop();
        const parsedParent = nodes[current.sourceIndex];
        const kind = parsedParent.kind;
        if (current.fragment) {
          if (kind.type !== "documentFragment" || parsedParent.templateContents !== null)
            return null;
        } else if (kind.type === "documentFragment") {
          return null;
        }
        const rawParent = current.fragment ? current.rawParent : rawNodes[current.sourceIndex];
        const isHtmlTemplate = !current.fragment && kind.type === "element"
          && kind.data.namespace === nativeHtmlNamespaceUri && kind.data.name === "template";
        if (!current.fragment && (isHtmlTemplate !== (parsedParent.templateContents !== null)
            || (isHtmlTemplate && parsedParent.children.length !== 0))) return null;

        for (const childIndex of parsedParent.children) {
          if (visited[childIndex]) return null;
          const child = nodes[childIndex];
          if (child.kind.type === "documentFragment") return null;
          const rawChild = rawNodes[childIndex];
          if (!rawChild) return null;
          visited[childIndex] = 1;
          publishedNodeCount += 1;
          if (publishedNodeCount > nativeXmlMaxNodes) return null;
          const depth = current.depth + Number(rawChild.type === "element");
          if (depth > nativeXmlMaxDepth) return null;
          rawParent.children.push(rawChild);
          rawChild.parent = rawParent;
          pending.push({ sourceIndex: childIndex, rawParent: null, depth, fragment: false });
        }

        if (!current.fragment && parsedParent.templateContents !== null) {
          const fragmentIndex = parsedParent.templateContents;
          if (!validIndex(fragmentIndex) || visited[fragmentIndex]
              || nodes[fragmentIndex].kind.type !== "documentFragment") return null;
          const rawTemplate = rawNodes[current.sourceIndex];
          const rawContents = rawNodes[fragmentIndex];
          if (!rawTemplate || rawTemplate.type !== "element"
              || !rawContents || rawContents.type !== "fragment") return null;
          visited[fragmentIndex] = 1;
          publishedNodeCount += 1;
          if (publishedNodeCount > nativeXmlMaxNodes) return null;
          rawTemplate.templateContents = rawContents;
          pending.push({
            sourceIndex: fragmentIndex,
            rawParent: rawContents,
            depth: current.depth,
            fragment: true,
          });
        }
      }
      if (visited.some((value) => value === 0)) return null;
      return nativeXmlMaterializeDocument(rawDocument);
    } catch (_error) {
      return null;
    }
  };
  const nativeHtmlIsMime = (value) => String(value || "").split(";", 1)[0].trim().toLowerCase() === "text/html";
  const nativeXmlNodeList = (values) => {
    const list = Array.from(values || []);
    if (typeof NodeListNative === "function") {
      try { Object.setPrototypeOf(list, NodeListNative.prototype); } catch (_error) {}
    }
    Object.defineProperty(list, "item", { configurable: true, value(index) {
      const numeric = Number(index);
      return Number.isInteger(numeric) && numeric >= 0 ? (list[numeric] || null) : null;
    }});
    return Object.freeze(list);
  };
  const nativeXmlAttributeList = (values, html = false) => {
    const list = Array.from(values || []);
    if (typeof NamedNodeMapNative === "function") {
      try { Object.setPrototypeOf(list, NamedNodeMapNative.prototype); } catch (_error) {}
    }
    Object.defineProperties(list, {
      item: { configurable: true, value(index) {
        const numeric = Number(index);
        return Number.isInteger(numeric) && numeric >= 0 ? (list[numeric] || null) : null;
      }},
      getNamedItem: { configurable: true, value(name) {
        const requested = String(name);
        return Array.from(list).find((attribute) => attribute.name === (html ? requested.toLowerCase() : requested)) || null;
      }},
      getNamedItemNS: { configurable: true, value(namespaceURI, localName) {
        const namespace = namespaceURI === null || namespaceURI === undefined ? null : String(namespaceURI);
        return Array.from(list).find((attribute) => attribute.namespaceURI === namespace && attribute.localName === String(localName)) || null;
      }},
    });
    return Object.freeze(list);
  };
  const nativeXmlEscapeText = (value) => String(value)
    .replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
  const nativeXmlEscapeAttribute = (value) => String(value)
    .replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll('"', "&quot;")
    .replaceAll("\t", "&#x9;").replaceAll("\n", "&#xA;").replaceAll("\r", "&#xD;");
  const nativeXmlDescendants = (node, includeSelf) => {
    const result = [];
    const visit = (candidate) => {
      if (candidate.type === "element") result.push(candidate);
      for (const child of candidate.children || []) visit(child);
    };
    if (includeSelf) visit(node);
    else for (const child of node.children || []) visit(child);
    return result;
  };
  const nativeXmlElementNameMatches = (raw, requested) => {
    const value = String(requested);
    return value === "*" || raw.name === (raw.html ? value.toLowerCase() : value);
  };
  const nativeXmlAttributeNameMatches = (raw, requested) => {
    const value = String(requested);
    return raw.name === (raw.html ? value.toLowerCase() : value);
  };
  const nativeXmlMaterializeDocument = (rawDocument) => {
    const materialized = new Map();
    let templateContentsOwnerDocument = null;
    const getTemplateContentsOwnerDocument = () => {
      if (!templateContentsOwnerDocument) {
        templateContentsOwnerDocument = materializeNode({
          type: "document",
          html: true,
          children: [],
          parent: null,
          url: "",
          contentType: "text/html",
          compatMode: "CSS1Compat",
        }, null);
      }
      return templateContentsOwnerDocument;
    };
    const setPrototype = (target, constructor) => {
      const prototype = constructor && constructor.prototype
        ? constructor.prototype
        : constructor;
      if (prototype && typeof prototype === "object") {
        try { Object.setPrototypeOf(target, prototype); } catch (_error) {}
      }
      return target;
    };
    const common = (target, raw, nodeType, nodeName, constructor, ownerDocument) => {
      setPrototype(target, constructor || NodeNative);
      const parent = raw.parent ? materialized.get(raw.parent) || null : null;
      Object.defineProperties(target, {
        nodeType: { configurable: false, enumerable: true, value: nodeType },
        nodeName: { configurable: false, enumerable: true, value: nodeName },
        nodeValue: { configurable: false, enumerable: true, value: raw.type === "text" || raw.type === "cdata" || raw.type === "comment" || raw.type === "pi" ? raw.value : null },
        __glassXmlRaw: { configurable: false, enumerable: false, value: raw },
        __glassXmlChildren: { configurable: false, enumerable: false, value: [] },
        __glassXmlParent: { configurable: false, enumerable: false, writable: true, value: parent },
        __glassXmlOwnerDocument: { configurable: false, enumerable: false, value: ownerDocument },
        parentNode: { configurable: true, enumerable: true, get() { return this.__glassXmlParent; } },
        parentElement: { configurable: true, enumerable: true, get() {
          return this.__glassXmlParent && this.__glassXmlParent.nodeType === 1 ? this.__glassXmlParent : null;
        }},
        ownerDocument: { configurable: true, enumerable: true, get() { return this.__glassXmlOwnerDocument; } },
        childNodes: { configurable: true, enumerable: true, get() { return nativeXmlNodeList(this.__glassXmlChildren); } },
        children: { configurable: true, enumerable: true, get() {
          return nativeXmlNodeList(this.__glassXmlChildren.filter((child) => child.nodeType === 1));
        }},
        firstChild: { configurable: true, enumerable: true, get() { return this.__glassXmlChildren[0] || null; } },
        lastChild: { configurable: true, enumerable: true, get() { return this.__glassXmlChildren[this.__glassXmlChildren.length - 1] || null; } },
        hasChildNodes: { configurable: true, value() { return this.__glassXmlChildren.length > 0; } },
        getRootNode: { configurable: true, value() {
          let root = this;
          while (root.parentNode) root = root.parentNode;
          return root;
        } },
        contains: { configurable: true, value(other) {
          let candidate = other;
          while (candidate) {
            if (candidate === this) return true;
            candidate = candidate.parentNode;
          }
          return false;
        }},
        isSameNode: { configurable: true, value(other) { return this === other; } },
        textContent: { configurable: true, enumerable: true, get() {
          if ([3, 4, 7, 8].includes(this.nodeType)) return String(this.nodeValue || "");
          return this.__glassXmlChildren.map((child) => child.textContent).join("");
        }},
      });
      for (const method of ["appendChild", "insertBefore", "removeChild", "replaceChild", "replaceChildren", "normalize"])
        Object.defineProperty(target, method, { configurable: true, value() {
          throw new DOMExceptionNative("The native XML response document is read-only", "NoModificationAllowedError");
        }});
      materialized.set(raw, target);
      return target;
    };
    const materializeNode = (raw, ownerDocument) => {
      if (materialized.has(raw)) return materialized.get(raw);
      let target;
      if (raw.type === "document") target = common({}, raw, 9, "#document", DocumentNative, ownerDocument);
      else if (raw.type === "fragment") target = common({}, raw, 11, "#document-fragment", DocumentFragmentNative, ownerDocument);
      else if (raw.type === "element") target = common(
        {}, raw, 1, raw.html ? raw.name.toUpperCase() : raw.name,
        raw.htmlDocument && raw.html
          ? elementPrototypeFor(raw.name.toUpperCase())
          : ElementNative,
        ownerDocument,
      );
      else if (raw.type === "text") target = common({}, raw, 3, "#text", TextNative, ownerDocument);
      else if (raw.type === "cdata") target = common({}, raw, 4, "#cdata-section", CharacterDataNative, ownerDocument);
      else if (raw.type === "comment") target = common({}, raw, 8, "#comment", CommentNative, ownerDocument);
      else if (raw.type === "pi") target = common({}, raw, 7, raw.target, CharacterDataNative, ownerDocument);
      else target = common({}, raw, 10, raw.name, DocumentTypeNative, ownerDocument);
      if (raw.type === "text" || raw.type === "cdata" || raw.type === "comment" || raw.type === "pi") {
        Object.defineProperties(target, {
          data: { configurable: true, enumerable: true, get() { return String(this.nodeValue || ""); } },
          length: { configurable: true, enumerable: true, get() { return String(this.nodeValue || "").length; } },
        });
      }
      if (raw.type === "pi") {
        Object.defineProperty(target, "target", { configurable: true, enumerable: true, value: raw.target });
      }
      if (raw.type === "element") {
        Object.defineProperties(target, {
          tagName: { configurable: true, enumerable: true, value: raw.html ? raw.name.toUpperCase() : raw.name },
          localName: { configurable: true, enumerable: true, value: raw.localName },
          prefix: { configurable: true, enumerable: true, value: raw.prefix },
          namespaceURI: { configurable: true, enumerable: true, value: raw.namespaceURI },
          attributes: { configurable: true, enumerable: true, value: nativeXmlAttributeList(
            raw.attributes.map((attribute) => materializeAttribute(attribute, ownerDocument, target)),
            raw.namespaceURI === nativeHtmlNamespaceUri,
          ) },
        });
        const attributes = target.attributes;
        Object.defineProperties(target, {
          getAttribute: { configurable: true, value(name) {
            const attribute = Array.from(attributes).find((candidate) => nativeXmlAttributeNameMatches(
              { name: candidate.name, html: raw.namespaceURI === nativeHtmlNamespaceUri },
              name,
            ));
            return attribute ? attribute.value : null;
          }},
          getAttributeNS: { configurable: true, value(namespaceURI, localName) {
            const namespace = namespaceURI === null || namespaceURI === undefined ? null : String(namespaceURI);
            const attribute = Array.from(attributes).find((candidate) => candidate.namespaceURI === namespace && candidate.localName === String(localName));
            return attribute ? attribute.value : null;
          }},
          getAttributeNode: { configurable: true, value(name) {
            return attributes.getNamedItem(name);
          }},
          getAttributeNodeNS: { configurable: true, value(namespaceURI, localName) {
            return attributes.getNamedItemNS(namespaceURI, localName);
          }},
          hasAttribute: { configurable: true, value(name) {
            return Array.from(attributes).some((candidate) => nativeXmlAttributeNameMatches(
              { name: candidate.name, html: raw.namespaceURI === nativeHtmlNamespaceUri },
              name,
            ));
          } },
          hasAttributeNS: { configurable: true, value(namespaceURI, localName) {
            const namespace = namespaceURI === null || namespaceURI === undefined ? null : String(namespaceURI);
            return Array.from(attributes).some((candidate) => candidate.namespaceURI === namespace && candidate.localName === String(localName));
          }},
          getAttributeNames: { configurable: true, value() { return Array.from(attributes).map((attribute) => attribute.name); } },
          getElementsByTagName: { configurable: true, value(name) {
            return nativeXmlNodeList(nativeXmlDescendants(raw, false).filter((candidate) => nativeXmlElementNameMatches(candidate, name)).map((candidate) => materialized.get(candidate)));
          }},
          getElementsByTagNameNS: { configurable: true, value(namespaceURI, localName) {
            const namespace = namespaceURI === null || namespaceURI === undefined ? null : String(namespaceURI);
            const requested = String(localName);
            return nativeXmlNodeList(nativeXmlDescendants(raw, false).filter((candidate) => (namespace === "*" || candidate.namespaceURI === namespace) && (requested === "*" || candidate.localName === (candidate.namespaceURI === nativeHtmlNamespaceUri ? requested.toLowerCase() : requested))).map((candidate) => materialized.get(candidate)));
          }},
          querySelector: { configurable: true, value(selector) { return nativeXmlQuery(this, selector, false)[0] || null; } },
          querySelectorAll: { configurable: true, value(selector) { return nativeXmlNodeList(nativeXmlQuery(this, selector, false)); } },
          innerHTML: { configurable: true, enumerable: true, get() {
            const children = raw.templateContents
              ? this.content.__glassXmlChildren
              : this.__glassXmlChildren;
            return children.map((child) => nativeXmlSerialize(child)).join("");
          } },
          outerHTML: { configurable: true, enumerable: true, get() { return nativeXmlSerialize(this); } },
          lookupNamespaceURI: { configurable: true, value(prefix) {
            const requested = prefix === null || prefix === undefined ? "" : String(prefix);
            const attribute = Array.from(attributes).find((candidate) => requested === ""
              ? candidate.name === "xmlns"
              : candidate.name === "xmlns:" + requested);
            return attribute ? attribute.value : (requested === this.prefix ? this.namespaceURI : null);
          }},
        });
        if (raw.html && raw.namespaceURI === nativeHtmlNamespaceUri && raw.name === "template") {
          Object.defineProperty(target, "content", {
            configurable: true,
            enumerable: true,
            get() {
              if (!raw.templateContents) return null;
              return materializeNode(
                raw.templateContents,
                getTemplateContentsOwnerDocument(),
              );
            },
          });
        }
      }
      if (raw.type === "fragment") {
        Object.defineProperties(target, {
          getElementById: { configurable: true, value(id) {
            return nativeXmlDescendants(raw, false)
              .map((candidate) => materialized.get(candidate))
              .find((candidate) => candidate.getAttribute("id") === String(id)) || null;
          } },
          getElementsByTagName: { configurable: true, value(name) {
            return nativeXmlNodeList(nativeXmlDescendants(raw, false)
              .filter((candidate) => nativeXmlElementNameMatches(candidate, name))
              .map((candidate) => materialized.get(candidate)));
          } },
          getElementsByTagNameNS: { configurable: true, value(namespaceURI, localName) {
            const namespace = namespaceURI === null || namespaceURI === undefined ? null : String(namespaceURI);
            const requested = String(localName);
            return nativeXmlNodeList(nativeXmlDescendants(raw, false)
              .filter((candidate) => (namespace === "*" || candidate.namespaceURI === namespace)
                && (requested === "*" || candidate.localName === (
                  candidate.namespaceURI === nativeHtmlNamespaceUri
                    ? requested.toLowerCase()
                    : requested
                )))
              .map((candidate) => materialized.get(candidate)));
          } },
          querySelector: { configurable: true, value(selector) {
            return nativeXmlQuery(this, selector, false)[0] || null;
          } },
          querySelectorAll: { configurable: true, value(selector) {
            return nativeXmlNodeList(nativeXmlQuery(this, selector, false));
          } },
        });
      }
      if (raw.type === "document") {
        Object.defineProperties(target, {
          URL: { configurable: true, enumerable: true, value: raw.url },
          documentURI: { configurable: true, enumerable: true, value: raw.url },
          contentType: { configurable: true, enumerable: true, value: raw.contentType },
          compatMode: { configurable: true, enumerable: true, value: raw.compatMode || "CSS1Compat" },
          characterSet: { configurable: true, enumerable: true, value: "UTF-8" },
          charset: { configurable: true, enumerable: true, value: "UTF-8" },
          inputEncoding: { configurable: true, enumerable: true, value: "UTF-8" },
          xmlEncoding: { configurable: true, enumerable: true, value: "UTF-8" },
          xmlStandalone: { configurable: true, enumerable: true, value: false },
          xmlVersion: { configurable: true, enumerable: true, value: "1.0" },
          documentElement: { configurable: true, enumerable: true, get() { return this.__glassXmlChildren.find((child) => child.nodeType === 1) || null; } },
          head: { configurable: true, enumerable: true, get() {
            if (!raw.html) return undefined;
            const root = this.__glassXmlChildren.find((child) => child.nodeType === 1);
            return root ? root.__glassXmlChildren.find((child) => child.nodeType === 1 && child.__glassXmlRaw.name === "head") || null : null;
          } },
          body: { configurable: true, enumerable: true, get() {
            if (!raw.html) return undefined;
            const root = this.__glassXmlChildren.find((child) => child.nodeType === 1);
            return root ? root.__glassXmlChildren.find((child) => child.nodeType === 1 && child.__glassXmlRaw.name === "body") || null : null;
          } },
          title: { configurable: true, enumerable: true, get() {
            if (!raw.html) return undefined;
            const head = this.head;
            const title = head && head.__glassXmlChildren.find((child) => child.nodeType === 1 && child.__glassXmlRaw.name === "title");
            return title ? title.textContent : "";
          } },
          doctype: { configurable: true, enumerable: true, get() { return this.__glassXmlChildren.find((child) => child.nodeType === 10) || null; } },
          getElementById: { configurable: true, value(id) {
            return nativeXmlDescendants(raw, true).map((candidate) => materialized.get(candidate)).find((candidate) => candidate.getAttribute("id") === String(id)) || null;
          }},
          getElementsByTagName: { configurable: true, value(name) {
            return nativeXmlNodeList(nativeXmlDescendants(raw, true).filter((candidate) => nativeXmlElementNameMatches(candidate, name)).map((candidate) => materialized.get(candidate)));
          }},
          getElementsByTagNameNS: { configurable: true, value(namespaceURI, localName) {
            const namespace = namespaceURI === null || namespaceURI === undefined ? null : String(namespaceURI);
            const requested = String(localName);
            return nativeXmlNodeList(nativeXmlDescendants(raw, true).filter((candidate) => (namespace === "*" || candidate.namespaceURI === namespace) && (requested === "*" || candidate.localName === (candidate.html ? requested.toLowerCase() : requested))).map((candidate) => materialized.get(candidate)));
          }},
          querySelector: { configurable: true, value(selector) { return nativeXmlQuery(this, selector, true)[0] || null; } },
          querySelectorAll: { configurable: true, value(selector) { return nativeXmlNodeList(nativeXmlQuery(this, selector, true)); } },
        });
      }
      if (raw.type === "doctype") {
        Object.defineProperties(target, {
          name: { configurable: true, enumerable: true, value: raw.name },
          publicId: { configurable: true, enumerable: true, value: raw.publicId },
          systemId: { configurable: true, enumerable: true, value: raw.systemId },
          internalSubset: { configurable: true, enumerable: true, value: raw.internalSubset },
          entities: { configurable: true, enumerable: true, value: nativeXmlNodeList([]) },
          notations: { configurable: true, enumerable: true, value: nativeXmlNodeList([]) },
        });
      }
      for (const child of raw.children || []) {
        const materializedChild = materializeNode(child, ownerDocument || target);
        target.__glassXmlChildren.push(materializedChild);
      }
      Object.freeze(target.__glassXmlChildren);
      return Object.freeze(target);
    };
    const materializeAttribute = (raw, ownerDocument, ownerElement) => {
      const target = setPrototype({}, AttrNative);
      Object.defineProperties(target, {
        nodeType: { configurable: false, enumerable: true, value: 2 },
        nodeName: { configurable: false, enumerable: true, value: raw.name },
        nodeValue: { configurable: false, enumerable: true, value: raw.value },
        name: { configurable: true, enumerable: true, value: raw.name },
        localName: { configurable: true, enumerable: true, value: raw.localName },
        prefix: { configurable: true, enumerable: true, value: raw.prefix },
        namespaceURI: { configurable: true, enumerable: true, value: raw.namespaceURI },
        value: { configurable: true, enumerable: true, value: raw.value },
        specified: { configurable: true, enumerable: true, value: true },
        ownerElement: { configurable: true, enumerable: true, value: ownerElement || null },
        ownerDocument: { configurable: true, enumerable: true, value: ownerDocument },
        parentNode: { configurable: true, enumerable: true, value: null },
        childNodes: { configurable: true, enumerable: true, value: nativeXmlNodeList([]) },
      });
      return Object.freeze(target);
    };
    const nativeXmlMatchSelector = (node, selector) => {
      const value = String(selector).trim();
      if (!value || /[\s>+~,]/.test(value)) return false;
      const attributeMatch = value.match(/^(.*)\[([^=\]]+)(?:=([^\]]+))?\]$/);
      const tag = attributeMatch ? attributeMatch[1] : value;
      const raw = node.__glassXmlRaw;
      const normalizedTag = raw && raw.html ? tag.toLowerCase() : tag;
      if (normalizedTag && normalizedTag !== "*" && !normalizedTag.startsWith("#") && !normalizedTag.startsWith(".")
          && (!raw || raw.name !== normalizedTag)) return false;
      if (tag.startsWith("#") && node.getAttribute("id") !== tag.slice(1)) return false;
      if (tag.startsWith(".") && !String(node.getAttribute("class") || "").split(/\s+/).includes(tag.slice(1))) return false;
      if (attributeMatch) {
        const name = raw && raw.html ? attributeMatch[2].trim().toLowerCase() : attributeMatch[2].trim();
        const expected = attributeMatch[3] === undefined ? null : attributeMatch[3].trim().replace(/^(["'])(.*)\1$/, "$2");
        const actual = node.getAttribute(name);
        if (actual === null || (expected !== null && actual !== expected)) return false;
      }
      return true;
    };
    const nativeXmlQuery = (source, selector, includeRoot) => {
      const candidates = nativeXmlDescendants(source.__glassXmlRaw, includeRoot)
        .map((candidate) => materialized.get(candidate));
      return candidates.filter((candidate) => nativeXmlMatchSelector(candidate, selector));
    };
    const nativeXmlSerialize = (node) => {
      if (!node) return "";
      if (node.nodeType === 9) return node.childNodes.map((child) => nativeXmlSerialize(child)).join("");
      if (node.nodeType === 11) return node.childNodes.map((child) => nativeXmlSerialize(child)).join("");
      if (node.nodeType === 1) {
        const raw = node.__glassXmlRaw;
        const attributes = Array.from(node.attributes).map((attribute) => " " + attribute.name + "=\"" + nativeXmlEscapeAttribute(attribute.value) + "\"").join("");
        const childNodes = raw && raw.templateContents
          ? node.content.childNodes
          : node.childNodes;
        const children = childNodes.map((child) => nativeXmlSerialize(child)).join("");
        if (raw && raw.html && raw.namespaceURI === nativeHtmlNamespaceUri
            && nativeHtmlVoidElements.has(raw.name)) return "<" + raw.name + attributes + ">";
        const name = raw && raw.html ? raw.name : node.nodeName;
        if (!raw || !raw.htmlDocument) return children ? "<" + name + attributes + ">" + children + "</" + name + ">" : "<" + name + attributes + "/>";
        return "<" + name + attributes + ">" + children + "</" + name + ">";
      }
      if (node.nodeType === 3) {
        const raw = node.__glassXmlRaw;
        const parent = raw && raw.parent;
        if (parent && parent.html && nativeHtmlRawTextElements.has(parent.name)) return String(node.nodeValue || "");
        return nativeXmlEscapeText(node.nodeValue || "");
      }
      if (node.nodeType === 4) return "<![CDATA[" + String(node.nodeValue || "") + "]]>";
      if (node.nodeType === 7) return "<?" + node.target + (node.data ? " " + node.data : "") + "?>";
      if (node.nodeType === 8) return "<!--" + String(node.nodeValue || "") + "-->";
      if (node.nodeType === 10) return node.__glassXmlRaw.html
        ? "<!DOCTYPE " + node.__glassXmlRaw.name + ">"
        : node.__glassXmlRaw.raw;
      return "";
    };
    globalThis.__glassSerializeXmlNode = nativeXmlSerialize;
    return materializeNode(rawDocument, null);
  };
  globalThis.__glassParseXmlDocument = nativeXmlParseDocument;
  globalThis.__glassIsXmlMime = nativeXmlIsMime;
  globalThis.__glassParseHtmlDocument = nativeHtmlParseDocument;
  globalThis.__glassIsHtmlMime = nativeHtmlIsMime;
  const NativeXMLSerializer = function XMLSerializer() {
    if (!(this instanceof NativeXMLSerializer)) throw new TypeError("XMLSerializer requires new");
  };
  NativeXMLSerializer.prototype.serializeToString = function(node) {
    if (!node || ![1, 4, 7, 8, 9, 10, 11].includes(Number(node.nodeType))) throw new TypeError("XMLSerializer target is not an XML node");
    return globalThis.__glassSerializeXmlNode(node);
  };
  globalThis.XMLSerializer = NativeXMLSerializer;
"###;
