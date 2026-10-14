# Hosting decision and constraints

**Target: Cloudflare Pages (static site) + Cloudflare R2 (installer and videos).**

## Why

- The page is media-heavy and serves a DMG; Cloudflare Pages/R2 do not bill egress, and the free plan allows commercial use. Some alternatives restrict the free plan to non-commercial projects and meter bandwidth.
- Global edge with strong presence in Europe and Latin America (the four launch languages).
- Workers give header control (CSP, HSTS, cache) and a cookie-less `Accept-Language` locale suggestion.
- The same account can host a stable DMG URL, the app's static updater JSON, and cookie-less analytics, plus the domain.

Alternatives considered: Vercel (best DX and previews, but free plan is non-commercial and bandwidth-metered; pick it only if the site becomes dynamic), Netlify (balanced, no decisive edge), GitHub Pages (no custom headers, 100 MB file limit, not for binaries), S3 + CloudFront (more setup than needed). Plan limits change: confirm on each provider's pricing page.

## Constraints for the build

- Output must be fully static (Astro build or Next static export); no server runtime in the page.
- Videos and the DMG are served from R2 under a stable public URL (`{{DMG_URL}}`), not from the site bundle (Pages has a per-file size limit).
- Locale routes `/en`, `/pt-br`, `/es`, `/de`; the locale suggestion banner is the only edge logic. No forced redirects, no cookies.
- Preview deploys per branch on Pages; custom domain with HSTS.

## Before the download button goes live

The app is not notarized yet (pending in the project). macOS blocks apps downloaded outside the App Store unless they are signed with a Developer ID and notarized. Until that is done, do not promise "download and open"; ship the page with the download CTA disabled or pointing to a waitlist, and publish the DMG's SHA-256 (`{{SHA256}}`) when available.
