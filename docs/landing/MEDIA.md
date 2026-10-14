# Media to capture

All from the real app (`npm run tauri dev`, or the built app), dark mode, 1200x780 window, demo data only (paths under `/Users/demo`, no personal files). Nothing here has been captured yet.

## Videos (6-10 s each, looping, deliberate cursor)

Source at 60 fps. Export per clip: `name.mp4` (H.264), `name.webm` (VP9/AV1), `name-poster.avif` (+ `.jpg` fallback). Target under 1.5 MB per clip.

| File | What it shows |
|---|---|
| `hero-cleanup` | Cleanup: intro → Analyze → scanning with the progress ring → bento results (the hero loop) |
| `assistant` | Assistant: health ring and suggestion cards |
| `review-dialog` | Review dialog opening, paths and sizes, then "Move to Trash" |
| `clutter` | My Clutter: duplicates and similar images |
| `applications` | Applications: unused apps and leftovers |
| `performance` | Performance: live gauges and the process list |
| `projects` | Projects: unpushed repos and regenerable folders |
| `menubar` | Menu-bar companion |
| `sidebar-rail` | Sidebar collapsing to the icon rail when a module starts scanning |

## Screenshots (PNG 2x, window shadow baked in)

`assistant`, `cleanup-results`, `applications-results`, `clutter-results`, `projects-results`, `performance-results`, `security-results` (beta), `review-dialog`, `operations-log`, `settings`. One tint per screen, as in the app.

## Icons

Use `public/approved_icons/*.png` untouched: Aplicativos, Clean, Cuidado inteligente, Desempenho, Home, Mapa de espaço, Mess, OrganizeMyMac-icon, Scanner, Seguranca, Projetos, Ajustes. Export AVIF/WebP at 1x and 2x for the page.

## Rules

- No text baked into images (all copy is localized).
- Every asset gets explicit width/height, alt text per locale, and a caption/transcript for videos.
- Record the same flows once; locales share the visuals (the app UI is in Portuguese today: decide whether to record an English UI pass or keep the Portuguese UI with localized captions).
