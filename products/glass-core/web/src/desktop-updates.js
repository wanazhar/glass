import { ownerFetch } from "./owner-auth.js";

// Single-flight region updates. Checkpoints advance only after every
// region has decoded; a reconnect starts with a full frame, never a partial one.
export async function consumeDesktopUpdates({ session, signal, current, emit, canvas = document.createElement("canvas") }) {
  const display = canvas.getContext("2d", { alpha: false, willReadFrequently: true });
  if (!display) throw Error("Desktop canvas unavailable");
  let hashes = [], sequence = 0, lastImage = null;
  const check = () => !signal.aborted && current();
  const decode = async (data, type, width, height) => {
    if (typeof data !== "string" || data.length > 6 * 1024 * 1024) throw Error("Invalid desktop region");
    const bytes = Uint8Array.from(atob(data), (character) => character.charCodeAt(0));
    const blob = new Blob([bytes], { type });
    const url = URL.createObjectURL(blob);
    const image = new Image();
    try {
      image.src = url;
      await image.decode();
      if (image.naturalWidth !== width || image.naturalHeight !== height) throw Error("Desktop region dimensions changed");
      return { image, blob };
    } finally { URL.revokeObjectURL(url); }
  };
  while (check()) {
    const controller = new AbortController();
    const abort = () => controller.abort();
    signal.addEventListener("abort", abort, { once: true });
    const timeout = setTimeout(abort, 12000);
    try {
      const response = await ownerFetch("/api/desktop/updates", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ sessionId: session, after: sequence }),
        cache: "no-store", signal: controller.signal,
      });
      if (!check()) return;
      if (response.status === 401 || response.status === 403) {
        const error = Error("Desktop owner authorization failed");
        error.streamFailureReason = "authorization";
        throw error;
      }
      if (response.status !== 204) {
        if (!response.ok) {
          const error = Error("Desktop region connection unavailable");
          error.httpStatus = response.status;
          throw error;
        }
        const text = await response.text();
        if (text.length > 6 * 1024 * 1024) throw Error("Desktop update exceeds bounds");
        const update = JSON.parse(text);
        if (!check()) return;
        const { width, height, tileSize, columns, frameSeq, frameTimeMs } = update;
        if (update.version !== 1 || update.sessionId !== session
          || width !== 1280 || height !== 800 || tileSize !== 64 || columns !== 20
          || !Number.isSafeInteger(frameSeq) || frameSeq <= sequence
          || !Number.isFinite(frameTimeMs) || frameTimeMs < 0
          || !Array.isArray(update.tiles) || update.tiles.length > 260) throw Error("Invalid desktop update");
        let changed = false;
        const patches = [];
        try {
          if (update.full !== null) {
            if (!Array.isArray(update.hashes) || update.hashes.length !== 260
              || update.hashes.some((hash) => typeof hash !== "string" || !/^[a-f0-9]{16}$/.test(hash))
              || update.tiles.length) throw Error("Invalid full desktop update");
            const decoded = await decode(update.full, "image/jpeg", width, height);
            patches.push({ ...decoded, x: 0, y: 0 });
            if (!check()) return;
            lastImage = decoded.blob;
            hashes = update.hashes;
          } else {
            if (!lastImage || hashes.length !== 260) throw Error("Desktop update has no base frame");
            const seen = new Set();
            for (const tile of update.tiles) {
              const index = tile.index;
              if (!Number.isInteger(index) || index < 0 || index >= 260 || seen.has(index)
                || tile.x !== (index % columns) * tileSize
                || tile.y !== Math.floor(index / columns) * tileSize
                || tile.width !== Math.min(tileSize, width - tile.x)
                || tile.height !== Math.min(tileSize, height - tile.y)
                || typeof tile.hash !== "string" || !/^[a-f0-9]{16}$/.test(tile.hash)) throw Error("Invalid desktop tile");
              seen.add(index);
              const mime = tile.mime ?? "image/png";
              if (!["image/png", "image/jpeg"].includes(mime)) throw Error("Invalid desktop tile");
            }
            let next = 0;
            const workers = await Promise.allSettled(Array.from({ length: Math.min(4, update.tiles.length) }, async () => {
              while (check() && next < update.tiles.length) {
                const tile = update.tiles[next++];
                const decoded = await decode(tile.data, tile.mime ?? "image/png", tile.width, tile.height);
                patches.push({ ...decoded, x: tile.x, y: tile.y });
              }
            }));
            const failed = workers.find(result => result.status === "rejected");
            if (failed) throw failed.reason;
          }
          if (!check()) return;
          changed = patches.length > 0;
          if (changed) {
            if (canvas.width !== width) canvas.width = width;
            if (canvas.height !== height) canvas.height = height;
            // Decode and validate the entire update before one synchronous
            // paint task. Copy only changed images, never a full canvas snapshot.
            for (const patch of patches) display.drawImage(patch.image, patch.x, patch.y);
            canvas.dataset.paintSeq = String(frameSeq);
          }
        } finally {
          for (const patch of patches) patch.image.close?.();
        }
        sequence = frameSeq;
        emit({ blob: lastImage, seq: frameSeq, timeMs: frameTimeMs, unchanged: !changed });
      }
    } finally {
      clearTimeout(timeout);
      signal.removeEventListener("abort", abort);
    }
  }
}
