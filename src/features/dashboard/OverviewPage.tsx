import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { AppWindow, HardDrive, Loader2, MemoryStick, ShieldCheck, Sparkles } from "lucide-react";
import { api } from "../../lib/ipc";
import { BentoButton, BentoIcon } from "../../components/ResultsBento";
import { formatAge, formatBytes, formatCount, formatDateTime, shortPath } from "../../lib/format";
import { useNav } from "../../stores/nav";
import { useJob } from "../../stores/jobs";
import { useHome } from "../scanner/ScannerPage";
import { modules, smartCare, tools } from "../../app/modules";
import type { Section } from "../../stores/nav";
import type { AppsResult, SmartCareReport } from "../../types";
import appIconHd from "../../assets/icons/app-icon-hd.webp";

const UNUSED_MS = 180 * 86_400_000;
const STALE_MS = 14 * 86_400_000;

type Advice = { id: string; level: "high" | "suggest"; icon: ReactNode; title: string; text: string; action: string; to: Section; busy?: boolean };

const allModules = [smartCare, ...modules, ...tools];
const moduleLabel = (id: string) => allModules.find((m) => m.sections.some((s) => s.id === id))?.label ?? id;

function Ring({ children }: { children: ReactNode }) {
  return (
    <div className="relative grid size-[min(300px,34vh)] place-items-center">
      <svg viewBox="0 0 200 200" className="absolute inset-0 drop-shadow-[0_0_24px_color-mix(in_srgb,var(--tint)_55%,transparent)]">
        <defs>
          <linearGradient id="ring" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stopColor="var(--tint)" />
            <stop offset="100%" stopColor="#5eead4" />
          </linearGradient>
        </defs>
        <circle cx="100" cy="100" r="92" fill="rgb(255 255 255 / 0.05)" stroke="rgb(255 255 255 / 0.12)" strokeWidth="1" />
        <circle cx="100" cy="100" r="88" fill="none" stroke="url(#ring)" strokeWidth="5" strokeLinecap="round" />
      </svg>
      {children}
    </div>
  );
}

function AdviceCard({ a }: { a: Advice }) {
  const go = useNav((s) => s.go);
  return (
    <section className="glass flex shrink-0 items-start gap-4 rounded-[22px] p-5">
      <BentoIcon size={52}>{a.busy ? <Loader2 className="animate-spin" /> : a.icon}</BentoIcon>
      <div className="min-w-0 flex-1">
        <h2 className="text-[16px] leading-snug font-semibold text-ink">{a.title}</h2>
        <p className="mt-1 text-[13px] leading-snug text-ink-2">{a.text}</p>
        <div className="mt-3 flex justify-end">
          <BentoButton primary={a.level === "high"} onClick={() => go(a.to)}>
            {a.action}
          </BentoButton>
        </div>
      </div>
    </section>
  );
}

