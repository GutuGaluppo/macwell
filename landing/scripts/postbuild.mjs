// Writes dist/_headers (Cloudflare Pages) and verifies the CSP assumptions hold for the built HTML.
import { readdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../dist');
const media = (process.env.PUBLIC_MEDIA_BASE || '').match(/^https?:\/\/[^/]+/)?.[0] ?? '';
const analytics = (process.env.PUBLIC_ANALYTICS_SRC || '').match(/^https?:\/\/[^/]+/)?.[0] ?? '';

const csp = [
  "default-src 'self'",
  `img-src 'self' data: ${media}`.trim(),
  `media-src 'self' ${media}`.trim(),
  "font-src 'self'",
  "style-src 'self'",
  "style-src-attr 'unsafe-inline'",
  `script-src 'self' ${analytics}`.trim(),
  `connect-src 'self' ${analytics}`.trim(),
  "base-uri 'self'",
  "form-action 'self'",
  "frame-ancestors 'none'",
  "object-src 'none'",
].join('; ');

const headers = `/*
  Content-Security-Policy: ${csp}
  Strict-Transport-Security: max-age=63072000; includeSubDomains; preload
  X-Content-Type-Options: nosniff
  Referrer-Policy: strict-origin-when-cross-origin
  Permissions-Policy: camera=(), microphone=(), geolocation=(), interest-cohort=()

/_astro/*
  Cache-Control: public, max-age=31536000, immutable

/img/*
  Cache-Control: public, max-age=31536000, immutable

/media/*
  Cache-Control: public, max-age=31536000, immutable
`;
await writeFile(path.join(dist, '_headers'), headers);

// Fail the build if any page ships an executable inline script (it would be blocked by the CSP).
async function* walk(dir) {
  for (const e of await readdir(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) yield* walk(p);
    else if (p.endsWith('.html')) yield p;
  }
}
let bad = 0;
for await (const file of walk(dist)) {
  const html = await readFile(file, 'utf8');
  for (const m of html.matchAll(/<script(?![^>]*\bsrc=)(?![^>]*type="application\/ld\+json")[^>]*>/g)) {
    console.error(`inline script in ${path.relative(dist, file)}: ${m[0]}`);
    bad++;
  }
}
if (bad) process.exit(1);
console.log('postbuild ok: _headers written, no inline scripts');
