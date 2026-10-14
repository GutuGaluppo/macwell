// Guards the copy: same keys in every locale, verbatim COPY.md headline/CTA, and no forbidden claims.
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const locales = ['en', 'pt-br', 'es', 'de'];
const data = Object.fromEntries(await Promise.all(locales.map(async (l) => [l, JSON.parse(await readFile(path.join(root, `../src/content/${l}.json`), 'utf8'))])));
const copyMd = await readFile(path.join(root, '../../docs/landing/COPY.md'), 'utf8');

const keys = (o, p = '') => Object.entries(o).flatMap(([k, v]) => (v && typeof v === 'object' ? keys(v, `${p}${k}.`) : [`${p}${k}`])).sort();
let errors = 0;
const fail = (m) => { console.error('✗', m); errors++; };

const ref = keys(data.en);
for (const l of locales.slice(1)) {
  const k = keys(data[l]);
  for (const x of ref) if (!k.includes(x)) fail(`${l}: missing ${x}`);
  for (const x of k) if (!ref.includes(x)) fail(`${l}: extra ${x}`);
}
// Verbatim checks against the approved deck.
for (const l of locales) {
  for (const s of [data[l].hero.h1, data[l].hero.cta1, data[l].finalCta.title, ...data[l].trust, ...data[l].principles.map((p) => `${p.t}: ${p.d}`), ...data[l].faq.map((f) => `${f.q} ${f.a}`)]) {
    if (!copyMd.includes(s)) fail(`${l}: not verbatim in COPY.md: ${s.slice(0, 60)}`);
  }
}
// Claims the brief forbids (negations such as "not an antivirus" are allowed).
const banned = [/revolutionary|revolucion|revolutionär/i, /AI-powered|con IA|mit KI|com IA/i, /notariz|notariza|notarisier/i, /App Store/i, /★|⭐|stars?\b/i, /award|prêmio|premio|auszeichnung/i, /testimonial|depoimento|testimonio/i];
for (const l of locales) {
  const text = JSON.stringify(data[l]);
  for (const re of banned) if (re.test(text)) fail(`${l}: forbidden claim matches ${re}`);
}
if (errors) process.exit(1);
console.log(`copy ok (${ref.length} keys × ${locales.length} locales)`);
