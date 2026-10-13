/**
 * Two identical file cards drift together, lock with a soft flash, then part again.
 * While the full hash runs, a dashed ring turns around the locked pair.
 * Pure SVG + CSS (see `.dl-*` in index.css); colours come from the theme.
 */
export type DuplicatesPhase = "listing" | "sample" | "full";

function Card({ className }: { className: string }) {
  return (
    <g className={className}>
      <rect width="46" height="58" rx="8" className="fill-[color-mix(in_srgb,var(--accent)_22%,transparent)] stroke-[var(--accent)]" strokeWidth="1.5" />
      <path d="M31 0v12a3 3 0 0 0 3 3h12" fill="none" className="stroke-[var(--accent)]" strokeWidth="1.5" opacity=".7" />
      <rect x="9" y="26" width="28" height="3.5" rx="1.75" className="fill-ink" opacity=".75" />
      <rect x="9" y="35" width="22" height="3.5" rx="1.75" className="fill-ink" opacity=".5" />
      <rect x="9" y="44" width="26" height="3.5" rx="1.75" className="fill-ink" opacity=".35" />
    </g>
  );
}

export function DuplicatesLoader({ phase }: { phase: DuplicatesPhase }) {
  return (
    <svg viewBox="0 0 220 120" className="mx-auto h-[120px] w-[220px]" role="img" aria-label="Procurando arquivos idênticos" data-phase={phase}>
      <g transform="translate(110 60)">
        <circle r="52" fill="none" strokeWidth="2" strokeDasharray="5 9" strokeLinecap="round" className={`stroke-[var(--accent)] dl-ring ${phase === "full" ? "dl-ring-on" : ""}`} />
        <circle r="30" className="dl-flash fill-[var(--accent)]" />
      </g>
      <g transform="translate(0 31)">
        <Card className="dl-left" />
        <Card className="dl-right" />
      </g>
    </svg>
  );
}
