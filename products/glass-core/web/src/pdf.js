import { getDocument, GlobalWorkerOptions } from "pdfjs-dist";
GlobalWorkerOptions.workerSrc = "/pdf.worker.min.mjs";
export async function pdf(container, bytes, signal) {
  let task = null,
    closed = false;
  const loading = getDocument({
    data: new Uint8Array(bytes),
    isEvalSupported: false,
    useSystemFonts: true,
    disableFontFace: true,
    maxImageSize: 16000000,
    canvasMaxAreaInBytes: 16000000,
  });
  const cleanup = () => {
    closed = true;
    task?.cancel();
    loading.destroy().catch(() => {});
  };
  signal?.addEventListener("abort", cleanup, { once: true });
  if (signal?.aborted) {
    cleanup();
    return;
  }
  const doc = await loading.promise;
  if (closed) return;
  if (doc.numPages > 500) {
    await loading.destroy();
    throw Error("PDF exceeds the 500-page preview limit. Download to view it.");
  }
  const controls = document.createElement("div");
  controls.className = "pdf-controls";
  const previous = document.createElement("button"),
    next = document.createElement("button"),
    label = document.createElement("span");
  previous.textContent = "← Previous";
  next.textContent = "Next →";
  previous.className = next.className = "quiet";
  controls.append(previous, label, next);
  const canvas = document.createElement("canvas");
  canvas.style.maxWidth = "100%";
  canvas.style.display = "block";
  canvas.style.margin = "20px auto";
  container.append(controls, canvas);
  let index = 1;
  async function draw() {
    previous.disabled = next.disabled = true;
    label.textContent = `Page ${index} of ${doc.numPages}`;
    const page = await doc.getPage(index);
    if (closed) return;
    const natural = page.getViewport({ scale: 1 });
    const scale = Math.min(
      1.5,
      Math.sqrt(4_000_000 / (natural.width * natural.height)),
    );
    const viewport = page.getViewport({ scale });
    canvas.width = Math.ceil(viewport.width);
    canvas.height = Math.ceil(viewport.height);
    task = page.render({ canvasContext: canvas.getContext("2d"), viewport });
    await task.promise;
    previous.disabled = index === 1;
    next.disabled = index === doc.numPages;
    page.cleanup();
  }
  previous.onclick = () => {
    if (index > 1) {
      index--;
      draw().catch((error) => {
        if (!closed) container.append(document.createTextNode(error.message));
      });
    }
  };
  next.onclick = () => {
    if (index < doc.numPages) {
      index++;
      draw().catch((error) => {
        if (!closed) container.append(document.createTextNode(error.message));
      });
    }
  };
  await draw();
}
