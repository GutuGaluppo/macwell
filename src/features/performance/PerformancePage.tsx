import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { BatteryCharging, BatteryMedium, Clock, Cpu, HardDrive, X } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { Button, Card, ErrorNote } from "../../components/ui";
import { useNav } from "../../stores/nav";
import { ModuleStage } from "../../components/ModuleStage";
import { BentoButton, BentoIcon, ResultsBento } from "../../components/ResultsBento";
import performanceHero from "../../assets/icons/desempenho-hd.webp";
import { Modal } from "../../components/ReviewDialog";
import { formatBytes } from "../../lib/format";
import { AppIcon } from "../applications/shared";
import type { AppUsage, Health, ProcessCategory, ProcessList } from "../../types";

const HISTORY = 60;

function duration(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return d ? `${d} d ${h} h` : h ? `${h} h ${m} min` : `${m} min`;
}

function Sparkline({ values, max = 100 }: { values: number[]; max?: number }) {
  if (values.length < 2) return <div className="h-[110px]" />;
  const w = 220;
  const h = 40;
  const last = values.length - 1;
  const pts = values.map((v, i) => `${(i / last) * w},${h - (Math.min(v, max) / max) * (h - 2) - 1}`).join(" ");
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="h-[110px] w-full" preserveAspectRatio="none" aria-hidden>
      <polyline points={`0,${h} ${pts} ${w},${h}`} fill="var(--accent-soft)" stroke="none" />
      <polyline points={pts} fill="none" stroke="var(--accent)" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

function Meter({ value, tone }: { value: number; tone?: string }) {
  const color = tone ?? (value > 0.9 ? "bg-danger" : value > 0.75 ? "bg-review" : "bg-accent");
  return (
    <div className="h-2 overflow-hidden rounded-full bg-surface-3">
      <div className={`h-full rounded-full ${color} transition-[width] duration-500`} style={{ width: `${Math.max(1, Math.min(1, value) * 100)}%` }} />
    </div>
  );
}

const categories: { id: ProcessCategory; label: string; note: string }[] = [
  { id: "application", label: "Aplicativos", note: "Apps que você abriu, com os processos auxiliares de cada um somados." },
  { id: "background", label: "Em segundo plano", note: "Outros processos da sua conta: agentes, ajudantes e ferramentas de linha de comando." },
  { id: "system", label: "Sistema", note: "Processos do macOS e de outras contas. Não podem ser encerrados por aqui." },
];

// The process table, in a dialog: apps with their helpers added up, background and system processes.
function ProcessesModal({ procs, onClose }: { procs: ProcessList; onClose: () => void }) {
  const qc = useQueryClient();
  const [tab, setTab] = useState<ProcessCategory>("application");
  const [quitting, setQuitting] = useState<AppUsage | null>(null);
  const [quitError, setQuitError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const rows: (AppUsage & { key: string; user?: string | null })[] = useMemo(() => {
    if (tab === "application")
      return procs.apps
        .filter((a) => a.appPath && procs.processes.some((p) => p.appPath === a.appPath && p.category === "application"))
        .map((a) => ({ ...a, key: a.appPath ?? a.name }));
    return procs.processes
      .filter((p) => p.category === tab)
      .sort((a, b) => b.memory - a.memory)
      .slice(0, 150)
      .map((p) => ({ key: String(p.pid), name: p.name, appPath: null, memory: p.memory, cpu: p.cpu, processes: 1, canQuit: false, user: p.user }));
  }, [procs, tab]);

  const quit = async () => {
    if (!quitting?.appPath) return;
    setBusy(true);
    setQuitError(null);
    try {
      await api.quitApplication(quitting.appPath);
      setQuitting(null);
      setTimeout(() => qc.invalidateQueries({ queryKey: ["processes"] }), 1500);
    } catch (e) {
      setQuitError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Modal onClose={onClose} width={760}>
        <div className="flex items-center justify-between gap-4 px-6 pt-5 pb-3">
          <div className="flex items-center gap-3">
            <h2 className="text-[16px] font-semibold">Processos por memória</h2>
            <div className="inline-flex rounded-lg glass p-0.5">
              {categories.map((c) => (
                <button
                  key={c.id}
                  onClick={() => setTab(c.id)}
                  className={`h-7 rounded-md px-2.5 text-[12px] ${tab === c.id ? "bg-accent font-medium text-accent-ink" : "text-ink-2 hover:text-ink"}`}
                >
                  {c.label}
                </button>
              ))}
            </div>
          </div>
          <button className="rounded p-1 text-ink-3 hover:bg-surface-2 hover:text-ink" onClick={onClose} aria-label="Fechar">
            <X className="size-4" />
          </button>
        </div>
        <p className="px-6 pb-2 text-[12px] text-ink-3">{categories.find((c) => c.id === tab)?.note}</p>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-5">
          <Card className="divide-y divide-line">
            {rows.map((r) => (
              <div key={r.key} className="group grid grid-cols-[28px_minmax(0,1fr)_70px_90px_80px] items-center gap-3 px-4 py-1.5 text-[12.5px]">
                {r.appPath ? <AppIcon path={r.appPath} size={22} /> : <span className="size-[22px]" />}
                <div className="min-w-0 truncate">
                  {r.name}
                  {r.processes > 1 && <span className="text-ink-3"> · {r.processes} processos</span>}
                  {r.user && tab === "system" && <span className="text-ink-3"> · {r.user}</span>}
                </div>
                <span className="tabular text-right text-ink-2">{r.cpu.toFixed(1).replace(".", ",")}%</span>
                <span className="tabular text-right">{formatBytes(r.memory)}</span>
                <div className="text-right">
                  {r.canQuit && (
                    <Button size="sm" variant="ghost" className="opacity-0 group-hover:opacity-100" onClick={() => setQuitting(r)}>
                      Encerrar…
                    </Button>
                  )}
                </div>
              </div>
            ))}
          </Card>
        </div>
      </Modal>
      {quitting && (
        <Modal onClose={busy ? undefined : () => setQuitting(null)} width={440}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">Encerrar {quitting.name}?</h2>
            <p className="mt-1.5 text-ink-2">É o mesmo que escolher Encerrar no menu do app: ele pode pedir para você salvar documentos abertos.</p>
            {quitError && (
              <div className="mt-3">
                <ErrorNote>{quitError}</ErrorNote>
              </div>
            )}
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button onClick={() => setQuitting(null)} disabled={busy}>
              Cancelar
            </Button>
            <Button variant="primary" busy={busy} onClick={quit}>
              Encerrar
            </Button>
          </div>
        </Modal>
      )}
    </>
  );
}

// A donut gauge for the tall tile.
function Ring({ value, label }: { value: number; label: string }) {
  const r = 70;
  const c = 2 * Math.PI * r;
  const v = Math.max(0.02, Math.min(1, value));
  return (
    <div className="relative grid size-[190px] place-items-center">
      <svg viewBox="0 0 180 180" className="absolute inset-0 -rotate-90">
        <circle cx="90" cy="90" r={r} fill="none" stroke="rgb(255 255 255 / 0.14)" strokeWidth="14" />
        <circle cx="90" cy="90" r={r} fill="none" stroke="var(--tint)" strokeWidth="14" strokeLinecap="round" strokeDasharray={`${c * v} ${c}`} className="transition-[stroke-dasharray] duration-700" />
      </svg>
      <div className="text-center">
        <div className="tabular text-[34px] leading-none font-semibold">{Math.round(value * 100)}%</div>
        <div className="mt-1 text-[12px] text-ink-2">{label}</div>
      </div>
    </div>
  );
}

function verdict(h: Health): string {
  if (h.memoryUsed / h.memoryTotal > 0.92 || h.swapUsed > 4e9) return "Seu Mac está com pouca memória.";
  if (h.cpu > 85) return "Seu Mac está muito ocupado agora.";
  if (h.diskFree / Math.max(h.diskTotal, 1) < 0.08) return "Seu disco está quase cheio.";
  return "Seu Mac está funcionando bem.";
}

function PerformanceResults({ h, procs, cpuHistory }: { h: Health; procs?: ProcessList; cpuHistory: number[] }) {
  const go = useNav((s) => s.go);
  const [showProcs, setShowProcs] = useState(false);
  const mem = h.memoryUsed / h.memoryTotal;
  const disk = (h.diskTotal - h.diskFree) / Math.max(h.diskTotal, 1);
  const bat = h.battery;
  const review = (
    <BentoButton primary disabled={!procs} onClick={() => setShowProcs(true)}>
      Revisar
    </BentoButton>
  );
  return (
    <>
      <ResultsBento
        headline={verdict(h)}
        pill={<BentoButton onClick={() => setShowProcs(true)}>Ver processos</BentoButton>}
        extra={<span className="text-[12px] text-ink-3">Atualiza a cada 2 segundos</span>}
        tiles={[
          {
            id: "memory",
            title: `${formatBytes(h.memoryUsed)} de memória em uso`,
            description: `de ${formatBytes(h.memoryTotal)} · ${formatBytes(h.memoryAvailable)} disponíveis${h.swapTotal > 0 ? ` · swap ${formatBytes(h.swapUsed)}` : ""}${h.swapUsed > 2e9 ? ". O Mac está usando o disco como memória." : ""}`,
            visual: <Ring value={mem} label="em uso" />,
            actions: review,
          },
          {
            id: "cpu",
            title: `${Math.round(h.cpu)}% de CPU`,
            description: `${h.cores} núcleos · carga ${h.loadAverage[0].toFixed(2).replace(".", ",")} · ligado há ${duration(h.uptime)}`,
            visual: <BentoIcon><Cpu /></BentoIcon>,
            body: <Sparkline values={cpuHistory} />,
            actions: review,
          },
          {
            id: "disk",
            title: `${formatBytes(h.diskFree)} livres no disco`,
            description: `de ${formatBytes(h.diskTotal)}`,
            visual: <BentoIcon size={48}><HardDrive /></BentoIcon>,
            body: <Meter value={disk} />,
            actions: <BentoButton primary onClick={() => go("spaceMap")}>Mapa de espaço</BentoButton>,
          },
          {
            id: "battery",
            title: bat ? `${bat.percent}% de bateria` : "Sem bateria",
            description: bat
              ? [bat.charging ? "carregando" : bat.onAc ? "na tomada" : "na bateria", bat.minutesRemaining ? duration(bat.minutesRemaining * 60) + (bat.charging ? " para carregar" : " restantes") : null].filter(Boolean).join(" · ")
              : "Este Mac não tem bateria.",
            visual: <BentoIcon size={48}>{bat?.charging ? <BatteryCharging /> : bat ? <BatteryMedium /> : <Clock />}</BentoIcon>,
            body: bat ? <Meter value={bat.percent / 100} tone={bat.percent < 15 ? "bg-danger" : "bg-safe"} /> : undefined,
            actions: <></>,
          },
        ]}
      />
      {showProcs && procs && <ProcessesModal procs={procs} onClose={() => setShowProcs(false)} />}
    </>
  );
}

// Intro, first reading and live results. The health and process queries only run once the user
// presses Analisar, and stop when they go back.
export function PerformancePage() {
  const [started, setStarted] = useState(false);
  const { data: h, error } = useQuery({ queryKey: ["health"], queryFn: api.health, refetchInterval: 2000, enabled: started });
  const { data: procs } = useQuery({ queryKey: ["processes"], queryFn: api.processList, refetchInterval: 5000, enabled: started });
  const [cpuHistory, setCpuHistory] = useState<number[]>([]);

  useEffect(() => {
    if (h) setCpuHistory((prev) => [...prev, h.cpu].slice(-HISTORY));
  }, [h]);

  const phase = !started ? "intro" : h || error ? "results" : "scanning";
  const reset = () => {
    setStarted(false);
    setCpuHistory([]);
  };

  return (
    <ModuleStage
      phase={phase}
      title="Desempenho"
      hero={performanceHero}
      intro="Veja em tempo real como o Mac está usando CPU, memória, disco e bateria, e quais apps pesam mais."
      scanning="Medindo o desempenho…"
      detail="Lendo CPU, memória e processos"
      onScan={() => setStarted(true)}
      onStop={reset}
      onBack={reset}
      backLabel="Voltar"
    >
      {error && (
        <div className="px-8">
          <ErrorNote>{errorMessage(error)}</ErrorNote>
        </div>
      )}
      {h && <PerformanceResults h={h} procs={procs} cpuHistory={cpuHistory} />}
    </ModuleStage>
  );
}
