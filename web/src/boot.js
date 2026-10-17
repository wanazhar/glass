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
retry.addEventListener("click", () => location.reload());
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
if (!stylesheet?.sheet)
  failure("The interface styles could not load. Try again.");
else
  import(appUrl)
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
