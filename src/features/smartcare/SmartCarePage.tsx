import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, AppWindow, Boxes, Check, ChevronDown, ChevronRight, Copy, Download, FileSearch, Loader2, PackageX, Sparkles, Trash2 } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { withBackupAck } from "../../stores/backupGate";
import { useNav } from "../../stores/nav";
import { Badge, Button, Card, ErrorNote } from "../../components/ui";
import { GlassTile, RoundAction, TileButton } from "../../components/GlassTile";
import { modules, smartCare } from "../../app/modules";
import { Modal } from "../../components/ReviewDialog";
import { useHome } from "../scanner/ScannerPage";
import { ConfidenceBadge, kindLabel } from "../applications/shared";
import { formatAge, formatBytes, formatCount, shortPath } from "../../lib/format";
import { artifactKindLabel, noteLabel, unsyncedReasons } from "../dev/labels";
import type { CarePlan, OperationOutcome, SmartCareReport } from "../../types";

const MODULE = "smartCare";

const stages = [
  { id: "trash", label: "Lixeira" },
  { id: "downloads", label: "Downloads" },
  { id: "home", label: "Pasta pessoal" },
  { id: "duplicates", label: "Duplicados" },
  { id: "apps", label: "Aplicativos" },
  { id: "leftovers", label: "Restos de apps" },
];
// Only runs when the user has added project folders.
const projectsStage = { id: "projects", label: "Projetos" };
const repoName = (p: string) => p.split("/").filter(Boolean).pop() ?? p;

type Row = { key: string; label: ReactNode; detail?: ReactNode; size: number; badge?: ReactNode; locked?: boolean };

