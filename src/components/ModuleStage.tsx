import { useEffect, type ReactNode } from "react";
import { ChevronLeft } from "lucide-react";
import { useStage } from "../stores/stage";
import { RoundAction } from "./GlassTile";

export type StagePhase = "intro" | "scanning" | "results";

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
        <div className="min-h-0 flex-1">{children}</div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col items-center px-8 pb-10 text-center">
          <div className="flex min-h-0 flex-1 items-center justify-center">
            <img
              src={hero}
              alt=""
              draggable={false}
              className={`size-[min(340px,40vh)] object-contain drop-shadow-[0_24px_48px_rgb(0_0_0/0.45)] ${phase === "scanning" ? "animate-pulse" : ""}`}
            />
          </div>
          <h1 className="text-[44px] leading-none font-light tracking-tight text-ink">{phase === "intro" ? title : scanning}</h1>
          <p className="mt-4 h-[44px] max-w-[520px] text-[15px] leading-snug text-ink-2">
            {phase === "intro" ? intro : <span className="block truncate">{detail}</span>}
          </p>
          <div className="mt-3 flex h-8 items-center">{phase === "intro" && introExtra}</div>
          <div className="mt-5">
            {phase === "intro" ? <RoundAction onClick={onScan}>{scanLabel}</RoundAction> : <RoundAction onClick={onStop}>Parar</RoundAction>}
          </div>
        </div>
      )}
    </div>
  );
}
