import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ChevronLeft, Copy, FileSearch, FolderOpen, Hourglass, Image as ImageIcon, Search } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { useNav } from "../../stores/nav";
import { ModuleStage } from "../../components/ModuleStage";
import { BentoButton, BentoIcon, ResultsBento } from "../../components/ResultsBento";
import clutterHero from "../../assets/icons/minha-bagunca-hd.webp";
import { Button, EmptyState, ErrorNote, PageHeader } from "../../components/ui";
import { FileTable, useSelection } from "../../components/FileTable";
import { ReviewDialog, SelectionBar } from "../../components/ReviewDialog";
import { ScanProgressCard, WarningsNote, useHome } from "../scanner/ScannerPage";
import { formatBytes, formatCount, shortPath } from "../../lib/format";
import type { DuplicatesResult, FileCategory, FileFilter, FindResult, SimilarResult } from "../../types";

const MODULE = "largeFiles";

const sizePresets = [
  { label: "> 100 MB", value: 100e6 },
  { label: "> 500 MB", value: 500e6 },
  { label: "> 1 GB", value: 1e9 },
];
const agePresets = [
  { label: "Qualquer data", value: null },
  { label: "> 6 meses", value: 182 },
  { label: "> 1 ano", value: 365 },
  { label: "> 2 anos", value: 730 },
];
const typePresets: { label: string; value: FileCategory[] }[] = [
  { label: "Todos os tipos", value: [] },
  { label: "Vídeos", value: ["video"] },
  { label: "Imagens", value: ["image"] },
  { label: "Áudio", value: ["audio"] },
  { label: "Documentos", value: ["document"] },
  { label: "Compactados", value: ["archive"] },
  { label: "Imagens de disco e instaladores", value: ["diskImage", "installer"] },
];

