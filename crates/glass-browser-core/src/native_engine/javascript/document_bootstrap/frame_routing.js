  const frameBindingForNode = (nodeIndex) => {{
    const bindings = Array.isArray(globalThis.__glassFrameBindings)
      ? globalThis.__glassFrameBindings
      : [];
    return bindings.find((binding) => binding && binding.nodeIndex === nodeIndex) || null;
  }};
  const frameChildBindingForNode = (binding, nodeIndex) => {{
    const children = binding && Array.isArray(binding.children) ? binding.children : [];
    return children.find((child) => child && child.nodeIndex === nodeIndex) || null;
  }};
  const frameIdentifier = (binding) => String(
    binding && (binding.frameId || binding.contextId) || ""
  );
  const frameMutationNodes = globalThis.__glassFrameMutationNodes instanceof Map
    ? globalThis.__glassFrameMutationNodes
    : new Map();
  const frameMutationAttributes = globalThis.__glassFrameMutationAttributes instanceof Map
    ? globalThis.__glassFrameMutationAttributes
    : new Map();
  const frameMutationText = globalThis.__glassFrameMutationText instanceof Map
    ? globalThis.__glassFrameMutationText
    : new Map();
  const frameMutationParents = globalThis.__glassFrameMutationParents instanceof Map
    ? globalThis.__glassFrameMutationParents
    : new Map();
  const frameMutationChildren = globalThis.__glassFrameMutationChildren instanceof Map
    ? globalThis.__glassFrameMutationChildren
    : new Map();
  globalThis.__glassFrameMutationNodes = frameMutationNodes;
  globalThis.__glassFrameMutationAttributes = frameMutationAttributes;
  globalThis.__glassFrameMutationText = frameMutationText;
  globalThis.__glassFrameMutationParents = frameMutationParents;
  globalThis.__glassFrameMutationChildren = frameMutationChildren;
  const frameMutationKey = (binding, nodeIndex) =>
    frameIdentifier(binding) + "\\u0000" + String(nodeIndex);
  const frameMutationNode = (binding, nodeIndex) =>
    frameMutationNodes.get(frameMutationKey(binding, nodeIndex)) || null;
  const recordFrameMutation = (binding, command) => {{
    if (!binding || !command || typeof command !== "object") return;
    const kind = String(command.kind || "");
    const nodeIndex = Number(command.node_index);
    const key = frameMutationKey(binding, nodeIndex);
    const target = frameMutationNode(binding, nodeIndex);
    if (kind === "setAttribute") {{
      if (!target) return;
      const attributes = frameMutationAttributes.get(key) || {{}};
      const name = String(command.name).toLowerCase();
      const oldValue = Object.prototype.hasOwnProperty.call(attributes, name) ? attributes[name] : null;
      const value = String(command.value);
      attributes[name] = value;
      frameMutationAttributes.set(key, attributes);
      if (oldValue !== value) queueMutation({{ type: "attributes", target, attributeName: name, oldValue }});
      return;
    }}
    if (kind === "removeAttribute") {{
      if (!target) return;
      const attributes = frameMutationAttributes.get(key) || {{}};
      const name = String(command.name).toLowerCase();
      const oldValue = Object.prototype.hasOwnProperty.call(attributes, name) ? attributes[name] : null;
      delete attributes[name];
      frameMutationAttributes.set(key, attributes);
      if (oldValue !== null) queueMutation({{ type: "attributes", target, attributeName: name, oldValue }});
      return;
    }}
    if (kind === "setTextContent") {{
      if (!target) return;
      const value = String(command.value);
      if ([3, 8].includes(Number(target.nodeType))) {{
        const oldValue = frameMutationText.has(key)
          ? frameMutationText.get(key)
          : String(target.nodeValue || "");
        frameMutationText.set(key, value);
        if (oldValue !== value) queueMutation({{ type: "characterData", target, oldValue }});
        return;
      }}
      const oldChildren = frameMutationChildren.get(key) || [];
      const addedNodes = Array.isArray(target.__glassChildren) ? target.__glassChildren.slice() : [];
      const addedIndexes = addedNodes
        .map((node) => Number(node && node.nodeIndex))
        .filter((index) => Number.isFinite(index));
      const removedNodes = oldChildren.map((index) => frameMutationNode(binding, index)).filter(Boolean);
      frameMutationChildren.set(key, addedIndexes);
      for (const childIndex of oldChildren) frameMutationParents.delete(frameMutationKey(binding, childIndex));
      for (const childIndex of addedIndexes) frameMutationParents.set(frameMutationKey(binding, childIndex), nodeIndex);
      if (oldChildren.length > 0 || addedNodes.length > 0 || value.length > 0) {{
        queueMutation({{ type: "childList", target, addedNodes, removedNodes }});
      }}
      return;
    }}
    if (kind === "setInnerHtml") {{
      if (!target) return;
      const oldChildren = frameMutationChildren.get(key) || [];
      const addedNodes = Array.isArray(target.__glassChildren) ? target.__glassChildren.slice() : [];
      const addedIndexes = addedNodes
        .map((node) => Number(node && node.nodeIndex))
        .filter((index) => Number.isFinite(index));
      const removedNodes = oldChildren.map((index) => frameMutationNode(binding, index)).filter(Boolean);
      for (const childIndex of oldChildren) frameMutationParents.delete(frameMutationKey(binding, childIndex));
      frameMutationChildren.set(key, addedIndexes);
      for (const childIndex of addedIndexes) frameMutationParents.set(frameMutationKey(binding, childIndex), nodeIndex);
      if (oldChildren.length > 0 || addedNodes.length > 0 || String(command.value).length > 0) {{
        queueMutation({{ type: "childList", target, addedNodes, removedNodes }});
      }}
      return;
    }}
    if (kind === "removeNode") {{
      if (!target) return;
      const parentKey = frameMutationParents.get(key);
      const parent = frameMutationNode(binding, parentKey);
      if (parent) {{
        const childrenKey = frameMutationKey(binding, parentKey);
        const children = frameMutationChildren.get(childrenKey) || [];
        const position = children.indexOf(nodeIndex);
        queueMutation({{
          type: "childList",
          target: parent,
          removedNodes: [target],
          previousSibling: position > 0 ? frameMutationNode(binding, children[position - 1]) : null,
          nextSibling: position >= 0 ? frameMutationNode(binding, children[position + 1]) : null,
        }});
        frameMutationChildren.set(childrenKey, children.filter((index) => index !== nodeIndex));
      }}
      frameMutationParents.delete(key);
      return;
    }}
    if (kind === "appendChild" || kind === "insertBefore") {{
      const parentIndex = Number(command.parent_index);
      const parent = frameMutationNode(binding, parentIndex);
      const childIndex = Number(command.child_index);
      const child = frameMutationNode(binding, childIndex);
      if (!parent || !child) return;
      const parentKey = frameMutationKey(binding, parentIndex);
      const childKey = frameMutationKey(binding, childIndex);
      const oldParentIndex = frameMutationParents.get(childKey);
      if (oldParentIndex !== undefined) {{
        const oldParentKey = frameMutationKey(binding, oldParentIndex);
        const oldChildren = frameMutationChildren.get(oldParentKey) || [];
        const oldPosition = oldChildren.indexOf(childIndex);
        const oldParent = frameMutationNode(binding, oldParentIndex);
        if (oldParent) queueMutation({{
          type: "childList",
          target: oldParent,
          removedNodes: [child],
          previousSibling: oldPosition > 0 ? frameMutationNode(binding, oldChildren[oldPosition - 1]) : null,
          nextSibling: oldPosition >= 0 ? frameMutationNode(binding, oldChildren[oldPosition + 1]) : null,
        }});
        frameMutationChildren.set(oldParentKey, oldChildren.filter((index) => index !== childIndex));
      }}
      const children = (frameMutationChildren.get(parentKey) || []).filter((index) => index !== childIndex);
      const beforeIndex = kind === "insertBefore" && command.before_index != null
        ? Number(command.before_index)
        : null;
      const insertion = beforeIndex === null ? children.length : Math.max(0, children.indexOf(beforeIndex));
      const previousSibling = insertion > 0 ? frameMutationNode(binding, children[insertion - 1]) : null;
      const nextSibling = frameMutationNode(binding, children[insertion]);
      children.splice(insertion, 0, childIndex);
      frameMutationChildren.set(parentKey, children);
      frameMutationParents.set(childKey, parentIndex);
      queueMutation({{ type: "childList", target: parent, addedNodes: [child], previousSibling, nextSibling }});
    }}
  }};
  globalThis.__glassRecordFrameMutation = recordFrameMutation;
  const transferredNodeOwners = globalThis.__glassTransferredNodeOwners instanceof Map
    ? globalThis.__glassTransferredNodeOwners
    : new Map();
  const pendingTransferredNodeOwners = new Map();
  const currentOwnerFrameId = String(host.frame_id || host.context_id || "");
  globalThis.__glassTransferredNodeOwners = transferredNodeOwners;
  globalThis.__glassPendingTransferredNodeOwners = pendingTransferredNodeOwners;
  const commandNodeIndexes = (command) => command && typeof command === "object"
    ? [command.node_index, command.parent_index, command.child_index, command.before_index]
      .map(Number)
      .filter((index) => Number.isSafeInteger(index) && index >= 0)
    : [];
  const transferredOwnerForCommand = (command) => {{
    for (const index of commandNodeIndexes(command)) {{
      const owner = transferredNodeOwners.get(index);
      if (owner) return {{ index, owner }};
    }}
    return null;
  }};
  const queueFrameScriptForOwner = (owner, command) => {{
    if (suppressHostCommands > 0) return;
    const ownerFrameId = String(owner && owner.frameId || "");
    const frameId = ownerFrameId === currentOwnerFrameId
      ? String(owner && owner.relayFrameId || "")
      : ownerFrameId;
    if (!ownerFrameId || !frameId || !currentOwnerFrameId || frameId === currentOwnerFrameId) {{
      throw new TypeError("native node transfer relay is invalid");
    }}
    const binding = frameBindingForId(frameId);
    if (!binding || binding.sameOrigin !== true || frameIdentifier(binding) !== frameId) {{
      throw new DOMExceptionNative("The native node transfer relay is no longer accessible", "InvalidStateError");
    }}
    const routedCommand = ownerFrameId === currentOwnerFrameId
      ? {{ kind: "adoptedNodeCommand", destination_frame_id: ownerFrameId, command }}
      : command;
    pushCommand({{
      kind: "frameScript",
      frame_id: frameId,
      source_frame_id: currentOwnerFrameId,
      command: routedCommand,
    }});
  }};
  const routeTransferredNativeCommand = (command) => {{
    if (!command || typeof command !== "object" || command.kind === "frameScript") return false;
    const routed = transferredOwnerForCommand(command);
    if (!routed) return false;
    const {{ index, owner }} = routed;
    const frameId = String(owner.frameId || "");
    if (!frameId) throw new TypeError("adopted native node has no destination owner");
    if (frameId === currentOwnerFrameId) {{
      if (pendingTransferredNodeOwners.has(index)) queueFrameScriptForOwner(owner, command);
      else return false;
      return true;
    }}
    const binding = frameBindingForId(frameId);
    if (!binding || frameIdentifier(binding) !== frameId || binding.sameOrigin !== true) {{
      throw new DOMExceptionNative("The adopted node destination is no longer accessible", "InvalidStateError");
    }}
    queueFrameCommand(binding, command);
    return true;
  }};
  globalThis.__glassRouteTransferredNativeCommand = routeTransferredNativeCommand;
  const frameBatchableCommand = (command) => command && [
    "setValue", "setSelection", "setChecked", "setIndeterminate", "setSelected", "mediaLoad",
    "setAttribute", "removeAttribute", "setTextContent", "setInnerHtml",
    "removeNode", "createElement", "createTextNode", "createComment", "createDocumentType", "appendChild",
    "insertBefore", "setCustomValidity", "setFormAssociatedDisabledState",
  ].includes(String(command.kind));
  const queueFrameCommand = (binding, command) => {{
    if (!binding || binding.sameOrigin !== true) throw crossOriginSecurityError("document");
    if (suppressHostCommands > 0) return;
    const routed = transferredOwnerForCommand(command);
    if (routed) {{
      const {{ index, owner }} = routed;
      const ownerFrameId = String(owner.frameId || "");
      if (!ownerFrameId) throw new TypeError("adopted native node has no destination owner");
      if (ownerFrameId === currentOwnerFrameId) {{
        if (pendingTransferredNodeOwners.has(index)) queueFrameScriptForOwner(owner, command);
        else pushCommand(command);
        return;
      }}
      if (frameIdentifier(binding) !== ownerFrameId) {{
        const ownerBinding = frameBindingForId(ownerFrameId);
        if (!ownerBinding || ownerBinding.sameOrigin !== true
            || frameIdentifier(ownerBinding) !== ownerFrameId) {{
          throw new DOMExceptionNative("The adopted node destination is no longer accessible", "InvalidStateError");
        }}
        binding = ownerBinding;
      }}
    }}
    if (typeof globalThis.__glassRecordFrameMutation === "function") {{
      globalThis.__glassRecordFrameMutation(binding, command);
    }}
    const frameId = frameIdentifier(binding);
    const sourceFrameId = String(host.frame_id || host.context_id || "");
    if (!frameId || !sourceFrameId || frameId === sourceFrameId) {{
      throw new TypeError("native frame command target is invalid");
    }}
    const target = activeCommands();
    const previous = target[target.length - 1];
    if (frameBatchableCommand(command)
        && previous && previous.kind === "frameScript"
        && previous.frame_id === frameId
        && previous.source_frame_id === sourceFrameId
        && (frameBatchableCommand(previous.command)
          || previous.command && previous.command.kind === "frameScriptBatch")) {{
      if (previous.command.kind === "frameScriptBatch") previous.command.commands.push(command);
      else previous.command = {{ kind: "frameScriptBatch", commands: [previous.command, command] }};
      return;
    }}
    const dispatchedCommand = frameBatchableCommand(command)
      ? {{ kind: "frameScriptBatch", commands: [command] }}
      : command;
    pushCommand({{
      kind: "frameScript",
      frame_id: frameId,
      source_frame_id: sourceFrameId,
      command: dispatchedCommand,
    }});
  }};
  globalThis.__glassQueueNativeFrameCommand = (frameId, command) => {{
    const binding = frameBindingForId(String(frameId || ""));
    if (!binding || binding.sameOrigin !== true || frameIdentifier(binding) !== String(frameId || "")) {{
      throw new DOMExceptionNative("The native node transfer context is no longer accessible", "InvalidStateError");
    }}
    queueFrameCommand(binding, command);
  }};
  const frameBindingForId = (frameId) => {{
    const visit = (bindings) => {{
      for (const binding of bindings) {{
        if (binding && frameIdentifier(binding) === frameId) return binding;
        const nested = binding && Array.isArray(binding.children) ? visit(binding.children) : null;
        if (nested) return nested;
      }}
      return null;
    }};
    const bindings = Array.isArray(globalThis.__glassFrameBindings)
      ? globalThis.__glassFrameBindings
      : [];
    const embedded = visit(bindings);
    if (embedded) return embedded;
    const context = globalThis.__glassFrameContext;
    for (const related of [context && context.parent, context && context.top]) {{
      if (related && frameIdentifier(related) === frameId) return related;
    }}
    return null;
  }};
  const frameTopologyKey = (binding) => JSON.stringify([
    frameIdentifier(binding),
    String(binding && binding.url || ""),
    Number(binding && binding.revision || 0),
    Boolean(binding && binding.sameOrigin),
    Array.isArray(binding && binding.children)
      ? binding.children.map((child) => frameTopologyKey(child))
      : [],
  ]);
  const frameChildBindings = (binding) =>
    binding && Array.isArray(binding.children) ? binding.children : [];
  const currentFrameWindow = (binding, ownerFrameElement, parentWindow, topWindow) =>
    makeFrameWindow(binding, ownerFrameElement, parentWindow, topWindow);
  const frameDocumentCache = globalThis.__glassFrameDocumentCache instanceof Map
    ? globalThis.__glassFrameDocumentCache
    : new Map();
  const nativeFrameFocusedNodeIndices = globalThis.__glassNativeFrameFocusedNodeIndices instanceof Map
    ? globalThis.__glassNativeFrameFocusedNodeIndices
    : new Map();
  const frameTemplateContentsOwnerDocuments = globalThis.__glassFrameTemplateContentsOwnerDocuments instanceof Map
    ? globalThis.__glassFrameTemplateContentsOwnerDocuments
    : new Map();
  const frameWindowCache = globalThis.__glassFrameWindowCache instanceof Map
    ? globalThis.__glassFrameWindowCache
    : new Map();
  globalThis.__glassFrameDocumentCache = frameDocumentCache;
  globalThis.__glassNativeFrameFocusedNodeIndices = nativeFrameFocusedNodeIndices;
  globalThis.__glassFrameTemplateContentsOwnerDocuments = frameTemplateContentsOwnerDocuments;
  globalThis.__glassFrameWindowCache = frameWindowCache;
  const projectedFrameMatches = (element, selector, scope = element) => {{
    return matchesSelector(element, selector, scope);
  }};
  const nativeFrameFocusKey = (binding) =>
    frameIdentifier(binding) + "\\u0000" + String(binding.generation || "");
  const nativeFrameFocusedNodeIndex = (binding) => {{
    const key = nativeFrameFocusKey(binding);
    if (nativeFrameFocusedNodeIndices.has(key)) return nativeFrameFocusedNodeIndices.get(key);
    const elements = binding.document && Array.isArray(binding.document.elements)
      ? binding.document.elements
      : [];
    const focused = elements.find((entry) => entry && entry.focused === true);
    const candidate = focused
      ? Number(focused.focusAnchorNodeIndex == null
        ? focused.nodeIndex
        : focused.focusAnchorNodeIndex)
      : null;
    const nodeIndex = Number.isSafeInteger(candidate) && candidate >= 0 ? candidate : null;
    nativeFrameFocusedNodeIndices.set(key, nodeIndex);
    return nodeIndex;
  }};
