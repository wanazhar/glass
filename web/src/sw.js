const CACHE = "glass-shell-__VERSION__";
self.addEventListener("install", (event) =>
  event.waitUntil(
    caches
      .open(CACHE)
      .then((c) =>
        c.addAll([
          "/",
          "/app.js?version=__VERSION__",
          "/boot.js?version=__VERSION__",
          "/style.css?version=__VERSION__",
          "/fonts/inter-latin.woff2",
          "/fonts/jetbrains-mono-latin.woff2",
          "/icon.svg",
          "/icon-192.png",
          "/icon-512.png",
          "/manifest.webmanifest",
        ]),
      )
      .then(() => self.skipWaiting()),
  ),
);
self.addEventListener("activate", (event) =>
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys
            .filter(
              (k) =>
                k.startsWith("glass-shell-") &&
                k !== CACHE &&
                k !==
                  keys
                    .filter(
                      (key) => key.startsWith("glass-shell-") && key !== CACHE,
                    )
                    .at(-1),
            )
            .map((k) => caches.delete(k)),
        ),
      )
      .then(() => self.clients.claim()),
  ),
);
self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (
    event.request.method !== "GET" ||
    url.origin !== location.origin ||
    url.pathname.startsWith("/api/")
  )
    return;
  const core =
    event.request.mode === "navigate" ||
    ["/app.js", "/boot.js", "/style.css"].includes(url.pathname);
  if (core) {
    event.respondWith(
      caches.open(CACHE).then(async (cache) => {
        const cached = await cache.match(
          event.request.mode === "navigate" ? "/" : event.request,
        );
        return (
          cached || (await caches.match(event.request)) || fetch(event.request)
        );
      }),
    );
    return;
  }
  event.respondWith(
    fetch(event.request)
      .then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(CACHE).then((c) => c.put(event.request, copy));
        }
        return response;
      })
      .catch(() =>
        caches
          .match(event.request)
          .then(
            (r) =>
              r || new Response("Reconnect to your GC host", { status: 503 }),
          ),
      ),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const id = event.notification.data?.thread;
  if (typeof id !== "string" || !/^[a-zA-Z0-9_-]{1,128}$/.test(id)) return;
  event.waitUntil(
    self.clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then(async (clients) => {
        const client = clients.find(
          (client) => new URL(client.url).origin === self.location.origin,
        );
        if (client) {
          await client.focus();
          client.postMessage({ t: "open_thread", id });
        } else
          await self.clients.openWindow("/?thread=" + encodeURIComponent(id));
      }),
  );
});