export function OverviewPage() {
  const home = useHome();
  const { data: volumes } = useQuery({ queryKey: ["volumes"], queryFn: api.diskOverview, refetchInterval: 30_000 });
  const { data: scans } = useQuery({ queryKey: ["recentScans"], queryFn: () => api.recentScans(undefined, 20) });
  const { data: metrics } = useQuery({ queryKey: ["metrics"], queryFn: api.localMetrics });
  const { data: h } = useQuery({ queryKey: ["health"], queryFn: api.health, refetchInterval: 30_000 });
  const care = useJob<SmartCareReport>(smartCare.sections[0].job!);
  const apps = useJob<AppsResult>("apps");
  const security = useJob("security");

  const disk = volumes?.find((v) => v.isRoot) ?? volumes?.[0];
  const lastScan = scans?.[0];
  const stale = !lastScan || Date.now() - lastScan.startedAt > STALE_MS;
  const unused = (apps.result?.apps ?? []).filter((a) => !a.systemApp && (a.lastUsedAt ?? 0) < Date.now() - UNUSED_MS);
  const careBytes = (care.result?.recommendations ?? []).reduce((s, r) => s + r.bytes, 0);

  // What the Assistente suggests, most urgent first. Everything is read from what is already known:
  // it never starts an analysis by itself.
  const advice: Advice[] = [];
  if (disk && disk.total && disk.free / disk.total < 0.15)
    advice.push({ id: "disk", level: "high", icon: <HardDrive />, title: "Pouco espaço livre", text: `Só ${formatBytes(disk.free)} livres em ${disk.name}. Uma limpeza pode ajudar.`, action: "Abrir Limpeza", to: "cleanup" });
  if (h && (h.memoryUsed / h.memoryTotal > 0.9 || h.swapUsed > 2e9))
    advice.push({ id: "memory", level: "high", icon: <MemoryStick />, title: "A memória está sob pressão", text: "O Mac está perto do limite de memória. Veja quais apps pesam mais.", action: "Ver desempenho", to: "performance" });
  if (care.status === "running")
    advice.push({ id: "care", level: "suggest", icon: <Sparkles />, busy: true, title: "Cuidado inteligente em andamento", text: "A análise completa está rodando.", action: "Ver progresso", to: "smartCare" });
  else if (care.status === "done" && care.result?.recommendations.length)
    advice.push({ id: "care", level: "suggest", icon: <Sparkles />, title: `${formatCount(care.result.recommendations.length)} recomendações do Cuidado inteligente`, text: `Você pode liberar até ${formatBytes(careBytes)}. Nada é removido sem a sua confirmação.`, action: "Ver resultado", to: "smartCare" });
  else
    advice.push({
      id: "care",
      level: stale ? "high" : "suggest",
      icon: <Sparkles />,
      title: stale ? "É hora de cuidar do Mac!" : "Faça uma análise completa",
      text: lastScan ? `A última análise foi ${formatAge(lastScan.startedAt)}. Rode o Cuidado inteligente para ver o que dá para liberar.` : "Rode o Cuidado inteligente para ver o que dá para liberar.",
      action: "Analisar",
      to: "smartCare",
    });
  if (security.status === "idle" && !scans?.some((s) => s.module === "security"))
    advice.push({ id: "security", level: "suggest", icon: <ShieldCheck />, title: "Auditoria de segurança recomendada", text: "Veja o que inicia sozinho no Mac e como os apps estão assinados.", action: "Abrir Segurança", to: "security" });
  if (unused.length)
    advice.push({ id: "apps", level: "suggest", icon: <AppWindow />, title: "Desinstalar apps sem uso", text: `${formatCount(unused.length)} ${unused.length === 1 ? "app não é aberto" : "apps não são abertos"} há mais de 6 meses e ${unused.length === 1 ? "ocupa" : "ocupam"} ${formatBytes(unused.reduce((s, a) => s + a.size, 0))}.`, action: "Revisar", to: "apps" });

  const score = advice.reduce((s, a) => s + (a.level === "high" ? 2 : 1), 0);
  const [state, tone, line] =
    score <= 1
      ? ["Bom", "text-safe", "Seu Mac está em boa forma. Uma manutenção de vez em quando mantém tudo rápido."]
      : score <= 3
        ? ["Atenção", "text-review", "Há algumas coisas que valem uma olhada. Veja as sugestões ao lado."]
        : ["Precisa de cuidado", "text-danger", "Seu Mac pede atenção. Comece pelas sugestões em destaque."];

  return (
    <div className="flex h-full flex-col">
      <header className="drag flex h-[52px] shrink-0 items-center justify-center text-[13px] font-medium text-ink-2">Assistente</header>
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] gap-4 px-8 pb-8">
        <section className="glass relative flex min-h-0 flex-col overflow-hidden rounded-[26px] p-7">
          <div className="flex min-h-0 flex-1 items-center justify-center">
            <Ring>
              <div className="flex flex-col items-center gap-3">
                <img src={appIconHd} alt="" draggable={false} className="size-[42%] min-h-[96px] object-contain drop-shadow-[0_14px_28px_rgb(0_0_0/0.45)]" />
                <span className="text-[16px] font-semibold">Seu Mac</span>
              </div>
            </Ring>
          </div>
          <div className="mt-4">
            <div className="text-[24px] leading-tight font-semibold tracking-tight">Saúde do Mac:</div>
            <div className={`text-[30px] leading-tight font-light ${tone}`}>{state}</div>
            <p className="mt-2 max-w-[460px] text-[13.5px] leading-snug text-ink-2">{line}</p>
            {disk && (
              <div className="mt-4 max-w-[460px]">
                <div className="mb-1.5 flex justify-between text-[12px] text-ink-2">
                  <span>{disk.name}</span>
                  <span className="tabular">
                    {formatBytes(disk.free)} livres de {formatBytes(disk.total)}
                  </span>
                </div>
                <div className="h-2 overflow-hidden rounded-full bg-white/12">
                  <div className="h-full rounded-full bg-[linear-gradient(90deg,var(--tint),#5eead4)]" style={{ width: `${Math.max(1, ((disk.total - disk.free) / Math.max(disk.total, 1)) * 100)}%` }} />
                </div>
              </div>
            )}
            {!!metrics?.bytes_reclaimed_total && <p className="mt-3 text-[12px] text-ink-3">Você já liberou {formatBytes(metrics.bytes_reclaimed_total)} com o OrganizeMyMac.</p>}
          </div>
        </section>

        <div className="flex min-h-0 flex-col gap-4 overflow-y-auto pr-1">
          {advice.map((a) => (
            <AdviceCard key={a.id} a={a} />
          ))}
          <section className="glass shrink-0 rounded-[22px] p-5">
            <h2 className="mb-2 text-[14px] font-semibold">Últimas análises</h2>
            {scans?.length ? (
              <ul className="divide-y divide-line">
                {scans.slice(0, 5).map((s) => (
                  <li key={s.id} className="grid grid-cols-[100px_minmax(0,1fr)_70px] gap-3 py-1.5 text-[12px]">
                    <span className="text-ink-3">{formatDateTime(s.startedAt)}</span>
                    <span className="truncate">
                      {moduleLabel(s.module)} <span className="text-ink-3">· {shortPath(s.root, home)}</span>
                    </span>
                    <span className="tabular text-right">{formatBytes(s.bytesScanned)}</span>
                  </li>
                ))}
              </ul>
            ) : (
              <div className="py-3 text-center text-[12.5px] text-ink-3">Nenhuma análise concluída ainda.</div>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}