function Chips<T>({ options, value, onChange }: { options: { label: string; value: T }[]; value: T; onChange: (v: T) => void }) {
  return (
    <div className="inline-flex rounded-lg glass p-0.5">
      {options.map((o) => {
        const active = JSON.stringify(o.value) === JSON.stringify(value);
        return (
          <button
            key={o.label}
            onClick={() => onChange(o.value)}
            className={`h-7 rounded-md px-2.5 text-[12px] transition ${active ? "bg-accent text-accent-ink font-medium" : "text-ink-2 hover:text-ink"}`}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

function LargeFilesDetails({ onBack }: { onBack: () => void }) {
  const home = useHome();
  const job = useJob<FindResult>(MODULE);
  const start = useJobs((s) => s.start);
  const update = useJobs((s) => s.update);
  const { data: locations } = useQuery({ queryKey: ["locations"], queryFn: api.suggestedLocations });
  const [root, setRoot] = useState<string | null>(null);
  const [minSize, setMinSize] = useState(500e6);
  const [olderThan, setOlderThan] = useState<number | null>(null);
  const [categories, setCategories] = useState<FileCategory[]>([]);
  const [includeLibrary, setIncludeLibrary] = useState(false);
  const [review, setReview] = useState(false);
  const { selected, setSelected, onSelect } = useSelection();
  const r = job.result;
  const effectiveRoot = root ?? locations?.find((l) => l.id === "home")?.path ?? home ?? "";

  const run = () => {
    setSelected(new Set());
    const filter: FileFilter = {
      minSize,
      olderThanDays: olderThan,
      dateField: "modified",
      extensions: [],
      categories,
      excludeLibrary: !includeLibrary,
      includeHidden: false,
    };
    start(MODULE, "start_find_files", { root: effectiveRoot, filter });
  };
  const choose = async () => {
    const dir = await open({ directory: true, title: "Onde procurar" });
    if (typeof dir === "string") setRoot(dir);
  };
  const removeRows = (paths: string[]) => {
    const gone = new Set(paths);
    update<FindResult>(MODULE, (res) => ({
      ...res,
      matches: res.matches.filter((f) => !gone.has(f.path)),
      matched: res.matched - paths.length,
    }));
    setSelected((s) => new Set([...s].filter((p) => !gone.has(p))));
  };

  const files = r?.matches ?? [];
  const selectedFiles = useMemo(() => files.filter((f) => selected.has(f.path)), [files, selected]);
  const selectedBytes = selectedFiles.reduce((s, f) => s + f.sizeLogical, 0);
  const running = job.status === "running";

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Arquivos grandes e antigos"
        subtitle="Encontre os arquivos que mais ocupam espaço e os que você não abre há muito tempo. Arquivos dentro de apps e bibliotecas (Fotos, Música) não são listados separadamente."
        actions={
          <Button variant="ghost" icon={<ChevronLeft className="size-3.5" />} onClick={onBack}>
            Voltar aos resultados
          </Button>
        }
      />
      <div className="px-8 pb-2">
        <div className="space-y-3 rounded-xl glass p-4">
          <div className="flex flex-wrap items-center gap-3">
            <span className="w-16 text-[12px] text-ink-3">Onde</span>
            <button
              onClick={choose}
              disabled={running}
              className="flex h-8 min-w-0 max-w-md items-center gap-2 rounded-lg border border-line bg-surface-2 px-3 text-[12.5px] hover:border-accent/50"
            >
              <FolderOpen className="size-4 shrink-0 text-accent" />
              <span className="truncate">{effectiveRoot === home ? "Pasta pessoal (~)" : shortPath(effectiveRoot, home) || "…"}</span>
            </button>
            <label className="flex items-center gap-2 text-[12px] text-ink-2">
              <input type="checkbox" className="accent-[var(--accent)]" checked={includeLibrary} onChange={(e) => setIncludeLibrary(e.target.checked)} />
              Incluir ~/Library (dados de apps)
            </label>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <span className="w-16 text-[12px] text-ink-3">Tamanho</span>
            <Chips options={sizePresets} value={minSize} onChange={setMinSize} />
            <span className="ml-3 text-[12px] text-ink-3">Sem modificação há</span>
            <Chips options={agePresets} value={olderThan} onChange={setOlderThan} />
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <span className="w-16 text-[12px] text-ink-3">Tipo</span>
            <Chips options={typePresets} value={categories} onChange={setCategories} />
            <div className="flex-1" />
            <Button variant="primary" icon={<Search className="size-3.5" />} onClick={run} disabled={running || !effectiveRoot}>
              Buscar
            </Button>
          </div>
        </div>
      </div>
      <div className="space-y-4 px-8 pt-3 pb-4">
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
        {running && <ScanProgressCard module={MODULE} label="Procurando arquivos" />}
        {job.status === "idle" && (
          <EmptyState icon={<FileSearch className="size-6" />} title="Defina os filtros e busque">
            A data usada é a da última modificação: o macOS não registra de forma confiável quando um arquivo foi aberto.
          </EmptyState>
        )}
        {r && !running && (
          <>
            <div className="flex items-baseline justify-between">
              <div className="text-[13px] text-ink-2">
                <span className="font-semibold text-ink">{formatCount(r.matched)}</span> {r.matched === 1 ? "arquivo" : "arquivos"} ·{" "}
                <span className="tabular font-semibold text-ink">{formatBytes(r.matchedBytes)}</span>
                {r.truncated && <span className="text-ink-3"> · mostrando os {formatCount(files.length)} maiores</span>}
                {r.cancelled && <span className="text-ink-3"> · busca cancelada, resultado parcial</span>}
              </div>
            </div>
            <WarningsNote module={MODULE} />
            {files.length ? (
              <FileTable rows={files} selected={selected} onSelect={onSelect} onIgnored={(p) => removeRows([p])} home={home} />
            ) : (
              <EmptyState icon={<FileSearch className="size-6" />} title="Nada encontrado com esses filtros" />
            )}
          </>
        )}
      </div>
      {r && files.length > 0 && (
        <div className="px-8">
          <SelectionBar count={selectedFiles.length} bytes={selectedBytes} onReview={() => setReview(true)} />
        </div>
      )}
      {review && (
        <ReviewDialog
          items={selectedFiles.map((f) => ({ path: f.path, size: f.sizeLogical, label: f.name }))}
          scanRoot={r?.root}
          home={home}
          onClose={() => setReview(false)}
          onDone={(out) => removeRows(out.filter((o) => o.ok).map((o) => o.path))}
        />
      )}
    </div>
  );
}

const DEFAULT_FILTER: FileFilter = { minSize: 500e6, olderThanDays: null, dateField: "modified", extensions: [], categories: [], excludeLibrary: true, includeHidden: false };
const YEAR_MS = 365 * 86_400_000;

// Intro, scan and results for the whole module. Analisar runs the three analyses with their usual
// defaults (home folder, files over 500 MB, duplicates, ~/Pictures); each tile opens the page with
// the filters to refine it.
export function LargeFilesPage() {
  const home = useHome();
  const go = useNav((s) => s.go);
  const { start, cancel, reset } = useJobs();
  const large = useJob<FindResult>(MODULE);
  const dups = useJob<DuplicatesResult>("duplicates");
  const images = useJob<SimilarResult>("similarImages");
  const [details, setDetails] = useState(false);

  const all = [large, dups, images];
  const phase = all.some((j) => j.status === "running") ? "scanning" : large.status === "done" || large.status === "failed" ? "results" : "intro";
  const scan = () => {
    if (!home) return;
    start(MODULE, "start_find_files", { root: home, filter: DEFAULT_FILTER });
    start("duplicates", "start_duplicate_scan", { root: home, minSize: 1e6 });
    start("similarImages", "start_similar_images", { root: `${home}/Pictures`, sensitivity: "normal" });
  };
  const modules = [MODULE, "duplicates", "similarImages"];

  const files = large.result?.matches ?? [];
  const old = files.filter((f) => (f.modifiedAt ?? Date.now()) < Date.now() - YEAR_MS);
  const oldBytes = old.reduce((s, f) => s + f.sizeLogical, 0);
  const copies = dups.result?.totalGroups ?? 0;
  const similar = images.result?.groups.length ?? 0;
  const reading = all.find((j) => j.status === "running" && j.progress?.current)?.progress?.current;

  return (
    <ModuleStage
      phase={phase}
      title="Minha bagunça"
      hero={clutterHero}
      intro="Encontre arquivos grandes, antigos, cópias idênticas e imagens parecidas na sua pasta pessoal. Nada é removido sem a sua confirmação."
      scanning="Cavando fundo…"
      detail={reading ? shortPath(reading, home) : "Lendo a pasta pessoal"}
      onScan={scan}
      onStop={() => modules.forEach((m) => cancel(m))}
      onBack={() => {
        setDetails(false);
        modules.forEach((m) => reset(m));
      }}
    >
      {details ? (
        <LargeFilesDetails onBack={() => setDetails(false)} />
      ) : (
        <>
          {large.status === "failed" && (
            <div className="px-8">
              <ErrorNote>{large.error}</ErrorNote>
            </div>
          )}
          <ResultsBento
            headline={large.result ? `${formatBytes(large.result.matchedBytes)} em arquivos grandes.` : "Resultados da análise"}
            pill={<BentoButton onClick={() => setDetails(true)}>Refinar a busca</BentoButton>}
            extra={<BentoButton onClick={scan}>Analisar de novo</BentoButton>}
            tiles={[
              {
                id: "large",
                title: `${formatCount(large.result?.matched ?? 0)} arquivos acima de 500 MB`,
                description: "Os que mais ocupam espaço na sua pasta pessoal.",
                visual: <BentoIcon size={112}><FileSearch /></BentoIcon>,
                body: (
                  <ul className="space-y-1 text-[12.5px]">
                    {files.slice(0, 3).map((f) => (
                      <li key={f.path} className="flex justify-between gap-3">
                        <span className="truncate text-ink-2">{f.name}</span>
                        <span className="tabular shrink-0">{formatBytes(f.sizeLogical)}</span>
                      </li>
                    ))}
                  </ul>
                ),
                actions: (
                  <BentoButton primary onClick={() => setDetails(true)}>
                    Revisar
                  </BentoButton>
                ),
              },
              {
                id: "duplicates",
                title: `${formatCount(copies)} ${copies === 1 ? "grupo de cópias idênticas" : "grupos de cópias idênticas"}`,
                description: copies ? `Guardar uma só cópia de cada libera ${formatBytes(dups.result?.wastedBytes ?? 0)}.` : "Nenhuma cópia encontrada.",
                visual: <BentoIcon size={56}><Copy /></BentoIcon>,
                actions: (
                  <BentoButton primary disabled={!copies} onClick={() => go("duplicates")}>
                    Revisar
                  </BentoButton>
                ),
              },
              {
                id: "old",
                title: `${formatCount(old.length)} ${old.length === 1 ? "arquivo grande parado" : "arquivos grandes parados"} há mais de 1 ano`,
                description: old.length ? `Ocupam ${formatBytes(oldBytes)}. A data é a da última modificação.` : "Nenhum arquivo grande está parado há tanto tempo.",
                visual: <BentoIcon size={48}><Hourglass /></BentoIcon>,
                actions: (
                  <BentoButton primary disabled={!old.length} onClick={() => setDetails(true)}>
                    Revisar
                  </BentoButton>
                ),
              },
              {
                id: "images",
                title: `${formatCount(similar)} ${similar === 1 ? "grupo de imagens parecidas" : "grupos de imagens parecidas"}`,
                description: similar ? `Podem liberar ${formatBytes(images.result?.reclaimable ?? 0)} em ~/Pictures.` : "Nenhuma imagem parecida em ~/Pictures.",
                visual: <BentoIcon size={48}><ImageIcon /></BentoIcon>,
                actions: (
                  <BentoButton primary disabled={!similar} onClick={() => go("similarImages")}>
                    Revisar
                  </BentoButton>
                ),
              },
            ]}
          />
        </>
      )}
    </ModuleStage>
  );
}
