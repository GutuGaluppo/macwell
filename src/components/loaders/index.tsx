import type { CSSProperties, ReactNode } from "react";

/**
 * Themed loaders for the analyses. Pure SVG + CSS (`.al-*` in index.css), colours from the theme,
 * only transform/opacity are animated. The global reduced-motion rule turns them into still frames.
 * The Duplicados loader lives in DuplicatesLoader.tsx.
 */
export { DuplicatesLoader } from "./DuplicatesLoader";

const ACCENT = "stroke-[var(--accent)]";
const FILL = "fill-[var(--accent)]";
const SOFT = "fill-[color-mix(in_srgb,var(--accent)_26%,transparent)]";

function Frame({ label, children }: { label: string; children: ReactNode }) {
  return (
    <svg viewBox="0 0 220 120" className="mx-auto h-[120px] w-[220px]" role="img" aria-label={label}>
      {children}
    </svg>
  );
}

const delay = (s: number): CSSProperties => ({ animationDelay: `${s}s` });

/** Disk scan: a treemap draws itself rectangle by rectangle. */
export function ScanLoader() {
  const rects = [
    [30, 14, 80, 56],
    [114, 14, 76, 34],
    [114, 52, 36, 18],
    [154, 52, 36, 18],
    [30, 74, 50, 30],
    [84, 74, 58, 30],
    [146, 74, 44, 30],
  ];
  return (
    <Frame label="Medindo o espaço">
      {rects.map(([x, y, w, h], i) => (
        <rect key={i} x={x} y={y} width={w} height={h} rx="5" className={`al-treemap ${SOFT} ${ACCENT}`} strokeWidth="1.2" style={delay(i * 0.28)} />
      ))}
    </Frame>
  );
}

/** Similar images: scattered thumbnails drift into matching pairs, each pair gets a focus frame. */
export function ImagesLoader() {
  // [startX, startY, endX, endY, hue shift]
  const tiles: [number, number, number, number, number][] = [
    [30, 18, 56, 34, 0],
    [160, 70, 56 + 50, 34, 0],
    [44, 74, 114, 34, 1],
    [150, 16, 114 + 50, 34, 1],
    [96, 84, 56, 76, 2],
    [100, 8, 56 + 50, 76, 2],
  ];
  const shades = ["var(--accent)", "color-mix(in srgb, var(--accent) 55%, white)", "color-mix(in srgb, var(--accent) 60%, black)"];
  return (
    <Frame label="Comparando imagens">
      {tiles.map(([sx, sy, ex, ey, h], i) => (
        <rect
          key={i}
          width="44"
          height="32"
          rx="6"
          className="al-tile"
          fill={shades[h]}
          style={{ "--sx": `${sx}px`, "--sy": `${sy}px`, "--ex": `${ex}px`, "--ey": `${ey}px` } as CSSProperties}
        />
      ))}
    </Frame>
  );
}

/** Installed apps: a 3×3 grid lights up in a diagonal wave. `ghost` fades a few cells like leftovers. */
export function AppsLoader({ ghost = false }: { ghost?: boolean }) {
  const cells = Array.from({ length: 9 }, (_, i) => ({ r: Math.floor(i / 3), c: i % 3, i }));
  return (
    <Frame label="Lendo os aplicativos">
      {cells.map(({ r, c, i }) => (
        <rect
          key={i}
          x={64 + c * 30}
          y={16 + r * 30}
          width="24"
          height="24"
          rx="7"
          className={`al-cell ${FILL} ${ghost && (i === 1 || i === 5 || i === 6) ? "al-cell-ghost" : ""}`}
          style={delay((r + c) * 0.18)}
        />
      ))}
    </Frame>
  );
}

