// Derives web-sized AVIF/WebP from the approved PNGs (untouched originals stay in ../public/approved_icons).
import sharp from 'sharp';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const src = path.join(root, '../../public/approved_icons');
const out = path.join(root, '../public/img/icons');
const map = {
  app: 'OrganizeMyMac-icon.png',
  clean: 'Clean.png',
  apps: 'Aplicativos.png',
  clutter: 'Mess.png',
  projects: 'Projetos.png',
  performance: 'Desempenho.png',
  security: 'Seguranca.png',
  smartcare: 'Cuidado inteligente.png',
  home: 'Home.png',
  spacemap: 'Mapa de espaço.png',
  scanner: 'Scanner.png',
  settings: 'Ajustes.png',
};
await mkdir(out, { recursive: true });
for (const [slug, file] of Object.entries(map)) {
  for (const w of [320, 640]) {
    const img = sharp(path.join(src, file)).resize(w, w, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } });
    await img.clone().avif({ quality: 62, effort: 6 }).toFile(path.join(out, `${slug}-${w}.avif`));
    await img.clone().webp({ quality: 82 }).toFile(path.join(out, `${slug}-${w}.webp`));
  }
}

// Open Graph card: deep gradient + the app icon, no text (copy stays localized in meta tags).
const icon = await sharp(path.join(src, map.app)).resize(420, 420).toBuffer();
const bg = Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630">
<defs><radialGradient id="a" cx="85%" cy="0%" r="80%"><stop offset="0" stop-color="#0e9f9a" stop-opacity=".75"/><stop offset="1" stop-color="#0e9f9a" stop-opacity="0"/></radialGradient>
<radialGradient id="b" cx="10%" cy="100%" r="70%"><stop offset="0" stop-color="#3ee0a2" stop-opacity=".4"/><stop offset="1" stop-color="#3ee0a2" stop-opacity="0"/></radialGradient></defs>
<rect width="1200" height="630" fill="#06302a"/><rect width="1200" height="630" fill="url(#a)"/><rect width="1200" height="630" fill="url(#b)"/></svg>`);
await sharp(bg).composite([{ input: icon, left: 390, top: 105 }]).png({ compressionLevel: 9 }).toFile(path.join(root, '../public/og.png'));
console.log('icons + og.png done');
