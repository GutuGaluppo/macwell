# OrganizeMyMac landing page

Static Astro site (TypeScript, Tailwind 4). Four locales on separate routes: `/en`, `/pt-br`, `/es`, `/de`. `/` is a language picker. Spec: `../docs/landing/`. Things only the owner can decide or do: `../docs/landing/MANUAL.md`.

## Run

```bash
cd landing && npm install
npm run dev          # http://localhost:4321/en/
npm run build        # astro check + build + postbuild (writes dist/_headers, fails on inline scripts)
npm run check:copy   # same keys in every locale, verbatim vs COPY.md, no forbidden claims
npm run icons        # regenerates AVIF/WebP icons + og.png from ../public/approved_icons (originals untouched)
```

## Where things live

| What | Where |
|---|---|
| Copy per locale | `src/content/{en,pt-br,es,de}.json` (built from `docs/landing/COPY.md`) |
| Undecided values | `src/config.ts` reads `PUBLIC_*` env vars (see `.env.example`); empty = visible `{{PLACEHOLDER}}` |
| Download button | `PUBLIC_DOWNLOAD_LIVE` (default `false` = waitlist notice). Flip only after notarization |
| Design tokens, glass, bento | `src/styles/global.css` (module tints match the app's `src/index.css`) |
| Page | `src/pages/[locale]/index.astro`; components in `src/components` |
| Small JS island (1 KB gz) | `src/scripts/site.ts`: tint crossfade, reveal, video lifecycle, parallax, language banner, menu |

## Add a language

1. Copy `src/content/en.json` to `<code>.json` and translate. 2. Register it in `src/i18n/locales.ts` (`LOCALES`, `content`, `meta`, `banner`) and in `astro.config.mjs` sitemap locales. 3. `npm run check:copy`.

## Media

Slots render a neutral placeholder until the real file exists, then switch automatically (same aspect ratio, no layout shift).

- Screenshots: `node scripts/build-shots.mjs <dir>` turns `<name>.png` files into `public/media/shots/<name>.{avif,webp,png}`.
- Videos: `scripts/encode-video.sh <raw.mov> <name>` writes `public/media/video/<name>.{mp4,webm}` + poster (needs ffmpeg).
- Names the page looks for: `hero-cleanup` (video), `cleanup-results`, `applications-results`, `clutter-results`, `projects-results`, `step-analyze`, `step-review`, `step-clean`, `review-dialog`, `operations-log`. Others from `MEDIA.md` are for later sections.
- Production: upload `public/media/**` to R2 and set `PUBLIC_MEDIA_BASE=https://<media-domain>` (the build then references R2 and adds it to the CSP).

## Deploy (Cloudflare Pages)

```bash
npm i -D wrangler && npx wrangler login      # once, by the owner
SITE_URL=https://<domain> PUBLIC_MEDIA_BASE=https://<media-domain> npm run deploy
```

Or connect the repo in the Cloudflare dashboard: root `landing`, build `npm run build`, output `dist`. Headers (CSP, HSTS, cache) come from `dist/_headers`; there is no server code. The language suggestion is client-side, dismissible, and never redirects or stores anything.

## Measured

Lighthouse (mobile, local build, /en/): performance 100, accessibility 100, best-practices 100, SEO 100; LCP 1.6 s, CLS 0, TBT 0. JS 1 KB gzip. Re-measure once real videos are in.
