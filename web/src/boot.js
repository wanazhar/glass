// Independent of the application bundle so import failures remain recoverable.
const launch = document.querySelector("#launch");
const status = document.querySelector("#launch-status");
const detail = document.querySelector("#launch-detail");
const retry = document.querySelector("#launch-retry");
const shell = document.querySelector(".app");
let complete = false;
launch.dataset.booted = "true";
// Discover repaired shells even when the cached application cannot initialize.
if ("serviceWorker" in navigator)
  navigator.serviceWorker
    .register("/sw.js", { updateViaCache: "none" })
    .catch(() => {});
retry.addEventListener("click", async () => {
  retry.disabled = true;
  try {
    // Refresh explicitly: navigation reload does not bypass cached subresources.
    // A failed/offline refresh keeps the existing shell available for recovery.
    const assets = [appUrl, stylesheet?.href].filter(Boolean);
    const responses = await Promise.all(assets.map(async url => {
      try { return await fetch(url, { cache: "reload" }); } catch { return null; }
    }));
    if ("caches" in window) {
      const names = (await caches.keys()).filter(name => name.startsWith("glass-shell-"));
      await Promise.all(names.map(async name => {
        const cache = await caches.open(name);
        await Promise.all(assets.map(async (url, index) => {
          if (responses[index]?.ok && await cache.match(url))
            await cache.put(url, responses[index].clone());
        }));
      }));
    }
  } catch { /* A restricted cache must not disable normal reload. */ }
  location.reload();
});
function failure(message) {
  if (complete) return;
  clearTimeout(watchdog);
  shell.hidden = true;
  shell.inert = true;
  launch.dataset.state = "error";
  status.textContent = "Connection unavailable";
  detail.textContent =
    navigator.onLine === false
      ? "Reconnect to your host, then try again."
      : String(
          message ||
            "Glass could not open. Check your connection and try again.",
        ).slice(0, 240);
  retry.hidden = false;
}
const watchdog = setTimeout(() => {
  if (complete) return;
  launch.dataset.state = "slow";
  status.textContent = "Opening is taking longer";
  detail.textContent = "You can try again.";
  retry.hidden = false;
}, 15000);
const appUrl = "/app.js?version=__SHELL_VERSION__";
const stylesheet = document.querySelector('link[rel="stylesheet"]');
// The applied readiness marker is authoritative. CSSStyleSheet availability
// alone is not a reliable signal across WebKit startup and cached restores.
const stylesReady = () => getComputedStyle(document.documentElement)
  .getPropertyValue("--glass-ui-ready").trim() === "1";
function waitForStyles(link, milliseconds) {
  if (stylesReady()) return Promise.resolve(true);
  return new Promise(resolve => {
    const deadline = Date.now() + milliseconds;
    let timer, settled = false;
    const done = value => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      link?.removeEventListener("load", check);
      link?.removeEventListener("error", failed);
      resolve(value);
    };
    const check = () => {
      if (settled) return;
      clearTimeout(timer);
      if (stylesReady()) return done(true);
      if (Date.now() >= deadline) return done(false);
      timer = setTimeout(check, 50);
    };
    const failed = () => done(stylesReady());
    link?.addEventListener("load", check, { once: true });
    link?.addEventListener("error", failed, { once: true });
    check();
  });
}
async function ensureStyles() {
  if (await waitForStyles(stylesheet, 2000)) return true;
  if (!stylesheet) return false;
  // A unique URL escapes both an HTTP cache entry and an older shell's SW
  // cache. Keep the existing link available for offline/late-load recovery.
  const repaired = document.createElement("link");
  repaired.rel = "stylesheet";
  const url = new URL(stylesheet.href);
  url.searchParams.set("recovery", Date.now().toString(36));
  repaired.href = url.href;
  const ready = waitForStyles(repaired, 8000);
  document.head.append(repaired);
  return ready;
}
ensureStyles().then(ready => {
  if (!ready) return failure("The interface styles could not load. Try again.");
  return import(appUrl)
    .then(async (app) => {
      const result = await app.initialReady;
      if (result?.ok !== true) {
        failure(result?.message);
        return;
      }
      clearTimeout(watchdog);
      shell.hidden = false;
      shell.inert = false;
      app.revealShell?.();
      launch.hidden = true;
      complete = true;
    })
    .catch((error) => failure(error?.message));
}).catch(error => failure(error?.message));
