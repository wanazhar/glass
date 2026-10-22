export function attention(button) {
  let enabled = false;
  try {
    enabled = localStorage.getItem("glass.notifications.v1") === "on";
  } catch {}
  if (!("Notification" in window) || Notification.permission !== "granted")
    enabled = false;
  const supported = "Notification" in window && isSecureContext;
  function paint() {
    button.textContent = enabled ? "Notifications on" : "Notifications";
    button.title = supported
      ? "Opt-in alerts while this page is connected. Background iPhone push is not provided."
      : "Notifications require a supported browser and HTTPS.";
    button.disabled = !supported;
    button.setAttribute("aria-pressed", String(enabled));
  }
  paint();
  button.onclick = async () => {
    if (!supported) return;
    if (!enabled && Notification.permission === "default")
      await Notification.requestPermission();
    enabled = !enabled && Notification.permission === "granted";
    try {
      localStorage.setItem("glass.notifications.v1", enabled ? "on" : "off");
    } catch {}
    paint();
  };
  return async (title, id) => {
    if (!enabled || !supported || Notification.permission !== "granted") return;
    if (document.hasFocus() && document.visibilityState === "visible") return;
    const options = {
      body: "Open Glass to review this thread.",
      tag: "glass-" + id,
      icon: "/icon-192.png",
      data: { thread: id },
    };
    try {
      const registration = await navigator.serviceWorker?.getRegistration();
      if (registration) await registration.showNotification(title, options);
      else {
        const notification = new Notification(title, options);
        notification.onclick = () => {
          window.focus();
          notification.close();
        };
      }
    } catch {
      /* Permission may be revoked outside the app. */
    }
  };
}
