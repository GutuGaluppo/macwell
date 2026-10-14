import type { ReactNode } from "react";

export type BentoTile = {
  id: string;
  title: ReactNode;
  description?: ReactNode;
  /** Illustration: large and centred on the tall tile, small in the corner on the others. */
  visual?: ReactNode;
  actions: ReactNode;
};

// The results frame of a module: a headline with a pill, then four glass tiles. The first stands
// tall on the left, the second spans the right, the last two sit side by side underneath.
export function ResultsBento({ headline, pill, extra, tiles }: { headline: ReactNode; pill?: ReactNode; extra?: ReactNode; tiles: [BentoTile, BentoTile, BentoTile, BentoTile] }) {
  const [tall, wide, a, b] = tiles;
  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto px-8 pb-8">
      <div className="flex shrink-0 flex-col items-center gap-4 pt-6 pb-8 text-center">
        <h1 className="text-[30px] leading-tight font-medium tracking-tight text-ink">{headline}</h1>
        <div className="flex items-center gap-2">
          {pill}
          {extra}
        </div>
      </div>
      <div className="mx-auto grid min-h-[430px] w-full max-w-[1000px] flex-1 grid-cols-3 grid-rows-2 gap-4">
        <Tile tile={tall} variant="tall" className="row-span-2" />
        <Tile tile={wide} variant="wide" className="col-span-2" />
        <Tile tile={a} variant="small" />
        <Tile tile={b} variant="small" />
      </div>
    </div>
  );
}

function Tile({ tile, variant, className = "" }: { tile: BentoTile; variant: "tall" | "wide" | "small"; className?: string }) {
  return (
    <section className={`glass relative flex min-h-0 min-w-0 flex-col overflow-hidden rounded-[22px] p-5 ${className}`}>
      {variant !== "tall" && tile.visual && <div className="pointer-events-none absolute top-4 right-4">{tile.visual}</div>}
      <h2 className={`font-semibold leading-snug tracking-tight text-ink ${variant === "small" ? "max-w-[78%] text-[17px]" : "max-w-[70%] text-[19px]"}`}>{tile.title}</h2>
      {tile.description && <p className={`mt-1.5 text-[13px] leading-snug text-ink-2 ${variant === "small" ? "line-clamp-3 max-w-[78%]" : "max-w-[78%]"}`}>{tile.description}</p>}
      {variant === "tall" && <div className="pointer-events-none flex min-h-0 flex-1 items-center justify-center">{tile.visual}</div>}
      <div className="mt-auto flex items-center justify-end gap-2 pt-4">{tile.actions}</div>
    </section>
  );
}

export function BentoButton({ children, onClick, primary, disabled }: { children: ReactNode; onClick: () => void; primary?: boolean; disabled?: boolean }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`h-8 rounded-lg px-3.5 text-[13px] font-semibold transition disabled:pointer-events-none disabled:opacity-40 ${
        primary ? "bg-white/90 text-[#10231c] hover:bg-white" : "border border-white/20 bg-white/12 text-ink hover:bg-white/20"
      }`}
    >
      {children}
    </button>
  );
}

/** A glossy rounded square holding an icon, in the module's tint. */
export function BentoIcon({ children, size = 56 }: { children: ReactNode; size?: number }) {
  return (
    <span
      style={{ width: size, height: size }}
      className="grid place-items-center rounded-[26%] border border-white/30 bg-[linear-gradient(160deg,var(--tint),var(--glow))] text-white shadow-[inset_0_2px_0_rgb(255_255_255/0.35),0_10px_22px_-8px_rgb(0_0_0/0.5)] [&_svg]:size-1/2"
    >
      {children}
    </span>
  );
}
