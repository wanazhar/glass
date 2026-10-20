  globalThis.__glassResolveFetch = (requestId, payload) => {{
    const pending = fetchRequests.get(Number(requestId));
    if (!pending) return;
    fetchRequests.delete(Number(requestId));
    clearFetchAbortListener(pending);
    if (pending.uploadStreamId !== null) {{
      const uploadGroup = fetchUploadGroups.get(pending.uploadStreamId);
      cancelFetchUploadGroup(uploadGroup, "native fetch request completed");
      fetchUploadGroups.delete(pending.uploadStreamId);
    }}
    if (pending.dynamicModuleImport) {{
      const moduleResult = payload && payload.dynamicModuleImport;
      if (!moduleResult || moduleResult.error) {{
        pending.reject(new TypeError(String(moduleResult && moduleResult.error
          || "Failed to load dynamically imported module")));
      }} else if (typeof moduleResult.moduleKey !== "string") {{
        pending.reject(new TypeError("Invalid dynamic module response"));
      }} else {{
        try {{
          const modulePromise = pending.moduleType === "json"
            ? import(moduleResult.moduleKey, {{ with: {{ type: "json" }} }})
            : import(moduleResult.moduleKey);
          modulePromise.then(pending.resolve, pending.reject);
        }} catch (error) {{ pending.reject(error); }}
      }}
      return;
    }}
    if (payload && payload.error) {{
      pending.reject(payload.timeout === true
        ? nativeTimeoutError()
        : new TypeError(String(payload.error)));
    }}
    else pending.resolve(responseFromFetch(payload));
  }};
  globalThis.queueMicrotask = (callback) => {{
    if (typeof callback !== "function") throw new TypeError("microtask callback must be callable");
    Promise.resolve().then(callback);
  }};
  const reportScheduledCallbackError = (error) => {{
    try {{
      if (typeof globalThis.__glassDispatchScriptError !== "function") return;
      const message = error && typeof error.message === "string"
        ? error.message
        : String(error);
      globalThis.__glassDispatchScriptError({{
        node_index: null,
        message,
        error,
        filename: String(host.url || ""),
        lineno: 0,
        colno: 0,
      }});
    }} catch (_reportError) {{
      // Reporting must not turn an uncaught page callback into a host failure.
    }}
  }};
  globalThis.__glassRunTimers = (currentNow) => {{
    const now = Number.isFinite(Number(currentNow)) ? Number(currentNow) : host.now_ms;
    const pending = Array.from(timers.entries())
      .filter(([, timer]) => Number(timer.dueAt === undefined ? 0 : timer.dueAt) <= now)
      .sort((left, right) => {{
        const due = Number(left[1].dueAt === undefined ? 0 : left[1].dueAt)
          - Number(right[1].dueAt === undefined ? 0 : right[1].dueAt);
        return due || left[0] - right[0];
    }});
    for (const [id, timer] of pending) {{
      if (!timers.has(id)) continue;
      timers.delete(id);
      runningTimers.set(id, timer);
      try {{
        timer.callback(...timer.args);
      }} catch (error) {{
        reportScheduledCallbackError(error);
      }} finally {{
        runningTimers.delete(id);
        if (timer.intervalMs > 0 && !timer.cancelled) {{
          timer.dueAt = now + timer.intervalMs;
          timers.set(id, timer);
        }}
      }}
    }}
    const frames = Array.from(animationFrames.entries())
      .filter(([, frame]) => Number(frame.dueAt === undefined ? 0 : frame.dueAt) <= now)
      .sort((left, right) => {{
        const due = Number(left[1].dueAt === undefined ? 0 : left[1].dueAt)
          - Number(right[1].dueAt === undefined ? 0 : right[1].dueAt);
        return due || left[0] - right[0];
      }});
    for (const [id, frame] of frames) {{
      if (!animationFrames.has(id)) continue;
      animationFrames.delete(id);
      try {{
        frame.callback.call(globalThis, now);
      }} catch (error) {{
        reportScheduledCallbackError(error);
      }}
    }}
    const idles = Array.from(idleCallbacks.entries())
      .filter(([, idle]) => idle.timeoutAt === null || Number(idle.timeoutAt) <= now)
      .sort((left, right) => {{
        const leftAt = left[1].timeoutAt === null ? Number.MAX_SAFE_INTEGER : Number(left[1].timeoutAt);
        const rightAt = right[1].timeoutAt === null ? Number.MAX_SAFE_INTEGER : Number(right[1].timeoutAt);
        return leftAt - rightAt || left[0] - right[0];
      }});
    for (const [id, idle] of idles) {{
      if (!idleCallbacks.has(id)) continue;
      idleCallbacks.delete(id);
      const startedAt = performance.now();
      const didTimeout = idle.timeoutAt !== null && Number(idle.timeoutAt) <= now;
      const deadline = {{
        didTimeout,
        timeRemaining() {{
          return Math.max(0, 50 - (performance.now() - startedAt));
        }},
      }};
      try {{
        idle.callback.call(globalThis, deadline);
      }} catch (error) {{
        reportScheduledCallbackError(error);
      }}
    }}
  }};
