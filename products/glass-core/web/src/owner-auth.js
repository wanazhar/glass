// Remote owner capabilities are origin-bound and never travel in URLs/cookies.
const storageKey = "gc-owner-capability-v1";
let ownerCapability = null;
try {
  const saved = localStorage.getItem(storageKey);
  if (/^[a-f0-9]{64}$/.test(saved || "")) ownerCapability = saved;
} catch { /* Storage may be disabled; the current tab can still authenticate. */ }

export function setOwnerCapability(value) {
  ownerCapability = /^[a-f0-9]{64}$/.test(value || "") ? value : null;
  try {
    if (ownerCapability) localStorage.setItem(storageKey, ownerCapability);
    else localStorage.removeItem(storageKey);
  } catch { /* Retain only the in-memory capability when storage is unavailable. */ }
}

export async function ownerFetch(input, options = {}) {
  const target = new URL(input, location.href);
  const headers = new Headers(options.headers);
  const authenticated = ownerCapability && target.origin === location.origin && target.pathname.startsWith("/api/");
  if (authenticated) headers.set("X-Glass-Owner-Capability", ownerCapability);
  const response = await fetch(input, { ...options, headers, ...(authenticated ? { redirect: "error" } : {}) });
  if (response.status === 403) {
    try {
      if ((await response.clone().json()).ownerAuthRequired === true) setOwnerCapability(null);
    } catch { /* Non-JSON authorization failures never expose response bodies. */ }
  }
  return response;
}

// Native EventSource cannot send authorization headers. Remote connections use
// an equivalent bounded fetch stream; local cookie-authenticated connections
// retain native EventSource behavior.
export function ownerEventSource(url) {
  if (!ownerCapability) return new EventSource(url);
  let closed = false, controller = null, retry = null, lastId = "";
  const source = {
    readyState: EventSource.CONNECTING,
    onopen: null, onmessage: null, onerror: null,
    close() {
      closed = true;
      source.readyState = EventSource.CLOSED;
      clearTimeout(retry);
      controller?.abort();
    },
  };
  const connect = async () => {
    if (closed) return;
    controller = new AbortController();
    source.readyState = EventSource.CONNECTING;
    try {
      const response = await ownerFetch(url, {
        signal: controller.signal, cache: "no-store", credentials: "same-origin",
        headers: lastId ? { "Last-Event-ID": lastId } : {},
      });
      if (!response.ok || !response.headers.get("content-type")?.startsWith("text/event-stream")) {
        if ([401, 403].includes(response.status)) {
          source.close();
          source.onerror?.();
          return;
        }
        throw Error("Workspace event connection unavailable");
      }
      source.readyState = EventSource.OPEN;
      source.onopen?.();
      const reader = response.body.getReader(), decoder = new TextDecoder();
      let buffered = "";
      while (!closed) {
        const { value, done } = await reader.read();
        if (done) break;
        buffered = (buffered + decoder.decode(value, { stream: true })).replaceAll("\r\n", "\n");
        let end;
        while ((end = buffered.indexOf("\n\n")) >= 0) {
          if (end > 4 * 1024 * 1024) throw Error("Workspace event exceeds bounds");
          const frame = buffered.slice(0, end);
          buffered = buffered.slice(end + 2);
          const data = [];
          for (const line of frame.split("\n")) {
            if (line.startsWith("data:")) data.push(line.slice(5).replace(/^ /, ""));
            if (line.startsWith("id:")) {
              const id = line.slice(3).trim();
              if (id.length <= 128 && !/[\x00-\x1f\x7f]/.test(id)) lastId = id;
            }
          }
          if (data.length && !closed) source.onmessage?.({ data: data.join("\n"), lastEventId: lastId });
        }
        if (buffered.length > 4 * 1024 * 1024) throw Error("Workspace event exceeds bounds");
      }
      if (!closed) throw Error("Workspace event connection ended");
    } catch {
      controller?.abort();
      if (!closed) {
        source.readyState = EventSource.CONNECTING;
        source.onerror?.();
        retry = setTimeout(connect, 1000);
      }
    }
  };
  queueMicrotask(connect);
  return source;
}

// Only the owned upload route accepts this progress-capable transport. The
// server returns JSON directly; arbitrary URLs and redirects are not supported.
export function ownerUpload(input, file, { signal, onProgress } = {}) {
  const target = new URL(input, location.href);
  if (target.origin !== location.origin || target.pathname !== "/api/upload")
    return Promise.reject(Error("Uploads must use the workspace upload endpoint"));
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new DOMException("Upload cancelled", "AbortError"));
    const xhr = new XMLHttpRequest();
    const abort = () => xhr.abort();
    const cleanup = () => signal?.removeEventListener("abort", abort);
    xhr.upload.addEventListener("progress", event => onProgress?.(event.loaded, event.lengthComputable ? event.total : 0));
    xhr.open("POST", target.href);
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    if (ownerCapability) xhr.setRequestHeader("X-Glass-Owner-Capability", ownerCapability);
    xhr.timeout = 20000;
    xhr.onload = () => {
      cleanup();
      if (xhr.responseURL && new URL(xhr.responseURL).origin !== location.origin) return reject(Error("Upload redirected outside this workspace"));
      let data;
      try { data = JSON.parse(xhr.responseText); } catch { return reject(Error("Invalid upload response")); }
      if (xhr.status === 403 && data.ownerAuthRequired) setOwnerCapability(null);
      if (xhr.status < 200 || xhr.status >= 300) return reject(Error(data.error || "Upload rejected"));
      resolve(data);
    };
    xhr.onerror = () => { cleanup(); reject(Error("Upload connection failed")); };
    xhr.ontimeout = () => { cleanup(); reject(Error("Upload timed out. Retry when connected.")); };
    xhr.onabort = () => { cleanup(); reject(new DOMException("Upload cancelled", "AbortError")); };
    signal?.addEventListener("abort", abort, { once: true });
    xhr.send(file);
  });
}
