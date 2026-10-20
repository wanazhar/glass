pub(in crate::native_engine::javascript) const NATIVE_DYNAMIC_IMPORT_OPTIONS_BOOTSTRAP: &str = r###"
if (globalThis.__glassDynamicImportIntrinsics === undefined) {
  const intrinsics = ((PromiseConstructor, ObjectKeys, StringConstructor, TypeErrorConstructor) => {
    const isObject = (value) => value !== null
      && (typeof value === "object" || typeof value === "function");
    const normalize = (specifier, options) => {
      const specifierString = `${specifier}`;
      let moduleType = null;
      if (options !== undefined) {
        if (!isObject(options))
          throw new TypeErrorConstructor("dynamic import options must be an object");
        const attributes = options.with;
        if (attributes !== undefined) {
          if (!isObject(attributes))
            throw new TypeErrorConstructor("dynamic import attributes must be an object");
          const keys = ObjectKeys(attributes);
          const entries = [];
          for (let index = 0; index < keys.length; index += 1) {
            entries[index] = [keys[index], attributes[keys[index]]];
          }
          for (let index = 0; index < entries.length; index += 1) {
            if (typeof entries[index][1] !== "string")
              throw new TypeErrorConstructor("dynamic import attribute values must be strings");
          }
          for (let index = 0; index < entries.length; index += 1) {
            if (entries[index][0] !== "type")
              throw new TypeErrorConstructor("unsupported dynamic import attribute");
          }
          for (let index = 0; index < entries.length; index += 1) {
            if (entries[index][0] === "type") {
              if (entries[index][1] !== "json")
                throw new TypeErrorConstructor("unsupported dynamic import module type");
              moduleType = "json";
            }
          }
        }
      }
      return { specifier: specifierString, moduleType };
    };
    return Object.freeze({ PromiseConstructor, StringConstructor, TypeErrorConstructor, normalize });
  })(Promise, Object.keys, String, TypeError);
  Object.defineProperty(globalThis, "__glassDynamicImportIntrinsics", {
    value: intrinsics, enumerable: false, configurable: false, writable: false,
  });
}
"###;

pub(in crate::native_engine::javascript) const NATIVE_SHARED_WORKER_BOOTSTRAP: &str = r###"
  globalThis.__glassDispatchSharedWorkerConnect = (payload) => {
    if (closed) return null;
    const envelope = glassMessageDecodeEnvelope(payload || {});
    const ports = Object.freeze(Array.isArray(envelope.ports) ? envelope.ports.slice() : []);
    const source = ports[0] || null;
    const MessageEventConstructor = globalThis.__glassWorkerMessageEventConstructor || globalThis.MessageEvent;
    const event = new MessageEventConstructor("connect", {
      data: "",
      ports,
      source,
      origin: "",
      bubbles: false,
      cancelable: false,
    });
    Object.defineProperty(event, "ports", {
      configurable: false,
      enumerable: true,
      value: ports,
    });
    Object.defineProperty(event, "source", {
      configurable: false,
      enumerable: true,
      value: source,
    });
    event.target = globalThis;
    event.currentTarget = globalThis;
    event.eventPhase = 2;
    dispatch("connect", event);
    event.currentTarget = null;
    event.eventPhase = 0;
    return null;
  };
"###;
