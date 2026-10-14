import { useEffect, type ReactNode } from "react";
import { ChevronLeft } from "lucide-react";
import { useStage } from "../stores/stage";
import { RoundAction } from "./GlassTile";

export type StagePhase = "intro" | "scanning" | "results";

// The Stop button with a ring around it: a spinning arc while the work has no known length,
// a filling arc when it has one.
function ScanRing({ progress, children }: { progress?: number; children: ReactNode }) {
  const r = 52;
  const c = 2 * Math.PI * r;
  const known = progress !== undefined;
  return (
    <div className="relative grid size-[84px] place-items-center">
      <svg viewBox="0 0 120 120" className="pointer-events-none absolute -inset-[18px] size-[120px] -rotate-90">
        <circle cx="60" cy="60" r={r} fill="none" stroke="rgb(255 255 255 / 0.16)" strokeWidth="4" />
        <circle
          cx="60"
          cy="60"
          r={r}
          fill="none"
          stroke="white"
          strokeWidth="4"
          strokeLinecap="round"
          strokeDasharray={`${c * (known ? Math.max(0.03, Math.min(1, progress)) : 0.25)} ${c}`}
          className={known ? "transition-[stroke-dasharray] duration-500" : "origin-center animate-[ring-spin_1.3s_linear_infinite]"}
          style={known ? undefined : { transformOrigin: "60px 60px" }}
        />
      </svg>
      {children}
    </div>
  );
}

// The three-step frame of a module: intro (big icon and a Scan button), scanning (the same icon,
// what is being read, a Stop button) and results (the module's own content under a slim title bar).
export function ModuleStage({
  phase,
  title,
  hero,
  intro,
  scanning,
  detail,
  onScan,
  onStop,
  onBack,
  backLabel = "Começar de novo",
  scanLabel = "Analisar",
  introExtra,
  scanDisabled,
  progress,
  children,
}: {
  phase: StagePhase;
  title: string;
  hero: string;
  intro: string;
  /** Headline while scanning, e.g. "Procurando lixo…". */
  scanning: string;
  /** Line under the scanning headline: the path being read, or a hint. */
  detail?: ReactNode;
  onScan: () => void;
  onStop: () => void;
  onBack: () => void;
  backLabel?: string;
  scanLabel?: string;
  introExtra?: ReactNode;
  /** Dims the Analisar button, e.g. while a required choice is still missing. */
  scanDisabled?: boolean;
  /** 0..1 when the scan reports how far it is; omit for an indeterminate ring. */
  progress?: number;
  children: ReactNode;
}) {
  const setCompact = useStage((s) => s.setCompact);
  useEffect(() => {
    setCompact(phase !== "intro");
    return () => setCompact(false);
  }, [phase, setCompact]);

  return (
    <div className="relative flex h-full flex-col">
      <header className="drag relative flex h-[52px] shrink-0 items-center justify-center">
        {phase === "results" && (
          <button
            onClick={onBack}
            className="absolute left-4 flex h-8 items-center gap-0.5 rounded-lg pr-2.5 pl-1.5 text-[13px] font-medium text-ink hover:bg-white/10 [-webkit-app-region:no-drag]"
          >
            <ChevronLeft className="size-4" />
            {backLabel}
          </button>
        )}
        {phase !== "intro" && <span className="text-[13px] font-medium text-ink-2">{title}</span>}
      </header>

      {phase === "results" ? (
        <div key="results" className="stage-in min-h-0 flex-1">{children}</div>
      ) : (
        <div key={phase} className="stage-in flex min-h-0 flex-1 flex-col items-center px-8 pb-10 text-center">
          <div className="flex min-h-0 flex-1 items-center justify-center">
            <img
              src={hero}
              alt=""
              draggable={false}
              className={`size-[min(340px,40vh)] object-contain drop-shadow-[0_24px_48px_rgb(0_0_0/0.45)] ${phase === "scanning" ? "animate-pulse" : ""}`}
            />
          </div>
          <h1 className="text-[44px] leading-none font-light tracking-tight text-ink">{phase === "intro" ? title : scanning}</h1>
          <p className="mt-4 min-h-[44px] max-w-[520px] text-[15px] leading-snug text-ink-2">
            {phase === "intro" ? intro : <span className="block truncate">{detail}</span>}
          </p>
          <div className="mt-3 flex min-h-8 flex-wrap items-center justify-center gap-2">{phase === "intro" && introExtra}</div>
          <div className="mt-5">
            {phase === "intro" ? (
              <RoundAction onClick={onScan} disabled={scanDisabled}>
                {scanLabel}
              </RoundAction>
            ) : (
              <ScanRing progress={progress}>
                <RoundAction onClick={onStop}>Parar</RoundAction>
              </ScanRing>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
