// Rasterize the small geometric app mark without a browser or image dependency.
import { writeFile } from "node:fs/promises";
import { deflateSync } from "node:zlib";
function crc(bytes) {
  let n = 0xffffffff;
  for (const b of bytes) {
    n ^= b;
    for (let i = 0; i < 8; i++) n = (n >>> 1) ^ (n & 1 ? 0xedb88320 : 0);
  }
  return (n ^ 0xffffffff) >>> 0;
}
function chunk(name, data) {
  const kind = Buffer.from(name),
    out = Buffer.alloc(data.length + 12);
  out.writeUInt32BE(data.length);
  kind.copy(out, 4);
  data.copy(out, 8);
  out.writeUInt32BE(crc(Buffer.concat([kind, data])), out.length - 4);
  return out;
}
const hex = [
  [96, 35],
  [150, 66],
  [150, 126],
  [96, 157],
  [42, 126],
  [42, 66],
  [96, 35],
];
const segments = [
  ...hex.slice(0, -1).map((p, i) => [p, hex[i + 1], 4]),
  [[69, 82], [96, 98], 3.5],
  [[96, 98], [123, 82], 3.5],
  [[96, 98], [96, 129], 3.5],
];
function distance(x, y, a, b) {
  const dx = b[0] - a[0],
    dy = b[1] - a[1],
    t = Math.max(
      0,
      Math.min(1, ((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy)),
    );
  return Math.hypot(x - a[0] - t * dx, y - a[1] - t * dy);
}
export async function icons() {
  for (const size of [192, 512]) {
    const raw = Buffer.alloc((size * 4 + 1) * size);
    for (let y = 0; y < size; y++)
      for (let x = 0; x < size; x++) {
        let coverage = 0;
        for (const [sx, sy] of [
          [0.25, 0.25],
          [0.75, 0.25],
          [0.25, 0.75],
          [0.75, 0.75],
        ]) {
          const xx = ((x + sx) * 192) / size,
            yy = ((y + sy) * 192) / size;
          if (segments.some(([a, b, width]) => distance(xx, yy, a, b) <= width))
            coverage++;
        }
        const at = y * (size * 4 + 1) + 1 + x * 4;
        for (let c = 0; c < 3; c++)
          raw[at + c] = Math.round(
            [20, 25, 22][c] +
              (coverage / 4) * ([182, 217, 169][c] - [20, 25, 22][c]),
          );
        raw[at + 3] = 255;
      }
    const header = Buffer.alloc(13);
    header.writeUInt32BE(size);
    header.writeUInt32BE(size, 4);
    header[8] = 8;
    header[9] = 6;
    await writeFile(
      `dist/icon-${size}.png`,
      Buffer.concat([
        Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
        chunk("IHDR", header),
        chunk("IDAT", deflateSync(raw)),
        chunk("IEND", Buffer.alloc(0)),
      ]),
    );
  }
}
