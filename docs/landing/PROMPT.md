# OrganizeMyMac landing page: brief

Read this with `COPY.md` (all text), `MEDIA.md` (assets to capture) and `DEPLOY.md` (hosting constraints).

## Role

You are a senior product designer and front-end engineer. Build a production-ready, multilingual landing page for "OrganizeMyMac", a native-feeling macOS app that finds and safely removes clutter. The page must feel like a premium, trustworthy, high-performance Apple-ecosystem product: calm, precise, never gimmicky.

## Brand identity (derived from the app itself)

- **Look:** deep, vivid gradient canvases (one hue per module) with frosted-glass surfaces (glassmorphism), thin luminous borders, soft inner highlights, generous blur. Dark by default; each section may take the tint of the module it presents. The app's real tokens live in `src/index.css` (`[data-tint="..."]`, `.glass`, `.page-canvas`).
- **Module palette** (section accents, never all at once; tint / glow on deep base):
  - Cleanup `#3ee0a2` / `#0e9f9a` on `#06302a`
  - Security `#6cb0ff` / `#2d5bea` on `#0b1d5a`
  - Projects `#ffd166` / `#ee8a1d` on `#3a2606`
  - Performance `#ffa04d` / `#d9336f` on `#3a1208`
  - Applications `#3fd9f0` / `#2563eb` on `#062a47`
  - My Clutter `#b69bff` / `#7c3aed` on `#1f1052`
  - Smart Care `#f07cf5` / `#b21fd0` on `#360b5e`
  - Settings `#a9c1ee` / `#4a6aa8` on `#0f1727`
  - Default `#8ea2ff` / `#6d4bd8` on `#1b1650`
- **Signature objects:** thick translucent 3D glass icons, front-facing and symmetric, camera slightly above, glass thickness visible only at the bottom, a milky-white frosted glyph in the middle, soft contact shadow. Use the existing PNGs in `public/approved_icons/` (transparent, 1254x1254) as-is; never redraw or restyle them.
- **Layout signature:** the app's bento results grid: one tall tile on the left, one wide tile top-right, two small tiles underneath; 22px rounded glass tiles; a title, a one-line description, a small icon top-right, quiet buttons bottom-right. Echo it in the features section. Reference implementation: `src/components/ResultsBento.tsx`.
- **Typography:** sober and technical. Inter Variable (system SF Pro first on Apple devices); tight tracking on headings (-0.02em); weights 400/500/600 only; no decorative display faces. Monospace (SF Mono / JetBrains Mono) only for file paths and sizes in UI snippets. Headings `clamp(40px, 6vw, 72px)`, body 17-18px, line-height 1.5. Sentence case.
- **Voice:** precise, reassuring, specific. Control and verification, not "magic". No hype words ("revolutionary", "AI-powered"), no fear tactics, no fake urgency.

## Product facts (claim only these)

- macOS app, runs locally; no account; nothing about your files leaves the Mac. The optional "check updates online" action asks for consent first.
- Nothing is removed without a review: every path and size is shown first. Items go to the Trash (recoverable) by default; an optional Quarantine mode moves items to a dated folder with a CSV log.
- System folders (`/System`, `/usr`, `/bin`, `/private`, `/Library`) are blocked. The app never bypasses macOS protections. Works without Full Disk Access, with some protected folders invisible.
- Modules: Assistant (health overview and suggestions), Smart Care (one full analysis), Cleanup (caches, logs, dev-tool caches, browsers, Mail, custom rules), Applications (installed apps, unused for 6+ months, leftovers of removed apps, update sources), My Clutter (large and old files, byte-exact duplicates compared with BLAKE3, visually similar images via Apple Vision), Projects (git repos with unpushed work, regenerable folders like node_modules/target/Pods, safe snapshots), Performance (live CPU, memory, disk, battery, heaviest apps), Security (BETA: read-only audit of start-up items and app signatures; explicitly NOT an antivirus), Space Map and Scanner tools, a menu-bar companion, an ignore list, an operations log, local-only statistics.
- **Do not claim:** antivirus/malware protection, App Store availability, notarization, ratings, user counts, awards, testimonials, a price. Use clearly marked placeholders: `{{PRICE}}`, `{{VERSION}}`, `{{DMG_URL}}`, `{{MIN_MACOS}}`, `{{SHA256}}`. Never fabricate reviews or logos.

## Information architecture (single page, anchored sections)

