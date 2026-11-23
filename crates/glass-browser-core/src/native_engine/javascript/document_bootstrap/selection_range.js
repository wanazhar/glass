  const selectionDomError = (message, name) => {{
    const Constructor = globalThis.DOMException;
    return typeof Constructor === "function" ? new Constructor(message, name) : new Error(message);
  }};
  const selectionNodeLength = (node) => {{
    if (!node) return 0;
    const type = Number(node.nodeType);
    if (type === 3 || type === 4 || type === 7 || type === 8) return String(node.nodeValue ?? node.data ?? "").length;
    return node.childNodes ? node.childNodes.length : 0;
  }};
  const selectionCheckBoundary = (node, offset) => {{
    if (!node || typeof node.nodeType !== "number") throw new TypeError("Range boundary must be a Node");
    if (Number(node.nodeType) === 10) throw selectionDomError("Range boundary cannot be a doctype", "InvalidNodeTypeError");
    const value = Number(offset) >>> 0;
    if (value > selectionNodeLength(node)) throw selectionDomError("Range offset exceeds the node length", "IndexSizeError");
    return value;
  }};
  const selectionIndexInParent = (node) => {{
    const parent = node && node.parentNode;
    if (!parent || !parent.childNodes) return 0;
    return Array.prototype.indexOf.call(parent.childNodes, node);
  }};
  const selectionAncestors = (node) => {{
    const values = [];
    for (let current = node; current; current = current.parentNode) values.push(current);
    return values;
  }};
  const selectionCommonAncestor = (left, right) => {{
    const rightAncestors = new Set(selectionAncestors(right));
    for (const candidate of selectionAncestors(left)) if (rightAncestors.has(candidate)) return candidate;
    return left;
  }};
  const selectionComparePoints = (leftNode, leftOffset, rightNode, rightOffset) => {{
    if (leftNode === rightNode) return leftOffset === rightOffset ? 0 : leftOffset < rightOffset ? -1 : 1;
    const leftPath = selectionAncestors(leftNode).reverse();
    const rightPath = selectionAncestors(rightNode).reverse();
    let depth = 0;
    while (depth < leftPath.length && depth < rightPath.length && leftPath[depth] === rightPath[depth]) depth += 1;
    if (depth === 0) return 0;
    if (depth === leftPath.length) {{
      const child = rightPath[depth];
      return leftOffset <= selectionIndexInParent(child) ? -1 : 1;
    }}
    if (depth === rightPath.length) {{
      const child = leftPath[depth];
      return selectionIndexInParent(child) < rightOffset ? -1 : 1;
    }}
    return selectionIndexInParent(leftPath[depth]) < selectionIndexInParent(rightPath[depth]) ? -1 : 1;
  }};
  const selectionElementFor = (node) => {{
    let current = node;
    while (current && Number(current.nodeType) !== 1) current = current.parentNode;
    return current || null;
  }};
  const selectionRect = (node) => {{
    const element = selectionElementFor(node);
    const rect = element && typeof element.getBoundingClientRect === "function" ? element.getBoundingClientRect() : null;
    const x = rect ? Number(rect.x ?? rect.left) || 0 : 0;
    const y = rect ? Number(rect.y ?? rect.top) || 0 : 0;
    const width = rect ? Number(rect.width) || 0 : 0;
    const height = rect ? Number(rect.height) || 0 : 0;
    if (typeof DOMRect === "function") return new DOMRect(x, y, width, height);
    return {{ x, y, width, height, left: x, top: y, right: x + width, bottom: y + height, toJSON() {{ return {{ x, y, width, height }}; }} }};
  }};
  class RangeNative {{
    constructor() {{
      this.startContainer = document;
      this.startOffset = 0;
      this.endContainer = document;
      this.endOffset = 0;
    }}
    get collapsed() {{ return this.startContainer === this.endContainer && this.startOffset === this.endOffset; }}
    get commonAncestorContainer() {{ return selectionCommonAncestor(this.startContainer, this.endContainer); }}
    setStart(node, offset) {{
      const value = selectionCheckBoundary(node, offset);
      this.startContainer = node;
      this.startOffset = value;
      if (selectionComparePoints(node, value, this.endContainer, this.endOffset) > 0) this.collapse(true);
    }}
    setEnd(node, offset) {{
      const value = selectionCheckBoundary(node, offset);
      this.endContainer = node;
      this.endOffset = value;
      if (selectionComparePoints(this.startContainer, this.startOffset, node, value) > 0) this.collapse(false);
    }}
    setStartBefore(node) {{ this.setStart(node.parentNode, selectionIndexInParent(node)); }}
    setStartAfter(node) {{ this.setStart(node.parentNode, selectionIndexInParent(node) + 1); }}
    setEndBefore(node) {{ this.setEnd(node.parentNode, selectionIndexInParent(node)); }}
    setEndAfter(node) {{ this.setEnd(node.parentNode, selectionIndexInParent(node) + 1); }}
    collapse(toStart = false) {{
      if (toStart) {{
        this.endContainer = this.startContainer;
        this.endOffset = this.startOffset;
      }} else {{
        this.startContainer = this.endContainer;
        this.startOffset = this.endOffset;
      }}
    }}
    selectNode(node) {{
      const parent = node && node.parentNode;
      if (!parent) throw selectionDomError("Range node has no parent", "InvalidNodeTypeError");
      const index = selectionIndexInParent(node);
      this.startContainer = parent;
      this.startOffset = index;
      this.endContainer = parent;
      this.endOffset = index + 1;
    }}
    selectNodeContents(node) {{
      selectionCheckBoundary(node, 0);
      this.startContainer = node;
      this.startOffset = 0;
      this.endContainer = node;
      this.endOffset = selectionNodeLength(node);
    }}
    compareBoundaryPoints(how, sourceRange) {{
      const mode = Number(how);
      const thisPoint = mode === 0 || mode === 3 ? [this.startContainer, this.startOffset] : [this.endContainer, this.endOffset];
      const otherPoint = mode === 0 || mode === 1 ? [sourceRange.startContainer, sourceRange.startOffset] : [sourceRange.endContainer, sourceRange.endOffset];
      return selectionComparePoints(thisPoint[0], thisPoint[1], otherPoint[0], otherPoint[1]);
    }}
    comparePoint(node, offset) {{
      const value = selectionCheckBoundary(node, offset);
      if (selectionComparePoints(node, value, this.startContainer, this.startOffset) < 0) return -1;
      if (selectionComparePoints(node, value, this.endContainer, this.endOffset) > 0) return 1;
      return 0;
    }}
    isPointInRange(node, offset) {{
      try {{ return this.comparePoint(node, offset) === 0; }} catch (_) {{ return false; }}
    }}
    intersectsNode(node) {{
      const parent = node && node.parentNode;
      if (!parent) return true;
      const index = selectionIndexInParent(node);
      return selectionComparePoints(parent, index, this.endContainer, this.endOffset) < 0
        && selectionComparePoints(parent, index + 1, this.startContainer, this.startOffset) > 0;
    }}
    cloneRange() {{
      const clone = new RangeNative();
      clone.startContainer = this.startContainer;
      clone.startOffset = this.startOffset;
      clone.endContainer = this.endContainer;
      clone.endOffset = this.endOffset;
      return clone;
    }}
    detach() {{}}
    getBoundingClientRect() {{ return selectionRect(this.startContainer); }}
    getClientRects() {{
      const rects = [selectionRect(this.startContainer)];
      if (typeof globalThis.DOMRectList === "function") return rects;
      rects.item = (index) => rects[index] ?? null;
      return rects;
    }}
    toString() {{
      if (this.startContainer === this.endContainer && Number(this.startContainer.nodeType) === 3)
        return String(this.startContainer.nodeValue ?? "").slice(this.startOffset, this.endOffset);
      const ancestor = this.commonAncestorContainer;
      return ancestor && typeof ancestor.textContent === "string" ? ancestor.textContent : "";
    }}
  }}
  Object.defineProperties(RangeNative, {{
    START_TO_START: {{ value: 0 }}, START_TO_END: {{ value: 1 }}, END_TO_END: {{ value: 2 }}, END_TO_START: {{ value: 3 }},
  }});
  Object.defineProperty(RangeNative, "name", {{ value: "Range" }});
  class SelectionNative {{
    constructor() {{
      this.__glassRange = null;
      this.__glassBackward = false;
    }}
    get rangeCount() {{ return this.__glassRange ? 1 : 0; }}
    get anchorNode() {{ const range = this.__glassRange; return range ? (this.__glassBackward ? range.endContainer : range.startContainer) : null; }}
    get anchorOffset() {{ const range = this.__glassRange; return range ? (this.__glassBackward ? range.endOffset : range.startOffset) : 0; }}
    get focusNode() {{ const range = this.__glassRange; return range ? (this.__glassBackward ? range.startContainer : range.endContainer) : null; }}
    get focusOffset() {{ const range = this.__glassRange; return range ? (this.__glassBackward ? range.startOffset : range.endOffset) : 0; }}
    get isCollapsed() {{ return !this.__glassRange || this.__glassRange.collapsed; }}
    get type() {{ return !this.__glassRange ? "None" : this.__glassRange.collapsed ? "Caret" : "Range"; }}
    get direction() {{ return !this.__glassRange || this.__glassRange.collapsed ? "none" : this.__glassBackward ? "backward" : "forward"; }}
    getRangeAt(index) {{
      if (Number(index) !== 0 || !this.__glassRange) throw selectionDomError("Selection range index is out of bounds", "IndexSizeError");
      return this.__glassRange;
    }}
    addRange(range) {{
      if (!(range instanceof RangeNative)) throw new TypeError("Selection.addRange requires a Range");
      if (this.__glassRange) return;
      this.__glassRange = range;
      this.__glassBackward = false;
    }}
    removeRange(range) {{
      if (range !== this.__glassRange) throw selectionDomError("Range is not in the selection", "NotFoundError");
      this.__glassRange = null;
    }}
    removeAllRanges() {{ this.__glassRange = null; this.__glassBackward = false; }}
    empty() {{ this.removeAllRanges(); }}
    collapse(node, offset = 0) {{
      if (node === null) {{ this.removeAllRanges(); return; }}
      const range = new RangeNative();
      range.setStart(node, offset);
      range.collapse(true);
      this.__glassRange = range;
      this.__glassBackward = false;
    }}
    setPosition(node, offset = 0) {{ this.collapse(node, offset); }}
    collapseToStart() {{
      const range = this.getRangeAt(0);
      this.collapse(range.startContainer, range.startOffset);
    }}
    collapseToEnd() {{
      const range = this.getRangeAt(0);
      this.collapse(range.endContainer, range.endOffset);
    }}
    extend(node, offset = 0) {{
      if (!this.__glassRange) throw selectionDomError("Selection has no range to extend", "InvalidStateError");
      const anchorNode = this.anchorNode;
      const anchorOffset = this.anchorOffset;
      this.setBaseAndExtent(anchorNode, anchorOffset, node, offset);
    }}
    setBaseAndExtent(anchorNode, anchorOffset, focusNode, focusOffset) {{
      const anchor = selectionCheckBoundary(anchorNode, anchorOffset);
      const focus = selectionCheckBoundary(focusNode, focusOffset);
      const backward = selectionComparePoints(anchorNode, anchor, focusNode, focus) > 0;
      const range = new RangeNative();
      if (backward) {{
        range.startContainer = focusNode; range.startOffset = focus;
        range.endContainer = anchorNode; range.endOffset = anchor;
      }} else {{
        range.startContainer = anchorNode; range.startOffset = anchor;
        range.endContainer = focusNode; range.endOffset = focus;
      }}
      this.__glassRange = range;
      this.__glassBackward = backward;
    }}
    selectAllChildren(node) {{
      const range = new RangeNative();
      range.selectNodeContents(node);
      this.__glassRange = range;
      this.__glassBackward = false;
    }}
    containsNode(node, allowPartialContainment = false) {{
      const range = this.__glassRange;
      if (!range || !node || !node.parentNode) return false;
      if (allowPartialContainment) return range.intersectsNode(node);
      const index = selectionIndexInParent(node);
      return selectionComparePoints(range.startContainer, range.startOffset, node.parentNode, index) <= 0
        && selectionComparePoints(node.parentNode, index + 1, range.endContainer, range.endOffset) <= 0;
    }}
    deleteFromDocument() {{}}
    modify() {{}}
    getComposedRanges() {{ return this.__glassRange ? [this.__glassRange.cloneRange()] : []; }}
    toString() {{ return this.__glassRange ? this.__glassRange.toString() : ""; }}
  }}
  Object.defineProperty(SelectionNative, "name", {{ value: "Selection" }});
  const documentSelection = new SelectionNative();
  globalThis.Range = RangeNative;
  globalThis.Selection = SelectionNative;
  globalThis.getSelection = () => documentSelection;
  Object.defineProperties(document, {{
    getSelection: {{ configurable: true, writable: true, value() {{ return documentSelection; }} }},
    createRange: {{ configurable: true, writable: true, value() {{ return new RangeNative(); }} }},
    hasFocus: {{ configurable: true, writable: true, value() {{ return true; }} }},
  }});
