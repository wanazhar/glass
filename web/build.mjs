import { icons } from "./build-icons.mjs";
import { build, transform } from "esbuild";
import { cp, mkdir, readdir, readFile, writeFile, rm } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { createHash } from "node:crypto";
await rm("dist", { recursive: true, force: true });
await mkdir("dist", { recursive: true });
const result = await build({
  entryPoints: ["src/app.js"],
  bundle: true,
  splitting: true,
  format: "esm",
  outdir: "dist",
  minify: true,
  target: ["safari16.4", "chrome110"],
  metafile: true,
  chunkNames: "chunks/[name]-[hash]",
  assetNames: "assets/[name]-[hash]",
  loader: { ".woff": "file", ".woff2": "file", ".ttf": "file" },
  logLevel: "info",
});
await cp(
  "node_modules/pdfjs-dist/build/pdf.worker.min.mjs",
  "dist/pdf.worker.min.mjs",
);
await cp("dist/app.css", "dist/math.css");
await cp("src/index.html", "dist/index.html");
await mkdir("dist/fonts", { recursive: true });
await cp(
  "node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2",
  "dist/fonts/inter-latin.woff2",
);
await cp(
  "node_modules/@fontsource-variable/jetbrains-mono/files/jetbrains-mono-latin-wght-normal.woff2",
  "dist/fonts/jetbrains-mono-latin.woff2",
);
await writeFile(
  "dist/style.css",
  (
    await transform(await readFile("src/style.css", "utf8"), {
      loader: "css",
      minify: true,
      target: ["safari16.4", "chrome110"],
    })
  ).code,
);
await cp("src/icon.svg", "dist/icon.svg");
await cp("src/manifest.webmanifest", "dist/manifest.webmanifest");
await icons();
const version = createHash("sha256")
  .update(await readFile("dist/app.js"))
  .update(await readFile("dist/style.css"))
  .digest("hex")
  .slice(0, 12);
await writeFile(
  "dist/sw.js",
  (await readFile("src/sw.js", "utf8")).replaceAll("__VERSION__", version),
);
async function zip(dir) {
  for (const e of await readdir(dir, { withFileTypes: true })) {
    const p = dir + "/" + e.name;
    if (e.isDirectory()) await zip(p);
    else if (/\.(mjs|js|css|html|svg|webmanifest)$/.test(p))
      await writeFile(p + ".gz", gzipSync(await readFile(p), { level: 9 }));
  }
}
let licenses = "Glass Web third-party notices\n\n";
async function collect(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
    const path = dir + "/" + entry.name;
    if (entry.name.startsWith("@")) {
      await collect(path);
      continue;
    }
    try {
      const pkg = JSON.parse(await readFile(path + "/package.json", "utf8"));
      const names = (await readdir(path)).filter((n) =>
        /^licen[cs]e(?:\.|$)/i.test(n),
      );
      for (const name of names) {
        try {
          licenses +=
            "\n--- " +
            pkg.name +
            " " +
            pkg.version +
            " ---\n" +
            (await readFile(path + "/" + name, "utf8")) +
            "\n";
        } catch {}
      }
    } catch {}
  }
}
await collect("node_modules");
await writeFile("dist/THIRD_PARTY_NOTICES.txt", licenses);
await zip("dist");
await writeFile(
  "dist/build.json",
  JSON.stringify(
    {
      version,
      outputs: Object.fromEntries(
        Object.entries(result.metafile.outputs).map(([p, v]) => [p, v.bytes]),
      ),
    },
    null,
    2,
  ),
);
