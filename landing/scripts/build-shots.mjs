// Usage: node scripts/build-shots.mjs <dir-with-PNGs>
// Each <name>.png (macOS window screenshot, 2x, shadow included) becomes public/media/shots/<name>.{avif,webp,png}.
import sharp from 'sharp';
import { mkdir, readdir } from 'node:fs/promises';
import path from 'node:path';

const input = process.argv[2];
if (!input) { console.error('usage: node scripts/build-shots.mjs <dir>'); process.exit(1); }
const out = path.resolve('public/media/shots');
await mkdir(out, { recursive: true });
for (const f of (await readdir(input)).filter((f) => f.toLowerCase().endsWith('.png'))) {
  const name = path.basename(f, path.extname(f));
  const img = sharp(path.join(input, f)).resize({ width: 2400, withoutEnlargement: true });
  await img.clone().avif({ quality: 60, effort: 6 }).toFile(path.join(out, `${name}.avif`));
  await img.clone().webp({ quality: 82 }).toFile(path.join(out, `${name}.webp`));
  await img.clone().png({ compressionLevel: 9 }).toFile(path.join(out, `${name}.png`));
  console.log('✓', name);
}