function Section({
  icon,
  title,
  note,
  rows,
  selected,
  onToggle,
  onAll,
  footer,
  defaultOpen = false,
}: {
  icon: ReactNode;
  title: string;
  note?: ReactNode;
  rows: Row[];
  selected: Set<string>;
  onToggle: (key: string) => void;
  onAll?: (value: boolean) => void;
  footer?: ReactNode;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const chosen = rows.filter((r) => selected.has(r.key));
  const bytes = chosen.reduce((s, r) => s + r.size, 0);
  if (!rows.length) return null;
  return (
    <Card className="overflow-hidden">
      <button className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-surface-2" onClick={() => setOpen(!open)}>
        <span className="grid size-8 place-items-center rounded-lg bg-accent-soft text-accent [&_svg]:size-4">{icon}</span>
        <span className="min-w-0 flex-1">
          <span className="block font-medium">{title}</span>
          <span className="block text-[12px] text-ink-3">
            {formatCount(chosen.length)} de {formatCount(rows.length)} selecionados · {formatBytes(bytes)}
          </span>
        </span>
        {open ? <ChevronDown className="size-4 text-ink-3" /> : <ChevronRight className="size-4 text-ink-3" />}
      </button>
      {open && (
        <div className="border-t border-line">
          {note && <div className="bg-surface-2 px-4 py-2 text-[12px] text-ink-2">{note}</div>}
          {onAll && (
            <div className="flex gap-2 px-4 pt-2">
              <Button size="sm" variant="ghost" onClick={() => onAll(true)}>
                Marcar todos
              </Button>
              <Button size="sm" variant="ghost" onClick={() => onAll(false)}>
                Desmarcar
              </Button>
            </div>
          )}
          <div className="max-h-80 overflow-y-auto px-2 py-1">
            {rows.map((r) => (
              <label key={r.key} className={`grid grid-cols-[18px_minmax(0,1fr)_auto_80px] items-center gap-3 rounded-md px-2 py-1.5 ${r.locked ? "opacity-60" : "cursor-pointer hover:bg-surface-2"}`}>
                <input type="checkbox" className="size-3.5 accent-[var(--accent)]" disabled={r.locked} checked={selected.has(r.key)} onChange={() => onToggle(r.key)} />
                <div className="min-w-0">
                  <div className="truncate text-[12.5px]">{r.label}</div>
                  {r.detail && <div className="truncate text-[11px] text-ink-3">{r.detail}</div>}
                </div>
                <div>{r.badge}</div>
                <span className="tabular text-right text-[12.5px]">{formatBytes(r.size)}</span>
              </label>
            ))}
          </div>
          {footer && <div className="border-t border-line px-4 py-2">{footer}</div>}
        </div>
      )}
    </Card>
  );
}

const tile = (id: string) => [smartCare, ...modules].find((m) => m.id === id)!;

type Focus = "downloads" | "clutter" | "apps" | "dev";

export function SmartCarePage() {
  const home = useHome();
  const qc = useQueryClient();
  const go = useNav((s) => s.go);
  const job = useJob<SmartCareReport | null>(MODULE);
  const start = useJobs((s) => s.start);
  const reset = useJobs((s) => s.reset);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [emptyTrash, setEmptyTrash] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [outcomes, setOutcomes] = useState<OperationOutcome[] | null>(null);
  const [focus, setFocus] = useState<Focus | null>(null);
  const r = job.result ?? undefined;
  const running = job.status === "running";
  const { data: devRoots = [] } = useQuery({ queryKey: ["devRoots"], queryFn: api.devRoots });
  const stageList = devRoots.length ? [...stages, projectsStage] : stages;

  // Defaults: old installers, the smart duplicate selection and safe leftovers. Never the Trash,
  // large files or anything ambiguous.
  useEffect(() => {
    if (!r) return;
    setEmptyTrash(false);
    setSelected(
      new Set([
        ...r.downloads.filter((d) => d.selected).map((d) => `dl:${d.path}`),
        ...r.duplicates.flatMap((g) => g.files.filter((f) => f.selected).map((f) => `dup:${f.path}`)),
        ...r.leftovers.flatMap((g) => g.items.filter((i) => i.selected).map((i) => `lo:${i.path}`)),
        // Regenerable folders: only the ones nothing speaks against.
        ...(r.dev?.artifacts ?? []).filter((a) => a.confidence === "safe").map((a) => `art:${a.path}`),
      ]),
    );
  }, [r]);

  const toggle = (key: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  const setMany = (keys: string[], value: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      for (const k of keys) {
        if (value) next.add(k);
        else next.delete(k);
      }
      return next;
    });

  // A tile's checkbox restores the default selection of its category or clears it. It never marks
  // everything: that would select every copy of a duplicate.
  const defaults = useMemo(() => {
    if (!r) return null;
    return {
      downloads: r.downloads.filter((d) => d.selected).map((d) => `dl:${d.path}`),
      duplicates: r.duplicates.flatMap((g) => g.files.filter((f) => f.selected).map((f) => `dup:${f.path}`)),
      leftovers: r.leftovers.flatMap((g) => g.items.filter((i) => i.selected).map((i) => `lo:${i.path}`)),
      artifacts: (r.dev?.artifacts ?? []).filter((a) => a.confidence === "safe").map((a) => `art:${a.path}`),
    };
  }, [r]);
  const prefix = { downloads: "dl:", duplicates: "dup:", leftovers: "lo:", artifacts: "art:" } as const;
  const categoryOn = (c: keyof typeof prefix) => [...selected].some((k) => k.startsWith(prefix[c]));
  const setCategory = (c: keyof typeof prefix, on: boolean) =>
    setSelected((prev) => {
      const next = new Set([...prev].filter((k) => !k.startsWith(prefix[c])));
      if (on && defaults) for (const k of defaults[c]) next.add(k);
      return next;
    });
  const review = (f: Focus) => {
    setFocus(f);
    requestAnimationFrame(() => document.getElementById("care-review")?.scrollIntoView({ behavior: "smooth", block: "start" }));
  };

  const rows = useMemo(() => {
    if (!r) return null;
    const downloads: Row[] = r.downloads.map((d) => ({
      key: `dl:${d.path}`,
      label: d.name,
      detail: d.ageDays != null ? `Chegou há ${formatCount(d.ageDays)} dias` : undefined,
      size: d.sizeLogical,
      badge: <ConfidenceBadge confidence={d.confidence} />,
    }));
    const duplicates: Row[] = r.duplicates.flatMap((g) =>
      g.files.map((f) => {
        const others = g.files.filter((x) => x.path !== f.path);
        const lastKept = !selected.has(`dup:${f.path}`) && others.every((x) => selected.has(`dup:${x.path}`));
        return { key: `dup:${f.path}`, label: shortPath(f.path, home), detail: `${g.files.length} cópias idênticas`, size: f.sizeLogical, badge: selected.has(`dup:${f.path}`) ? undefined : <Badge tone="safe">Manter</Badge>, locked: lastKept };
      }),
    );
    const largeOld: Row[] = r.largeOld.map((f) => ({ key: `lg:${f.path}`, label: shortPath(f.path, home), detail: `Modificado ${formatAge(f.modifiedAt)}`, size: f.sizeLogical, badge: <ConfidenceBadge confidence="review" /> }));
    const leftovers: Row[] = r.leftovers.flatMap((g) =>
      g.items.map((i) => ({ key: `lo:${i.path}`, label: g.bundleId, detail: kindLabel[i.kind], size: i.size, badge: <ConfidenceBadge confidence={i.confidence} /> })),
    );
    const artifacts: Row[] = (r.dev?.artifacts ?? []).map((a) => ({
      key: `art:${a.path}`,
      label: `${artifactKindLabel[a.kind] ?? a.kind} · ${repoName(a.repo)}`,
      detail: a.notes.length ? a.notes.map((n) => noteLabel[n]).join(" ") : (a.path.startsWith(`${a.repo}/`) ? a.path.slice(a.repo.length + 1) : shortPath(a.path, home)),
      size: a.size,
      badge: <ConfidenceBadge confidence={a.confidence} />,
      // The server refuses folders with versioned files, so they cannot be chosen here either.
      locked: a.notes.includes("tracked"),
    }));
    return { downloads, duplicates, largeOld, leftovers, artifacts };
  }, [r, selected, home]);

  const all = rows ? [...rows.downloads, ...rows.duplicates, ...rows.largeOld, ...rows.leftovers, ...rows.artifacts] : [];
  const chosenBytes = all.filter((x) => selected.has(x.key)).reduce((s, x) => s + x.size, 0) + (emptyTrash && r ? r.trash.bytes : 0);
  const chosenCount = all.filter((x) => selected.has(x.key)).length;
  const reclaimable = r
    ? r.downloads.filter((d) => d.selected).reduce((s, d) => s + d.sizeLogical, 0) +
      r.duplicates.reduce((s, g) => s + g.wasted, 0) +
      r.leftovers.reduce((s, g) => s + g.items.filter((i) => i.selected).reduce((a, i) => a + i.size, 0), 0) +
      (r.dev?.artifacts ?? []).filter((a) => a.confidence === "safe").reduce((s, a) => s + a.size, 0) +
      (r.trash.readable ? r.trash.bytes : 0)
    : 0;

  const plan = (): CarePlan => {
    const has = (k: string) => selected.has(k);
    return {
      emptyTrash: emptyTrash && r ? r.trash.items.map((i) => i.path) : [],
      files: [
        ...(r?.downloads ?? []).filter((d) => has(`dl:${d.path}`)).map((d) => ({ path: d.path, size: d.sizeLogical })),
        ...(r?.largeOld ?? []).filter((f) => has(`lg:${f.path}`)).map((f) => ({ path: f.path, size: f.sizeLogical })),
      ],
      duplicates: (r?.duplicates ?? [])
        .map((g) => ({
          all: g.files.map((f) => f.path),
          remove: g.files.filter((f) => has(`dup:${f.path}`)).map((f) => ({ path: f.path, size: f.sizeLogical, modifiedAt: f.modifiedAt ?? null })),
        }))
        .filter((g) => g.remove.length),
      leftovers: (r?.leftovers ?? []).flatMap((g) => g.items.filter((i) => has(`lo:${i.path}`)).map((i) => ({ path: i.path, size: i.size }))),
      artifacts: (r?.dev?.artifacts ?? []).filter((a) => has(`art:${a.path}`)).map((a) => ({ path: a.path, repo: a.repo, kind: a.kind, size: a.size })),
    };
  };

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const out = await withBackupAck((ack) => api.runSmartCare(plan(), ack));
      setOutcomes(out);
      setConfirm(false);
      for (const k of ["operations", "metrics", "volumes", "trash"]) qc.invalidateQueries({ queryKey: [k] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const stageIndex = stageList.findIndex((s) => s.id === job.stage?.stage);

  const downloadsBytes = r ? r.downloads.filter((d) => d.selected).reduce((s, d) => s + d.sizeLogical, 0) : 0;
  const duplicatesBytes = r ? r.duplicates.reduce((s, g) => s + g.wasted, 0) : 0;
  const leftoversBytes = r ? r.leftovers.reduce((s, g) => s + g.items.reduce((a, i) => a + i.size, 0), 0) : 0;
  const artifactsBytes = r ? (r.dev?.artifacts ?? []).filter((a) => a.confidence === "safe").reduce((s, a) => s + a.size, 0) : 0;
  const unsyncedRepos = (r?.dev?.repos ?? []).filter((x) => unsyncedReasons(x).length > 0);
  const nothing = (text: string) => <span className="text-ink-2">{text}</span>;

  return (
    <div className="h-full overflow-y-auto">
      <header className="drag relative px-8 pt-12 pb-7 text-center">
        <h1 className="text-[30px] font-semibold tracking-tight">
          {running ? "Analisando o seu Mac…" : r ? "Análise pronta. Veja o que encontramos:" : "Cuidado inteligente"}
        </h1>
        <p className="mx-auto mt-1.5 max-w-xl text-[13.5px] text-ink-2">
          {r && !running
            ? `Até ${formatBytes(reclaimable)} para liberar · análise em ${(r.durationMs / 1000).toFixed(0)} s. Você revisa tudo antes de qualquer remoção.`
            : "Lixeira, Downloads, duplicados, arquivos esquecidos e restos de apps numa análise só. Nada é alterado na análise."}
        </p>
        {r && !running && (
          <div className="absolute top-12 right-8 [-webkit-app-region:no-drag]">
            <Button variant="ghost" onClick={() => reset(MODULE)}>
              Nova análise
            </Button>
          </div>
        )}
      </header>

      <div className="space-y-4 px-8 pb-4">
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
        {error && <ErrorNote>{error}</ErrorNote>}

        {(job.status === "idle" || job.status === "cancelled" || (job.status === "done" && !r)) && (
          <div className="flex flex-col items-center pt-6 pb-10">
            <img src={smartCare.icon} alt="" draggable={false} className="size-[180px] object-contain drop-shadow-[0_20px_40px_rgb(0_0_0/0.4)]" />
            <div className="mt-8">
              <RoundAction onClick={() => start(MODULE, "start_smart_care", {})}>Analisar</RoundAction>
            </div>
            <p className="mt-3 text-[12px] text-ink-2">Leva de alguns segundos a poucos minutos.</p>
          </div>
        )}

        {running && (
          <div className="flex flex-col items-center pt-2 pb-8">
            <img src={smartCare.icon} alt="" draggable={false} className="size-[150px] animate-pulse object-contain drop-shadow-[0_20px_40px_rgb(0_0_0/0.4)]" />
            <Card className="mt-8 w-full max-w-[640px] p-5">
              <div className="grid grid-cols-3 gap-2">
                {stageList.map((s, i) => (
                  <div key={s.id} className={`flex items-center gap-2 rounded-lg px-3 py-2 text-[12.5px] ${i === stageIndex ? "bg-accent-soft text-ink" : i < stageIndex ? "text-ink-2" : "text-ink-3"}`}>
                    {i < stageIndex ? <Check className="size-4 text-safe" /> : i === stageIndex ? <Loader2 className="size-4 animate-spin" /> : <span className="size-4" />}
                    {s.label}
                  </div>
                ))}
              </div>
              {job.progress && stageIndex === 2 && <div className="mt-3 text-center text-[11.5px] text-ink-3">{formatCount(job.progress.files)} arquivos lidos</div>}
            </Card>
            <Button className="mt-4" onClick={() => useJobs.getState().cancel(MODULE)}>
              Cancelar
            </Button>
          </div>
        )}

        {r && rows && !running && (
          <>
            <div className="grid grid-cols-6 gap-4">
              <GlassTile
                className="col-span-2"
                tint="cleanup"
                icon={tile("cleanup").icon}
                label="Downloads"
                checked={categoryOn("downloads")}
                onCheck={rows.downloads.length ? (v) => setCategory("downloads", v) : undefined}
                value={downloadsBytes ? formatBytes(downloadsBytes) : nothing("Nada a limpar")}
                caption={downloadsBytes ? "em instaladores antigos" : "em Downloads"}
                action={rows.downloads.length > 0 && <TileButton onClick={() => review("downloads")}>Revisar</TileButton>}
              />
              <GlassTile
                className="col-span-2"
                tint="clutter"
                icon={tile("clutter").icon}
                label="Minha bagunça"
                checked={categoryOn("duplicates")}
                onCheck={rows.duplicates.length ? (v) => setCategory("duplicates", v) : undefined}
                value={duplicatesBytes ? formatBytes(duplicatesBytes) : nothing("Sem duplicados")}
                caption={
                  duplicatesBytes
                    ? r.largeOld.length
                      ? `em cópias · ${formatCount(r.largeOld.length)} ${r.largeOld.length === 1 ? "esquecido" : "esquecidos"}`
                      : "em cópias duplicadas"
                    : r.largeOld.length
                      ? `${formatCount(r.largeOld.length)} arquivos grandes esquecidos`
                      : "acima de 10 MB"
                }
                action={(rows.duplicates.length > 0 || rows.largeOld.length > 0) && <TileButton onClick={() => review("clutter")}>Revisar</TileButton>}
              />
              <GlassTile
                className="col-span-2"
                tint="apps"
                icon={tile("apps").icon}
                label="Aplicativos"
                checked={categoryOn("leftovers")}
                onCheck={rows.leftovers.length ? (v) => setCategory("leftovers", v) : undefined}
                value={leftoversBytes ? formatBytes(leftoversBytes) : nothing("Sem restos")}
                caption={`em restos de apps · ${formatCount(r.unusedApps.length)} sem uso`}
                action={(rows.leftovers.length > 0 || r.unusedApps.length > 0) && <TileButton onClick={() => review("apps")}>Revisar</TileButton>}
              />

              <GlassTile
                className="col-span-4"
                tint="smartCare"
                icon={smartCare.icon}
                label="Recomendações"
                value={r.recommendations.length ? undefined : nothing("Tudo em ordem por aqui")}
              >
                {r.recommendations.length > 0 && (
                  <ul className="relative mt-3 max-w-[78%] space-y-2">
                    {r.recommendations.slice(0, 4).map((rec) => (
                      <li key={rec.id} className="flex items-start gap-2">
                        {rec.level === "high" ? <AlertTriangle className="mt-0.5 size-4 shrink-0 text-review" /> : <Sparkles className="mt-0.5 size-4 shrink-0 text-ink-2" />}
                        <span className="min-w-0">
                          <span className="block text-[13.5px] font-semibold">{rec.title}</span>
                          <span className="block truncate text-[12px] text-ink-2">{rec.detail}</span>
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </GlassTile>
              <GlassTile
                className="col-span-2"
                tint="cleanup"
                icon={<Trash2 />}
                label="Lixeira"
                checked={emptyTrash}
                onCheck={r.trash.readable && r.trash.items.length > 0 ? setEmptyTrash : undefined}
                value={!r.trash.readable ? nothing("Sem acesso") : r.trash.bytes ? formatBytes(r.trash.bytes) : nothing("Vazia")}
                caption={r.trash.readable && r.trash.bytes ? (emptyTrash ? <span className="text-review">será apagada de vez</span> : "marque para esvaziar") : undefined}
              />

              {r.dev && (
                <GlassTile
                  className="col-span-6"
                  tint="dev"
                  icon={tile("dev").icon}
                  label="Projetos"
                  checked={categoryOn("artifacts")}
                  onCheck={rows.artifacts.length ? (v) => setCategory("artifacts", v) : undefined}
                  value={artifactsBytes ? formatBytes(artifactsBytes) : nothing("Nada a recriar")}
                  caption={artifactsBytes ? "em node_modules, target e similares: voltam com um comando" : "nas pastas de projetos"}
                  action={rows.artifacts.length > 0 && <TileButton onClick={() => review("dev")}>Revisar</TileButton>}
                >
                  {unsyncedRepos.length > 0 && (
                    <div className="relative mt-3 max-w-[70%] text-[12.5px] text-ink-2">
                      <span className="font-semibold text-ink">
                        {formatCount(unsyncedRepos.length)} {unsyncedRepos.length === 1 ? "repositório com trabalho" : "repositórios com trabalho"} não enviado:
                      </span>{" "}
                      {unsyncedRepos.slice(0, 4).map((x) => repoName(x.path)).join(", ")}
                      {unsyncedRepos.length > 4 ? ` e mais ${unsyncedRepos.length - 4}` : ""}.{" "}
                      <button className="underline underline-offset-2 hover:text-ink" onClick={() => go("projects")}>
                        Abrir Projetos
                      </button>
                    </div>
                  )}
                </GlassTile>
              )}
            </div>

            {focus && (
              <div id="care-review" className="scroll-mt-4 space-y-3 pt-4">
                <h2 className="text-[15px] font-semibold">
                  {focus === "downloads" ? "Downloads" : focus === "clutter" ? "Minha bagunça" : focus === "dev" ? "Projetos" : "Aplicativos"}
                </h2>
                {focus === "downloads" && (
                  <Section defaultOpen icon={<Download />} title="Downloads" rows={rows.downloads} selected={selected} onToggle={toggle} onAll={(v) => setMany(rows.downloads.map((x) => x.key), v)} note="Instaladores antigos já vêm marcados; o resto fica para você decidir." />
                )}
                {focus === "clutter" && (
                  <>
                    <Section defaultOpen icon={<Copy />} title="Duplicados (acima de 10 MB)" rows={rows.duplicates} selected={selected} onToggle={toggle} note="Uma cópia de cada grupo sempre fica. Duplicados menores estão na seção Duplicados." />
                    <Section icon={<FileSearch />} title="Arquivos grandes e esquecidos" rows={rows.largeOld} selected={selected} onToggle={toggle} note="Mais de 500 MB e sem modificação há mais de um ano. Nenhum vem marcado." />
                  </>
                )}
                {focus === "dev" && (
                  <Section
                    defaultOpen
                    icon={<Boxes />}
                    title="Pastas que dá para recriar"
                    rows={rows.artifacts}
                    selected={selected}
                    onToggle={toggle}
                    onAll={(v) => setMany(rows.artifacts.filter((x) => !x.locked).map((x) => x.key), v)}
                    note="Só vêm marcadas as que nada contraindica. As marcadas como Revisar (por exemplo, sem lockfile) ficam por sua conta."
                  />
                )}
                {focus === "apps" && (
                  <>
                    <Section defaultOpen icon={<PackageX />} title="Restos de apps desinstalados" rows={rows.leftovers} selected={selected} onToggle={toggle} onAll={(v) => setMany(rows.leftovers.map((x) => x.key), v)} />
                    {r.unusedApps.length > 0 && (
                      <Card className="flex items-center gap-3 px-4 py-3">
                        <span className="grid size-8 place-items-center rounded-lg bg-accent-soft text-accent">
                          <AppWindow className="size-4" />
                        </span>
                        <div className="min-w-0 flex-1">
                          <div className="font-medium">{formatCount(r.unusedApps.length)} apps sem uso há mais de 6 meses</div>
                          <div className="truncate text-[12px] text-ink-3">{r.unusedApps.map((a) => a.name).join(", ")}</div>
                        </div>
                        <Button size="sm" onClick={() => go("apps")}>
                          Ver em Aplicativos
                        </Button>
                      </Card>
                    )}
                  </>
                )}
              </div>
            )}

            <div className="pointer-events-none sticky bottom-0 -mx-8 flex flex-col items-center gap-1.5 bg-[linear-gradient(to_top,color-mix(in_srgb,var(--deep)_85%,transparent),transparent)] pt-10 pb-4">
              <div className="pointer-events-auto">
                <RoundAction disabled={!chosenCount && !emptyTrash} onClick={() => setConfirm(true)}>
                  Limpar
                </RoundAction>
              </div>
              <div className="text-[12px] text-ink-2">
                <span className="font-medium text-ink">{formatCount(chosenCount + (emptyTrash ? 1 : 0))}</span> ações ·{" "}
                <span className="tabular font-medium text-ink">{formatBytes(chosenBytes)}</span>
              </div>
            </div>
          </>
        )}
      </div>

      {confirm && r && (
        <Modal onClose={busy ? undefined : () => setConfirm(false)} width={480}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">Confirmar limpeza</h2>
            <ul className="mt-3 space-y-1 text-[12.5px] text-ink-2">
              {emptyTrash && (
                <li className="text-danger">
                  Apagar de vez {formatCount(r.trash.items.length)} itens da Lixeira ({formatBytes(r.trash.bytes)})
                </li>
              )}
              {chosenCount > 0 && (
                <li>
                  Mover {formatCount(chosenCount)} itens para a Lixeira ({formatBytes(chosenBytes - (emptyTrash ? r.trash.bytes : 0))}); dá para recuperar pelo Finder
                </li>
              )}
            </ul>
            {unsyncedRepos.length > 0 && rows?.artifacts.some((a) => selected.has(a.key)) && (
              <p className="mt-3 rounded-lg bg-[color-mix(in_srgb,var(--review)_12%,transparent)] px-3 py-2 text-[12px] text-review">
                {formatCount(unsyncedRepos.length)} {unsyncedRepos.length === 1 ? "repositório tem" : "repositórios têm"} trabalho não enviado. Remover pastas regeneráveis não toca nesses
                arquivos, mas vale fazer um snapshot em Projetos antes.
              </p>
            )}
            <p className="mt-3 text-[11.5px] text-ink-3">Cada item é validado de novo antes: pastas do sistema, arquivos alterados desde a análise e a última cópia de um duplicado são sempre preservados.</p>
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button onClick={() => setConfirm(false)} disabled={busy}>
              Cancelar
            </Button>
            <Button variant={emptyTrash ? "danger" : "primary"} busy={busy} onClick={run}>
              Limpar {formatBytes(chosenBytes)}
            </Button>
          </div>
        </Modal>
      )}

      {outcomes && (
        <Modal onClose={() => { setOutcomes(null); start(MODULE, "start_smart_care", {}); }} width={480}>
          <div className="p-6">
            <h2 className="flex items-center gap-2 text-[16px] font-semibold">
              <Check className="size-5 text-safe" /> Limpeza concluída
            </h2>
            <p className="mt-1.5 text-ink-2">
              {formatCount(outcomes.filter((o) => o.ok).length)} itens · {formatBytes(outcomes.filter((o) => o.ok).reduce((s, o) => s + o.size, 0))}
              {outcomes.some((o) => !o.ok) && `. ${outcomes.filter((o) => !o.ok).length} não foram processados (detalhes em Ajustes → Registro de operações).`}
            </p>
          </div>
          <div className="flex justify-end border-t border-line px-6 py-3">
            <Button variant="primary" onClick={() => { setOutcomes(null); start(MODULE, "start_smart_care", {}); }}>
              Analisar de novo
            </Button>
          </div>
        </Modal>
      )}
    </div>
  );
}
