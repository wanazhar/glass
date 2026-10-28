  const mutationObservers = globalThis.__glassMutationObservers instanceof Set
    ? globalThis.__glassMutationObservers
    : new Set();
  globalThis.__glassMutationObservers = mutationObservers;
  const mutationCreatedNodes = globalThis.__glassCreatedNodes instanceof Map
    ? globalThis.__glassCreatedNodes
    : new Map();
  mutationCreatedNodes.clear();
  globalThis.__glassCreatedNodes = mutationCreatedNodes;
  const scriptNodeObjects = globalThis.__glassScriptNodeObjects instanceof Map
    ? globalThis.__glassScriptNodeObjects
    : new Map();
  globalThis.__glassScriptNodeObjects = scriptNodeObjects;
  const transferredNodeOwnerRegistry = globalThis.__glassTransferredNodeOwners instanceof Map
    ? globalThis.__glassTransferredNodeOwners
    : new Map();
  globalThis.__glassTransferredNodeOwners = transferredNodeOwnerRegistry;
  const pendingTransferredNodeOwnerRegistry = globalThis.__glassPendingTransferredNodeOwners instanceof Map
    ? globalThis.__glassPendingTransferredNodeOwners
    : new Map();
  globalThis.__glassPendingTransferredNodeOwners = pendingTransferredNodeOwnerRegistry;
  const currentNativeOwnerId = String(host.frame_id || host.context_id || "");
  globalThis.__glassRebindTransferredNodeIndex = (temporaryIndex, nodeIndex, eventOwnerPrefix = "node:") => {{
    const temporary = Number(temporaryIndex);
    const next = Number(nodeIndex);
    if (!Number.isSafeInteger(temporary) || !Number.isSafeInteger(next)) return;
    if (temporary !== next) {{
      const previousPrefix = String(eventOwnerPrefix) + String(temporary) + ":";
      const nextPrefix = String(eventOwnerPrefix) + String(next) + ":";
      const rebindEventOwnerKeys = registry => {{
        for (const [key, value] of Array.from(registry.entries())) {{
          if (!key.startsWith(previousPrefix)) continue;
          const reboundKey = nextPrefix + key.slice(previousPrefix.length);
          if (registry.has(reboundKey)) {{
            throw new Error("native event owner identity collision");
          }}
          registry.set(reboundKey, value);
          registry.delete(key);
        }}
      }};
      rebindEventOwnerKeys(listeners);
      rebindEventOwnerKeys(eventHandlers);
    }}
    const owner = transferredNodeOwnerRegistry.get(temporary);
    if (!owner) return;
    const wasPending = pendingTransferredNodeOwnerRegistry.has(temporary);
    transferredNodeOwnerRegistry.delete(temporary);
    pendingTransferredNodeOwnerRegistry.delete(temporary);
    transferredNodeOwnerRegistry.set(next, owner);
    if (String(owner.frameId || "") === currentNativeOwnerId) {{
      pendingTransferredNodeOwnerRegistry.delete(next);
    }} else if (wasPending) {{
      pendingTransferredNodeOwnerRegistry.set(next, true);
    }}
  }};
  const scriptNodeAliasesByIndex = new Map();
  const scriptNodeIndexesByTemporary = new Map();
  for (const identity of Array.isArray(state.scriptNodes) ? state.scriptNodes : []) {{
    const temporaryIndex = Number(identity && identity.temporaryIndex);
    const nodeIndex = Number(identity && identity.nodeIndex);
    if (Number.isSafeInteger(temporaryIndex) && temporaryIndex >= 0
        && Number.isSafeInteger(nodeIndex) && nodeIndex >= 0) {{
      scriptNodeIndexesByTemporary.set(temporaryIndex, nodeIndex);
    }}
    const object = scriptNodeObjects.get(temporaryIndex);
    if (!object || !Number.isSafeInteger(nodeIndex) || nodeIndex < 0) continue;
    globalThis.__glassRebindTransferredNodeIndex(temporaryIndex, nodeIndex);
    rebindNodeEventOwner(Number(object.nodeIndex), nodeIndex);
    object.nodeIndex = nodeIndex;
    if (Number(object.nodeType) === 1
        && object.namespaceURI === HTML_NAMESPACE
        && object.localName === "template"
        && temporaryIndex !== nodeIndex) {{
      const templateContentsCache = globalThis.__glassTemplateContents;
      if (templateContentsCache instanceof Map && templateContentsCache.has(temporaryIndex)) {{
        const templateContent = templateContentsCache.get(temporaryIndex);
        templateContentsCache.delete(temporaryIndex);
        templateContentsCache.set(nodeIndex, templateContent);
        if (templateContent && typeof templateContent === "object") {{
          templateContent.__glassTemplateHost = object;
        }}
      }}
    }}
    scriptNodeAliasesByIndex.set(nodeIndex, object);
  }}
  globalThis.__glassScriptNodeIndexesByTemporary = scriptNodeIndexesByTemporary;
  const reuseMutationShadow = Boolean(
    reuseCachedHostSnapshotState
      && cachedMutationShadow !== null
      && cachedMutationShadow.state === state,
  );
  const mutationShadowAttributes = reuseMutationShadow
    ? cachedMutationShadow.attributes
    : new Map();
  const mutationShadowText = reuseMutationShadow ? cachedMutationShadow.text : new Map();
  const mutationShadowParents = reuseMutationShadow ? cachedMutationShadow.parents : new Map();
  const mutationShadowChildren = reuseMutationShadow ? cachedMutationShadow.children : new Map();
  if (!reuseMutationShadow) {{
    for (const entry of Array.isArray(state.elements) ? state.elements : []) {{
      // Host snapshot attributes are immutable for this document generation.
      // Keep the snapshot object until a mutation command touches this element.
      mutationShadowAttributes.set(Number(entry.nodeIndex), entry.attributes || {{}});
    }}
    for (const entry of Array.isArray(state.nodes) ? state.nodes : []) {{
      const index = Number(entry.nodeIndex);
      if ([3, 8].includes(Number(entry.nodeType))) mutationShadowText.set(index, String(entry.nodeValue || ""));
      if (entry.parentIndex !== null && entry.parentIndex !== undefined) {{
        mutationShadowParents.set(index, Number(entry.parentIndex));
      }}
      // Child indexes are already normalized by the host serializer. Treat the
      // snapshot array as immutable and replace it only when a command changes
      // this parent's children.
      mutationShadowChildren.set(index, Array.isArray(entry.children) ? entry.children : []);
    }}
  }}
  cachedMutationShadow = {{
    state,
    attributes: mutationShadowAttributes,
    text: mutationShadowText,
    parents: mutationShadowParents,
    children: mutationShadowChildren,
  }};
  const mutationNode = (nodeIndex) => {{
    const index = Number(nodeIndex);
    if (index === 0 && globalThis.__glassHostDocument) return globalThis.__glassHostDocument;
    const nodes = globalThis.__glassHostNodes;
    if (nodes instanceof Map && nodes.has(index)) return nodes.get(index);
    return mutationCreatedNodes.get(index) || null;
  }};
  const mutationParent = (node) => {{
    if (!node) return null;
    if (node.__glassParent) return node.__glassParent;
    if (node.__glassMutationDocument) return node.__glassMutationDocument;
    if (node.parentIndex !== null && node.parentIndex !== undefined) {{
      return mutationNode(node.parentIndex);
    }}
    const owner = globalThis.__glassHostDocument;
    return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(node)
      ? owner
      : null;
  }};
  const mutationMatchesTarget = (target, observed, subtree) => {{
    if (target === observed) return true;
    if (!subtree) return false;
    let current = mutationParent(target);
    for (let depth = 0; current && depth <= {max_commands}; depth += 1) {{
      if (current === observed) return true;
      current = mutationParent(current);
    }}
    return false;
  }};
  const mutationTextNode = (value) => {{
    const text = String(value);
    return {{
      nodeType: 3,
      nodeName: "#text",
      nodeValue: text,
      textContent: text,
      data: text,
      parentNode: null,
      parentElement: null,
    }};
  }};
  const mutationRecord = (record, options) => {{
    const type = String(record.type);
    return {{
      type,
      target: record.target,
      addedNodes: type === "childList" ? [...(record.addedNodes || [])] : [],
      removedNodes: type === "childList" ? [...(record.removedNodes || [])] : [],
      previousSibling: record.previousSibling || null,
      nextSibling: record.nextSibling || null,
      attributeName: type === "attributes" ? String(record.attributeName || "") : null,
      attributeNamespace: null,
      oldValue: (type === "attributes" || type === "characterData") && options.oldValue
        ? (record.oldValue === undefined ? null : record.oldValue)
        : null,
    }};
  }};
  let mutationDeliveryQueued = false;
  const deliverMutationObservers = () => {{
    mutationDeliveryQueued = false;
    for (const observer of Array.from(mutationObservers)) {{
      if (!observer.__glassRecords || observer.__glassRecords.length === 0) continue;
      const records = observer.__glassRecords.splice(0, observer.__glassRecords.length);
      observer.__glassCallback(records, observer);
    }}
  }};
  const scheduleMutationDelivery = () => {{
    if (mutationDeliveryQueued) return;
    mutationDeliveryQueued = true;
    Promise.resolve().then(deliverMutationObservers);
  }};
  const queueMutation = (record) => {{
    if (!record || !record.target) return;
    for (const observer of Array.from(mutationObservers)) {{
      const registrations = observer.__glassRegistrations || [];
      for (const registration of registrations) {{
        const options = registration.options;
        if (!mutationMatchesTarget(record.target, registration.target, options.subtree)) continue;
        if (record.type === "attributes" && !options.attributes) continue;
        if (record.type === "characterData" && !options.characterData) continue;
        if (record.type === "childList" && !options.childList) continue;
        if (record.type === "attributes" && options.attributeFilter
            && !options.attributeFilter.includes(String(record.attributeName).toLowerCase())) continue;
        observer.__glassRecords.push(mutationRecord(record, {{
          oldValue: record.type === "attributes"
            ? options.attributeOldValue
            : options.characterDataOldValue,
        }}));
        if (observer.__glassRecords.length > {max_commands}) {{
          observer.__glassRecords.splice(0, observer.__glassRecords.length - {max_commands});
        }}
        break;
      }}
    }}
    scheduleMutationDelivery();
  }};
  Object.defineProperty(globalThis, "__glassQueueInnerHtmlMutation", {{
    enumerable: false,
    configurable: true,
    value(target, removedNodes, addedNodes) {{
      const removed = Array.from(removedNodes || []);
      const added = Array.from(addedNodes || []);
      if (removed.length === 0 && added.length === 0) return;
      queueMutation({{
        type: "childList",
        target,
        removedNodes: removed,
        addedNodes: added,
        previousSibling: null,
        nextSibling: null,
      }});
    }},
  }});
  const queueFragmentChildRemoval = (parent, child) => {{
    if (!parent || parent.__glassFragment !== true || !child) return;
    const children = Array.isArray(parent.__glassChildren) ? parent.__glassChildren : [];
    const position = children.indexOf(child);
    if (position < 0) return;
    queueMutation({{
      type: "childList",
      target: parent,
      removedNodes: [child],
      previousSibling: position > 0 ? children[position - 1] : null,
      nextSibling: position + 1 < children.length ? children[position + 1] : null,
    }});
  }};
  const normalizeMutationOptions = (rawOptions) => {{
    if (!rawOptions || typeof rawOptions !== "object") throw new TypeError("MutationObserver options must be an object");
    const attributes = rawOptions.attributes === undefined
      ? rawOptions.attributeOldValue !== undefined || rawOptions.attributeFilter !== undefined
      : Boolean(rawOptions.attributes);
    const characterData = rawOptions.characterData === undefined
      ? rawOptions.characterDataOldValue !== undefined
      : Boolean(rawOptions.characterData);
    const childList = Boolean(rawOptions.childList);
    if (!attributes && !characterData && !childList) throw new TypeError("MutationObserver requires an observation type");
    if (rawOptions.attributeOldValue && !attributes) throw new TypeError("attributeOldValue requires attributes");
    if (rawOptions.attributeFilter !== undefined && !attributes) throw new TypeError("attributeFilter requires attributes");
    if (rawOptions.characterDataOldValue && !characterData) throw new TypeError("characterDataOldValue requires characterData");
    let attributeFilter = null;
    if (rawOptions.attributeFilter !== undefined) {{
      if (!Array.isArray(rawOptions.attributeFilter)) throw new TypeError("attributeFilter must be an array");
      attributeFilter = rawOptions.attributeFilter.map((name) => String(name).toLowerCase());
    }}
    return {{
      attributes,
      attributeOldValue: Boolean(rawOptions.attributeOldValue),
      attributeFilter,
      characterData,
      characterDataOldValue: Boolean(rawOptions.characterDataOldValue),
      childList,
      subtree: Boolean(rawOptions.subtree),
    }};
  }};
  const MutationObserverNative = globalThis.__glassMutationObserverConstructor
    || function MutationObserver(callback) {{
      if (!(this instanceof MutationObserverNative)) throw new TypeError("MutationObserver requires new");
      if (typeof callback !== "function") throw new TypeError("MutationObserver callback must be callable");
      Object.defineProperties(this, {{
        __glassCallback: {{ value: callback, writable: true }},
        __glassRegistrations: {{ value: [], writable: true }},
        __glassRecords: {{ value: [], writable: true }},
      }});
    }};
  MutationObserverNative.prototype.observe = function(target, rawOptions) {{
    if (!target || ![1, 3, 8, 9, 11].includes(Number(target.nodeType))) throw new TypeError("MutationObserver target must be a node");
    const options = normalizeMutationOptions(rawOptions);
    const registrations = this.__glassRegistrations || [];
    const existing = registrations.find((registration) => registration.target === target);
    if (existing) existing.options = options;
    else registrations.push({{ target, options }});
    this.__glassRegistrations = registrations;
    mutationObservers.add(this);
  }};
  MutationObserverNative.prototype.disconnect = function() {{
    this.__glassRegistrations = [];
    this.__glassRecords = [];
    mutationObservers.delete(this);
  }};
  MutationObserverNative.prototype.takeRecords = function() {{
    const records = this.__glassRecords || [];
    this.__glassRecords = [];
    return records;
  }};
  globalThis.__glassMutationObserverConstructor = MutationObserverNative;
  globalThis.MutationObserver = MutationObserverNative;
  const mutationIndexList = (indexes) => (Array.isArray(indexes) ? indexes : [])
    .map((index) => mutationNode(index))
    .filter(Boolean);
  const recordCommandMutation = (command) => {{
    if (!command || typeof command !== "object") return;
    const kind = String(command.kind || "");
    const nodeIndex = Number(command.node_index);
    const target = mutationNode(nodeIndex);
    if (kind === "setAttribute") {{
      if (!target) return;
      const attributes = {{ ...(mutationShadowAttributes.get(nodeIndex) || {{}}) }};
      const name = String(command.name).toLowerCase();
      const oldValue = Object.prototype.hasOwnProperty.call(attributes, name) ? attributes[name] : null;
      const value = String(command.value);
      attributes[name] = value;
      mutationShadowAttributes.set(nodeIndex, attributes);
      if (oldValue !== value) queueMutation({{ type: "attributes", target, attributeName: name, oldValue }});
      return;
    }}
    if (kind === "removeAttribute") {{
      if (!target) return;
      const attributes = {{ ...(mutationShadowAttributes.get(nodeIndex) || {{}}) }};
      const name = String(command.name).toLowerCase();
      const oldValue = Object.prototype.hasOwnProperty.call(attributes, name) ? attributes[name] : null;
      delete attributes[name];
      mutationShadowAttributes.set(nodeIndex, attributes);
      if (oldValue !== null) queueMutation({{ type: "attributes", target, attributeName: name, oldValue }});
      return;
    }}
    if (kind === "setTextContent") {{
      if (!target) return;
      const value = String(command.value);
      if ([3, 8].includes(Number(target.nodeType))) {{
        const oldValue = mutationShadowText.has(nodeIndex)
          ? mutationShadowText.get(nodeIndex)
          : String(target.nodeValue || "");
        mutationShadowText.set(nodeIndex, value);
        if (oldValue !== value) queueMutation({{ type: "characterData", target, oldValue }});
        return;
      }}
      const oldChildren = mutationShadowChildren.get(nodeIndex) || [];
      const addedNodes = Array.isArray(target.__glassChildren) ? target.__glassChildren.slice() : [];
      const addedIndexes = addedNodes
        .map((node) => Number(node && node.nodeIndex))
        .filter((index) => Number.isFinite(index));
      const removedNodes = mutationIndexList(oldChildren);
      for (const childIndex of oldChildren) mutationShadowParents.delete(Number(childIndex));
      mutationShadowChildren.set(nodeIndex, addedIndexes);
      for (const childIndex of addedIndexes) mutationShadowParents.set(childIndex, nodeIndex);
      if (oldChildren.length > 0 || addedNodes.length > 0 || value.length > 0) {{
        queueMutation({{ type: "childList", target, addedNodes, removedNodes }});
      }}
      return;
    }}
    if (kind === "setInnerHtml") {{
      if (!target) return;
      const oldChildren = mutationShadowChildren.get(nodeIndex) || [];
      const addedNodes = Array.isArray(target.__glassChildren) ? target.__glassChildren.slice() : [];
      const addedIndexes = addedNodes
        .map((node) => Number(node && node.nodeIndex))
        .filter((index) => Number.isFinite(index));
      const removedNodes = mutationIndexList(oldChildren);
      for (const childIndex of oldChildren) mutationShadowParents.delete(Number(childIndex));
      mutationShadowChildren.set(nodeIndex, addedIndexes);
      for (const childIndex of addedIndexes) mutationShadowParents.set(childIndex, nodeIndex);
      if (oldChildren.length > 0 || addedNodes.length > 0 || String(command.value).length > 0) {{
        queueMutation({{ type: "childList", target, addedNodes, removedNodes }});
      }}
      return;
    }}
    if (kind === "removeNode") {{
      if (!target) return;
      const parentIndex = mutationShadowParents.get(nodeIndex);
      const parent = mutationNode(parentIndex);
      if (parent) {{
        const children = mutationShadowChildren.get(Number(parentIndex)) || [];
        const position = children.indexOf(nodeIndex);
        queueMutation({{
          type: "childList",
          target: parent,
          removedNodes: [target],
          previousSibling: position > 0 ? mutationNode(children[position - 1]) : null,
          nextSibling: position >= 0 ? mutationNode(children[position + 1]) : null,
        }});
        mutationShadowChildren.set(Number(parentIndex), children.filter((index) => index !== nodeIndex));
      }}
      mutationShadowParents.delete(nodeIndex);
      return;
    }}
    if (kind === "appendChild" || kind === "insertBefore") {{
      const parentIndex = Number(command.parent_index);
      const parent = mutationNode(parentIndex);
      const childIndex = Number(command.child_index);
      const child = mutationNode(childIndex);
      if (!parent || !child) return;
      const oldParentIndex = mutationShadowParents.get(childIndex);
      if (oldParentIndex !== undefined) {{
        const oldChildren = mutationShadowChildren.get(oldParentIndex) || [];
        const oldPosition = oldChildren.indexOf(childIndex);
        const oldParent = mutationNode(oldParentIndex);
        if (oldParent) queueMutation({{
          type: "childList",
          target: oldParent,
          removedNodes: [child],
          previousSibling: oldPosition > 0 ? mutationNode(oldChildren[oldPosition - 1]) : null,
          nextSibling: oldPosition >= 0 ? mutationNode(oldChildren[oldPosition + 1]) : null,
        }});
        mutationShadowChildren.set(oldParentIndex, oldChildren.filter((index) => index !== childIndex));
      }}
      const children = (mutationShadowChildren.get(parentIndex) || []).filter((index) => index !== childIndex);
      const beforeIndex = kind === "insertBefore" && command.before_index != null
        ? Number(command.before_index)
        : null;
      const insertion = beforeIndex === null ? children.length : Math.max(0, children.indexOf(beforeIndex));
      const previousSibling = insertion > 0 ? mutationNode(children[insertion - 1]) : null;
      const nextSibling = mutationNode(children[insertion]);
      children.splice(insertion, 0, childIndex);
      mutationShadowChildren.set(parentIndex, children);
      mutationShadowParents.set(childIndex, parentIndex);
      queueMutation({{ type: "childList", target: parent, addedNodes: [child], previousSibling, nextSibling }});
    }}
  }};
  globalThis.__glassRecordMutationCommand = recordCommandMutation;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("module_mutation_observers");