1. **Sticky glass header:** text wordmark (no icon), anchors (Features, How it works, Privacy, FAQ), language switcher (EN, PT-BR, ES, DE), primary CTA.
2. **Hero:** H1, subhead, two CTAs, a trust strip of 4 claims, and a large looping video of the real app in a window frame with a soft glow in the Cleanup green. Floating glass icons at depth with subtle parallax (off for reduced motion).
3. **Control, not magic:** 4 principle cards (review everything, Trash by default, system folders locked, stays on your Mac), each with a small clip or a static UI crop.
4. **Features bento:** tall + wide + 2 small presenting Cleanup, Applications, My Clutter, Projects; then a compact row for Performance and Security (beta, with an honest "not an antivirus" note). Each tile pairs the module's 3D icon with a real screenshot crop in that module's tint.
5. **How it works:** 3 steps (Analyze, Review, Clean), shown as consecutive frames of the app's intro / scanning / results states.
6. **Show your work:** a close-up of the review dialog (paths, sizes, "Goes to the Trash and can be recovered") as proof of trust, plus the operations log.
7. **Privacy and safety:** local-only, no account, what is read and what is never touched. Placeholder link to the Privacy Policy.
8. **Requirements and download:** `{{MIN_MACOS}}`, architecture, `{{VERSION}}`, `{{PRICE}}`, checksum line (`{{SHA256}}`), release notes link.
9. **FAQ** (accordion, 6 items), final CTA band, footer (language switcher, privacy, contact placeholder, copyright).

## Media rules

- Product video: real captures of the actual app (see `MEDIA.md`), dark mode. Deliver as looping H.264 MP4 + WebM with poster frames; avoid GIF (size/quality), use it only as a fallback. Autoplay muted, `playsinline`, loop; pause off-screen (IntersectionObserver); show the poster under `prefers-reduced-motion`.
- Screenshots: AVIF/WebP with PNG fallback, 2x, window shadow baked in, no personal paths (use `/Users/demo`).
- All images: explicit width/height, lazy-load below the fold, preload the hero poster.

## UI / UX requirements

- **Premium restraint:** whitespace, one focal point per viewport, at most two accent hues per section. Glass cards: background `rgba(255,255,255,.06-.10)`, 1px border `rgba(255,255,255,.14-.2)`, `backdrop-blur 24px`, inner top highlight, large soft shadow. Page background: a deep module-tinted gradient that crossfades as sections enter view (subtle, 600ms).
- **Motion:** purposeful, 200-450ms, ease-out; staggered tile entrance (70ms steps); progress-ring and scan animations lifted from the app (`src/index.css` `stage-in`, `ring-spin`); no scroll-jacking; full `prefers-reduced-motion` support.
- **Accessibility:** WCAG 2.2 AA contrast (verify text over gradients), visible focus rings, keyboard-operable accordion and language switcher, semantic landmarks, alt text on every image, captions/transcripts for videos, 44px touch targets, `lang` per page.
- **Performance budget:** LCP < 2.0s on 4G, CLS < 0.05, INP < 200ms, total JS < 90KB gzip, no render-blocking third-party scripts, self-hosted Inter (subset, `font-display: swap`), videos never block LCP (poster first). Lighthouse >= 95 in all four categories.
- **Responsive:** mobile-first; the bento collapses to one column with the tall tile first; header becomes a menu; the hero video moves below the CTAs on mobile.
- **Trust signals allowed:** the principle cards, the review-dialog proof shot, the privacy section, the checksum line, the honest "Security is beta, not an antivirus" note, a "What it never touches" list. Not allowed: fake badges, star ratings, "as seen in", counters.
- Dark is primary; provide a light variant that keeps the glass look on a soft gray-blue canvas.

## Internationalization

- Four locales on separate routes: `/en`, `/pt-br`, `/es`, `/de`. `hreflang` + `x-default`, localized `<title>`, meta description, Open Graph/Twitter cards, alt text and video captions.
- Locale suggestion from `Accept-Language` as a dismissible banner, never a forced redirect; remember the choice through the URL only. No cookies, no tracking banners.
- Layout must tolerate German length (+30%) and accents; no text baked into images.
- Number formats follow the locale (`16,4 GB` in pt-br/es/de, `16.4 GB` in en).

## Tech

- Static site: Astro (preferred) or Next.js static export, TypeScript, Tailwind with the tokens above as CSS variables, content in per-locale JSON/MD files. Zero runtime dependency for core UI; tiny islands only for the accordion, the language banner and the video lifecycle.
- Semantic HTML, JSON-LD `SoftwareApplication` (`operatingSystem: macOS`, `applicationCategory: UtilitiesApplication`, no rating fields), sitemap, robots, canonical URLs.
- Cookie-less, privacy-friendly analytics only (self-hosted Plausible/Umami), loaded async, optional.
- Hosting target and constraints: see `DEPLOY.md`.

## Deliverables

Complete project (code, tokens, components); the four locale content files built from `COPY.md`; a media checklist with exact filenames and dimensions per video and screenshot; a short README with build, i18n and deploy steps.