/** Repositories: a git-style graph, trunk first, then branches and commit nodes. */
export function ProjectsLoader() {
  return (
    <Frame label="Procurando repositórios">
      <path d="M70 14V106" fill="none" strokeWidth="3" strokeLinecap="round" className={`al-draw ${ACCENT}`} pathLength={1} />
      <path d="M70 40C70 60 120 44 120 66V92" fill="none" strokeWidth="3" strokeLinecap="round" className={`al-draw ${ACCENT}`} pathLength={1} style={delay(0.5)} />
      <path d="M70 76C70 90 150 70 150 92" fill="none" strokeWidth="3" strokeLinecap="round" opacity=".6" className={`al-draw ${ACCENT}`} pathLength={1} style={delay(1)} />
      {[[70, 14, 0], [70, 40, 0.3], [70, 76, 0.7], [70, 106, 0.9], [120, 66, 0.9], [120, 92, 1.1], [150, 92, 1.5]].map(([x, y, d], i) => (
        <circle key={i} cx={x} cy={y} r="6" className={`al-node fill-[var(--deep,#1b1650)] ${ACCENT}`} strokeWidth="3" style={delay(d)} />
      ))}
    </Frame>
  );
}

/** Security audit: a light band sweeps down a shield. */
export function SecurityLoader() {
  const shield = "M110 12 L158 28 V60 C158 86 138 102 110 112 C82 102 62 86 62 60 V28 Z";
  return (
    <Frame label="Verificando o sistema">
      <defs>
        <clipPath id="al-shield">
          <path d={shield} />
        </clipPath>
        <linearGradient id="al-beam" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="white" stopOpacity="0" />
          <stop offset=".5" stopColor="white" stopOpacity=".7" />
          <stop offset="1" stopColor="white" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={shield} className={`${SOFT} ${ACCENT}`} strokeWidth="2.5" strokeLinejoin="round" />
      <g clipPath="url(#al-shield)">
        <rect x="60" y="-30" width="100" height="30" fill="url(#al-beam)" className="al-beam" />
      </g>
    </Frame>
  );
}

/** Cloud usage: a cloud floats while dots travel up and down, like syncing. */
export function CloudLoader() {
  return (
    <Frame label="Medindo a nuvem">
      <g className="al-float">
        <path
          d="M82 70a20 20 0 0 1 4-39 28 28 0 0 1 53-4 24 24 0 0 1 3 43z"
          className={`${SOFT} ${ACCENT}`}
          strokeWidth="2.5"
          strokeLinejoin="round"
          transform="translate(-3 -2)"
        />
      </g>
      {[86, 110, 134].map((x, i) => (
        <circle key={x} cx={x} cy="96" r="3.5" className={`al-sync ${FILL}`} style={delay(i * 0.35)} />
      ))}
    </Frame>
  );
}

/** Update check (network): a sonar ping spreads from a dot. */
export function UpdatesLoader() {
  return (
    <Frame label="Consultando atualizações">
      {[0, 0.8, 1.6].map((d) => (
        <circle key={d} cx="110" cy="60" r="44" fill="none" strokeWidth="2" className={`al-ping ${ACCENT}`} style={delay(d)} />
      ))}
      <circle cx="110" cy="60" r="7" className={FILL} />
    </Frame>
  );
}

/** Orbiting sparkles around a hero icon (Cuidado inteligente). */
export function Orbit({ size, children }: { size: number; children: ReactNode }) {
  return (
    <div className="relative grid place-items-center" style={{ width: size * 1.5, height: size * 1.5 }}>
      <svg viewBox="-60 -60 120 120" className="absolute inset-0 h-full w-full" aria-hidden>
        <g className="al-orbit">
          {[0, 120, 240].map((a, i) => (
            <path
              key={a}
              d="M0 -6 L1.6 -1.6 L6 0 L1.6 1.6 L0 6 L-1.6 1.6 L-6 0 L-1.6 -1.6 Z"
              className={FILL}
              transform={`rotate(${a}) translate(0 -52) scale(${i === 0 ? 1.2 : 0.8})`}
              opacity={i === 0 ? 1 : 0.7}
            />
          ))}
        </g>
      </svg>
      <div className="al-breathe">{children}</div>
    </div>
  );
}
