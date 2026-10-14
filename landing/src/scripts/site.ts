// Small islands: tint crossfade, staggered reveal, video lifecycle, parallax, language suggestion.
// No cookies, no storage: a dismissed banner stays dismissed only until the page reloads.
const doc = document.documentElement;
doc.classList.add('js');
const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;

// Background tint follows the section in view.
const sections = document.querySelectorAll<HTMLElement>('[data-tint-section]');
if ('IntersectionObserver' in window && sections.length) {
  const visible = new Map<Element, number>();
  const tintObserver = new IntersectionObserver(
    (entries) => {
      for (const e of entries) e.isIntersecting ? visible.set(e.target, e.intersectionRatio) : visible.delete(e.target);
      let best: Element | null = null;
      let score = 0;
      visible.forEach((r, el) => { if (r >= score) { score = r; best = el; } });
      const tint = (best as HTMLElement | null)?.dataset.tintSection;
      if (tint) document.body.dataset.tint = tint;
    },
    { rootMargin: '-35% 0px -35% 0px', threshold: [0, 0.25, 0.5, 0.75, 1] },
  );
  sections.forEach((s) => tintObserver.observe(s));
}

// Staggered entrance.
const reveals = document.querySelectorAll<HTMLElement>('.reveal');
if (reduce || !('IntersectionObserver' in window)) {
  reveals.forEach((r) => r.classList.add('in'));
} else {
  const revealObserver = new IntersectionObserver(
    (entries) => entries.forEach((e) => { if (e.isIntersecting) { e.target.classList.add('in'); revealObserver.unobserve(e.target); } }),
    { threshold: 0.15 },
  );
  reveals.forEach((r) => revealObserver.observe(r));
  setTimeout(() => reveals.forEach((r) => r.classList.add('in')), 2500); // never leave content hidden
}

// Videos play only while visible; reduced motion keeps the poster.
const videos = document.querySelectorAll<HTMLVideoElement>('video.app-video');
if (!reduce && 'IntersectionObserver' in window) {
  const videoObserver = new IntersectionObserver((entries) => {
    for (const e of entries) {
      const v = e.target as HTMLVideoElement;
      if (e.isIntersecting) { if (v.preload === 'none') v.preload = 'auto'; v.play().catch(() => {}); } else v.pause();
    }
  }, { threshold: 0.35 });
  videos.forEach((v) => videoObserver.observe(v));
}

// Subtle parallax on floating icons.
const floats = document.querySelectorAll<HTMLElement>('[data-depth]');
if (!reduce && floats.length) {
  let ticking = false;
  addEventListener('scroll', () => {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(() => {
      const y = scrollY;
      floats.forEach((f) => { f.style.transform = `translate3d(0, ${(-y * Number(f.dataset.depth)).toFixed(1)}px, 0)`; });
      ticking = false;
    });
  }, { passive: true });
}

// Language suggestion banner: dismissible, never a redirect.
const banner = document.querySelector<HTMLElement>('[data-lang-banner]');
if (banner) {
  const current = doc.lang.toLowerCase();
  const prefs = (navigator.languages?.length ? navigator.languages : [navigator.language]).map((l) => l.toLowerCase());
  const map = (l: string) => (l.startsWith('pt') ? 'pt-br' : l.startsWith('es') ? 'es' : l.startsWith('de') ? 'de' : l.startsWith('en') ? 'en' : '');
  const wanted = map(prefs[0] ?? '');
  const here = map(current);
  const target = wanted && wanted !== here ? banner.querySelector<HTMLElement>(`[data-for="${wanted}"]`) : null;
  if (target) {
    banner.hidden = false;
    banner.querySelectorAll<HTMLElement>('[data-for]').forEach((el) => { el.hidden = el !== target; });
    banner.querySelector('[data-dismiss]')?.addEventListener('click', () => { banner.hidden = true; });
  }
}

// Mobile menu.
const menu = document.querySelector<HTMLElement>('.menu');
const menuBtn = document.querySelector<HTMLButtonElement>('[data-menu-btn]');
if (menu && menuBtn) {
  const set = (open: boolean) => { menu.classList.toggle('open', open); menuBtn.setAttribute('aria-expanded', String(open)); };
  menuBtn.addEventListener('click', () => set(!menu.classList.contains('open')));
  menu.addEventListener('click', (e) => { if ((e.target as HTMLElement).closest('a')) set(false); });
  addEventListener('keydown', (e) => { if (e.key === 'Escape') set(false); });
}
