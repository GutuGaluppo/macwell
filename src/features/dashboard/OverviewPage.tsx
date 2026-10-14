import { useQuery } from "@tanstack/react-query";
import { HardDrive, Loader2, Usb } from "lucide-react";
import { api } from "../../lib/ipc";
import { Card } from "../../components/ui";
import { GlassTile, RoundAction, TileButton } from "../../components/GlassTile";
import { formatBytes, formatDateTime, shortPath } from "../../lib/format";
import { useNav } from "../../stores/nav";
import { useJob, useJobs } from "../../stores/jobs";
import { useHome } from "../scanner/ScannerPage";
import { modules, smartCare, tools, type Module } from "../../app/modules";
import type { ScanRecord, Volume } from "../../types";

function VolumeCard({ v }: { v: Volume }) {
  const used = v.total - v.free;
  const pct = v.total ? used / v.total : 0;
  const tone = pct > 0.9 ? "bg-danger" : pct > 0.75 ? "bg-review" : "bg-accent";
  return (
    <Card className="px-5 py-4">
      <div className="flex items-center gap-3">
        <div className="grid size-9 place-items-center rounded-xl bg-surface-3 text-ink-2">
          {v.isRoot ? <HardDrive className="size-[18px]" /> : <Usb className="size-[18px]" />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate font-semibold">{v.name}</div>
          <div className="text-[11.5px] text-ink-3">
            {v.fileSystem.toUpperCase()}
            {v.readOnly ? " · somente leitura" : ""}
            {!v.local ? " · rede" : ""}
          </div>
        </div>
        <div className="text-right">
          <div className="tabular text-[17px] font-semibold">{formatBytes(v.free)}</div>
          <div className="text-[11.5px] text-ink-3">livres de {formatBytes(v.total)}</div>
        </div>
      </div>
      <div className="mt-3 h-2 overflow-hidden rounded-full bg-surface-3">
        <div className={`h-full rounded-full ${tone}`} style={{ width: `${Math.max(1, pct * 100)}%` }} />
      </div>
    </Card>
  );
}

function ModuleTile({ module, lastScan }: { module: Module; lastScan?: ScanRecord }) {
  const go = useNav((s) => s.go);
  const running = useJobs((s) => module.sections.some((x) => x.job && s.jobs[x.job]?.status === "running"));
  const status = running ? "Analisando…" : lastScan ? `Analisado em ${formatDateTime(lastScan.startedAt)}` : "Ainda não analisado";
  return (
    <GlassTile
      tint={module.id}
      icon={module.icon}
      label={
        <>
          {module.label}
          {module.badge && <small className="rounded bg-white/18 px-1.5 py-px text-[9px] font-medium">{module.badge}</small>}
        </>
      }
      value={module.tagline}
      caption={
        <>
          {running && <Loader2 className="size-3.5 animate-spin" />}
          {status}
        </>
      }
      action={<TileButton onClick={() => go(module.sections[0].id)}>Abrir</TileButton>}
    />
  );
}

function SmartCareButton() {
  const go = useNav((s) => s.go);
  const { status } = useJob(smartCare.sections[0].job!);
  const label = status === "running" ? "Ver progresso" : status === "done" ? "Ver resultado" : "Analisar";
  return (
    <div data-tint={smartCare.id} className="flex flex-col items-center gap-2">
      <RoundAction onClick={() => go("smartCare")}>{status === "running" ? <Loader2 className="size-6 animate-spin" /> : label}</RoundAction>
      <span className="text-[12px] text-ink-2">{status === "running" ? "Cuidado inteligente em andamento" : "Cuidado inteligente"}</span>
    </div>
  );
}

const allModules = [smartCare, ...modules, ...tools];
const moduleLabel = (id: string) => allModules.find((m) => m.sections.some((s) => s.id === id))?.label ?? id;

export function OverviewPage() {
  const home = useHome();
  const go = useNav((s) => s.go);
  const { data: volumes } = useQuery({ queryKey: ["volumes"], queryFn: api.diskOverview, refetchInterval: 30_000 });
  const { data: scans } = useQuery({ queryKey: ["recentScans"], queryFn: () => api.recentScans(undefined, 20) });
  const { data: metrics } = useQuery({ queryKey: ["metrics"], queryFn: api.localMetrics });

  // Scans come newest first, so the first match is the latest one for the module.
  const lastScan = (m: Module) => scans?.find((s) => m.sections.some((x) => x.id === s.module));

  return (
    <div className="h-full overflow-y-auto">
      <header className="drag px-8 pt-12 pb-7 text-center">
        <h1 className="text-[30px] font-semibold tracking-tight">Vamos deixar seu Mac em ordem.</h1>
        <p className="mt-1.5 text-[13.5px] text-ink-2">
          {metrics?.bytes_reclaimed_total
            ? `Você já liberou ${formatBytes(metrics.bytes_reclaimed_total)} com o OrganizeMyMac.`
            : "Escolha um módulo ou faça uma análise completa. Nada é removido sem a sua confirmação."}
        </p>
      </header>

      <div className="space-y-4 px-8 pb-8">
        <div className="grid grid-cols-2 gap-4">
          {volumes?.map((v) => (
            <VolumeCard key={v.mountPoint} v={v} />
          ))}
        </div>

        <div className="grid grid-cols-3 gap-4">
          {modules.map((m) => (
            <ModuleTile key={m.id} module={m} lastScan={lastScan(m)} />
          ))}
        </div>

        <div className="flex items-center justify-between gap-4 pt-2">
          <div className="flex gap-2">
            {tools.map((t) => (
              <button
                key={t.id}
                onClick={() => go(t.sections[0].id)}
                className="glass flex h-9 items-center gap-2 rounded-full pr-4 pl-1.5 text-[12.5px] font-medium hover:bg-white/20"
              >
                <img src={t.icon} alt="" className="size-[26px] object-contain" draggable={false} />
                {t.label}
              </button>
            ))}
          </div>
          <SmartCareButton />
          <div className="w-[260px]" aria-hidden />
        </div>

        <div className="pt-2">
          <h2 className="mb-2 text-[13px] font-semibold">Últimas análises</h2>
          <Card className="divide-y divide-line">
            {scans?.length ? (
              scans.slice(0, 6).map((s) => (
                <div key={s.id} className="grid grid-cols-[130px_150px_minmax(0,1fr)_90px] gap-3 px-4 py-2 text-[12.5px]">
                  <span className="text-ink-3">{formatDateTime(s.startedAt)}</span>
                  <span className="truncate">{moduleLabel(s.module)}</span>
                  <span className="truncate text-ink-2">{shortPath(s.root, home)}</span>
                  <span className="tabular text-right">{formatBytes(s.bytesScanned)}</span>
                </div>
              ))
            ) : (
              <div className="px-4 py-4 text-center text-[12.5px] text-ink-3">Nenhuma análise concluída ainda.</div>
            )}
          </Card>
        </div>
      </div>
    </div>
  );
}
