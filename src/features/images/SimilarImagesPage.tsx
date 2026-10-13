import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useQuery } from "@tanstack/react-query";
import { Check, Eye, FolderOpen, Images, ImageOff, Library, Search } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { Badge, Button, Card, EmptyState, ErrorNote, PageHeader, ProgressBar, Stat } from "../../components/ui";
import { ImagesLoader } from "../../components/loaders";
import { Modal, ReviewDialog, SelectionBar } from "../../components/ReviewDialog";
import { WarningsNote, useHome } from "../scanner/ScannerPage";
import { formatBytes, formatCount, parentFolder, shortPath } from "../../lib/format";
import type { Sensitivity, SimilarPhotosResult, SimilarResult } from "../../types";

const FILES = "similarImages";
const PHOTOS = "similarPhotos";

function Thumb({ src, loading }: { src?: string | null; loading?: boolean }) {
  return (
    <div className="grid aspect-[4/3] w-full place-items-center overflow-hidden rounded-lg bg-surface-3">
      {src ? <img src={src} alt="" className="h-full w-full object-cover" draggable={false} /> : loading ? null : <ImageOff className="size-5 text-ink-3" />}
    </div>
  );
}

function FileThumb({ path }: { path: string }) {
  const { data, isLoading } = useQuery({ queryKey: ["thumb", path], queryFn: () => api.imageThumbnail(path), staleTime: Infinity });
  return <Thumb src={data} loading={isLoading} />;
}

function PhotoThumb({ id }: { id: string }) {
  const { data, isLoading } = useQuery({ queryKey: ["photoThumb", id], queryFn: () => api.photoThumbnail(id), staleTime: Infinity });
  return <Thumb src={data} loading={isLoading} />;
}

function Progress({ module, label }: { module: string; label: string }) {
  const job = useJob(module);
  const cancel = useJobs((s) => s.cancel);
  const st = job.stage;
  const stageLabel = !st ? "Procurando imagens" : st.stage === "thumbnails" ? "Lendo miniaturas" : "Comparando com o Vision";
  return (
    <Card className="p-6">
      <div className="flex items-start justify-between">
        <div>
          <div className="text-[15px] font-semibold">{label}</div>
          <div className="mt-0.5 text-[12px] text-ink-3">
            {stageLabel}
            {st ? ` · ${formatCount(st.done)} de ${formatCount(st.total)}` : job.progress ? ` · ${formatCount(job.progress.files)} arquivos` : ""}
          </div>
        </div>
        <Button size="sm" onClick={() => cancel(module)}>
          Cancelar
        </Button>
      </div>
      <div className="mt-4">
        <ImagesLoader />
      </div>
      <div className="mt-4">
        <ProgressBar value={st && st.total ? st.done / st.total : undefined} indeterminate={!st} />
      </div>
      <p className="mt-4 text-[11.5px] text-ink-3">
        Primeiro um hash perceptivo descarta as imagens sem parecidas; só as que sobram passam pelo Vision, que compara o conteúdo.
      </p>
    </Card>
  );
}

function SensitivityChips({ value, onChange }: { value: Sensitivity; onChange: (s: Sensitivity) => void }) {
  const opts: { v: Sensitivity; label: string }[] = [
    { v: "strict", label: "Quase idênticas" },
    { v: "normal", label: "Parecidas" },
  ];
  return (
    <div className="inline-flex rounded-lg glass p-0.5">
      {opts.map((o) => (
        <button key={o.v} onClick={() => onChange(o.v)} className={`h-7 rounded-md px-2.5 text-[12px] ${value === o.v ? "bg-accent font-medium text-accent-ink" : "text-ink-2 hover:text-ink"}`}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Tile({ children, selected, keep, onToggle, disabled, footer }: { children: React.ReactNode; selected: boolean; keep: boolean; onToggle: () => void; disabled: boolean; footer: React.ReactNode }) {
  return (
    <div className={`group relative rounded-xl border p-2 transition ${selected ? "border-danger/50 bg-danger-soft" : "glass"}`}>
      <button className="block w-full" onClick={onToggle} disabled={disabled} title={disabled ? "Pelo menos uma imagem de cada grupo fica" : undefined}>
        {children}
      </button>
      <div className="absolute top-3 left-3">
        {selected ? (
          <span className="grid size-5 place-items-center rounded-full bg-danger text-white">
            <Check className="size-3" />
          </span>
        ) : (
          keep && <Badge tone="safe">Manter</Badge>
        )}
      </div>
      <div className="mt-1.5 px-0.5">{footer}</div>
    </div>
  );
}

function FolderMode() {
  const home = useHome();
  const job = useJob<SimilarResult>(FILES);
  const start = useJobs((s) => s.start);
  const update = useJobs((s) => s.update);
  const [root, setRoot] = useState<string | null>(null);
  const [sensitivity, setSensitivity] = useState<Sensitivity>("normal");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [review, setReview] = useState(false);
  const r = job.result;
  const running = job.status === "running";
  const effectiveRoot = root ?? (home ? `${home}/Pictures` : "");

  useEffect(() => {
    if (r) setSelected(new Set(r.groups.flatMap((g) => g.images.filter((i) => !i.keep).map((i) => i.path))));
  }, [r?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const all = useMemo(() => (r?.groups ?? []).flatMap((g) => g.images), [r]);
  const chosen = all.filter((i) => selected.has(i.path));
  const toggle = (path: string, groupPaths: string[]) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else if (groupPaths.filter((p) => !next.has(p)).length > 1) next.add(path);
      return next;
    });

  return (
    <>
      <div className="flex flex-wrap items-center gap-3 rounded-xl glass p-4">
        <button
          onClick={async () => {
            const dir = await open({ directory: true, title: "Onde procurar imagens parecidas" });
            if (typeof dir === "string") setRoot(dir);
          }}
          disabled={running}
          className="flex h-8 max-w-sm items-center gap-2 rounded-lg border border-line bg-surface-2 px-3 text-[12.5px] hover:border-accent/50"
        >
          <FolderOpen className="size-4 shrink-0 text-accent" />
          <span className="truncate">{shortPath(effectiveRoot, home)}</span>
        </button>
        <SensitivityChips value={sensitivity} onChange={setSensitivity} />
        <div className="flex-1" />
        <Button variant="primary" icon={<Search className="size-3.5" />} disabled={running || !effectiveRoot} onClick={() => start(FILES, "start_similar_images", { root: effectiveRoot, sensitivity })}>
          Procurar
        </Button>
      </div>
      {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
      {running && <Progress module={FILES} label="Procurando imagens parecidas" />}
      {job.status === "idle" && (
        <EmptyState icon={<Images className="size-6" />} title="Encontre fotos repetidas e versões parecidas">
          Cópias redimensionadas, recomprimidas, recortadas e sequências de fotos quase iguais. A imagem de maior resolução de cada grupo fica.
        </EmptyState>
      )}
      {r && !running && (
        <>
          {!r.cancelled && (
            <Card className="grid grid-cols-4 gap-4 p-5">
              <Stat label="Grupos" value={formatCount(r.groups.length)} />
              <Stat label="Pode liberar" value={formatBytes(r.reclaimable)} />
              <Stat label="Imagens lidas" value={formatCount(r.images)} hint={r.truncated ? "limite de 30 mil atingido" : undefined} />
              <Stat label="Comparadas pelo Vision" value={formatCount(r.vision.featurePrints)} hint={`em ${(r.vision.elapsedMs / 1000).toFixed(1).replace(".", ",")} s`} />
            </Card>
          )}
          <WarningsNote module={FILES} />
          {!r.cancelled && r.groups.length === 0 && <EmptyState icon={<Images className="size-6" />} title="Nenhuma imagem parecida encontrada" />}
          {r.groups.map((g) => (
            <Card key={g.id} className="p-4">
              <div className="mb-3 flex items-baseline gap-2">
                <span className="font-medium">{g.images.length} imagens parecidas</span>
                <span className="text-[12px] text-ink-3">{formatBytes(g.reclaimable)} a liberar mantendo a melhor</span>
              </div>
              <div className="grid grid-cols-[repeat(auto-fill,minmax(170px,1fr))] gap-3">
                {g.images.map((img) => {
                  const paths = g.images.map((i) => i.path);
                  const isSel = selected.has(img.path);
                  const lastKept = !isSel && paths.filter((p) => !selected.has(p)).length === 1;
                  return (
                    <Tile
                      key={img.path}
                      selected={isSel}
                      keep={img.keep}
                      disabled={lastKept}
                      onToggle={() => toggle(img.path, paths)}
                      footer={
                        <>
                          <div className="flex items-center gap-1">
                            <span className="min-w-0 flex-1 truncate text-[12px]" title={img.path}>
                              {img.name}
                            </span>
                            <button className="text-ink-3 hover:text-ink" title="Visualização Rápida" onClick={() => api.quickLook(img.path)}>
                              <Eye className="size-3.5" />
                            </button>
                          </div>
                          <div className="truncate text-[11px] text-ink-3">
                            {img.width}×{img.height} · {formatBytes(img.sizeLogical)}
                          </div>
                          <div className="truncate text-[11px] text-ink-3">{shortPath(parentFolder(img.path), home)}</div>
                        </>
                      }
                    >
                      <FileThumb path={img.path} />
                    </Tile>
                  );
                })}
              </div>
            </Card>
          ))}
          {r.groups.length > 0 && <SelectionBar count={chosen.length} bytes={chosen.reduce((s, i) => s + i.sizeLogical, 0)} onReview={() => setReview(true)} />}
        </>
      )}
      {review && r && (
        <ReviewDialog
          items={chosen.map((i) => ({ path: i.path, size: i.sizeLogical, label: `${i.name} · ${i.width}×${i.height}` }))}
          scanRoot={r.root}
          home={home}
          onClose={() => setReview(false)}
          onDone={(out) => {
            const gone = new Set(out.filter((o) => o.ok).map((o) => o.path));
            update<SimilarResult>(FILES, (res) => {
              const groups = res.groups
                .map((g) => ({ ...g, images: g.images.filter((i) => !gone.has(i.path)) }))
                .filter((g) => g.images.length > 1)
                .map((g) => ({ ...g, reclaimable: g.images.filter((i) => !i.keep).reduce((s, i) => s + i.sizeLogical, 0) }));
              return { ...res, groups, reclaimable: groups.reduce((s, g) => s + g.reclaimable, 0) };
            });
            setSelected((s) => new Set([...s].filter((p) => !gone.has(p))));
          }}
        />
      )}
    </>
  );
}

function PhotosMode() {
  const job = useJob<SimilarPhotosResult>(PHOTOS);
  const start = useJobs((s) => s.start);
  const update = useJobs((s) => s.update);
  const [sensitivity, setSensitivity] = useState<Sensitivity>("normal");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { data: status, refetch } = useQuery({ queryKey: ["photosStatus"], queryFn: () => api.photosStatus(false) });
  const r = job.result;
  const running = job.status === "running";
  const authorized = status === "authorized" || status === "limited";

  useEffect(() => {
    if (r) setSelected(new Set(r.groups.flatMap((g) => g.filter((m) => !m.keep).map((m) => m.id))));
  }, [r]);

  const del = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.deletePhotos([...selected]);
      const gone = new Set(selected);
      update<SimilarPhotosResult>(PHOTOS, (res) => ({ ...res, groups: res.groups.map((g) => g.filter((m) => !gone.has(m.id))).filter((g) => g.length > 1) }));
      setSelected(new Set());
      setConfirm(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (!authorized) {
    return (
      <Card className="space-y-3 p-5">
        <div className="font-medium">Acesso ao Fotos</div>
        <p className="text-[12.5px] text-ink-2">
          O app analisa miniaturas pela biblioteca do Fotos (nunca mexe nos arquivos internos dela) e não baixa originais do iCloud. Para
          apagar, o próprio Fotos pede confirmação e as fotos vão para Apagados Recentemente.
        </p>
        {status === "notDetermined" ? (
          <Button variant="primary" onClick={async () => { await api.photosStatus(true); refetch(); }}>
            Permitir acesso ao Fotos…
          </Button>
        ) : (
          <div className="flex items-center gap-3 text-[12.5px] text-ink-2">
            Acesso negado. Libere o OrganizeMyMac em Ajustes do Sistema → Privacidade e Segurança → Fotos.
            <Button size="sm" onClick={() => api.openSystemSettings("photos")}>
              Abrir Ajustes do Sistema
            </Button>
          </div>
        )}
      </Card>
    );
  }

  return (
    <>
      <div className="flex items-center gap-3 rounded-xl glass p-4">
        <span className="flex items-center gap-2 text-[12.5px] text-ink-2">
          <Library className="size-4 text-accent" /> Biblioteca do Fotos
        </span>
        <SensitivityChips value={sensitivity} onChange={setSensitivity} />
        <div className="flex-1" />
        <Button variant="primary" icon={<Search className="size-3.5" />} disabled={running} onClick={() => start(PHOTOS, "start_similar_photos", { sensitivity })}>
          Procurar
        </Button>
      </div>
      {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
      {error && <ErrorNote>{error}</ErrorNote>}
      {running && <Progress module={PHOTOS} label="Procurando fotos parecidas" />}
      {r && !running && (
        <>
          <div className="text-[13px] text-ink-2">
            <span className="font-semibold text-ink">{formatCount(r.groups.length)}</span> grupos · {formatCount(r.vision.analyzed)} fotos analisadas
          </div>
          {r.groups.map((g) => (
            <Card key={g[0].id} className="p-4">
              <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-3">
                {g.map((m) => {
                  const isSel = selected.has(m.id);
                  const lastKept = !isSel && g.filter((x) => !selected.has(x.id)).length === 1;
                  return (
                    <Tile
                      key={m.id}
                      selected={isSel}
                      keep={m.keep}
                      disabled={lastKept}
                      onToggle={() =>
                        setSelected((prev) => {
                          const next = new Set(prev);
                          if (next.has(m.id)) next.delete(m.id);
                          else next.add(m.id);
                          return next;
                        })
                      }
                      footer={<div className="text-[11px] text-ink-3">{m.width}×{m.height}</div>}
                    >
                      <PhotoThumb id={m.id} />
                    </Tile>
                  );
                })}
              </div>
            </Card>
          ))}
          {r.groups.length > 0 && (
            <div className="sticky bottom-0 -mx-8 flex items-center gap-3 glass-bar px-8 py-3">
              <div className="flex-1 text-[12.5px] text-ink-2">{formatCount(selected.size)} fotos selecionadas</div>
              <Button variant="primary" disabled={!selected.size} onClick={() => setConfirm(true)}>
                Apagar no Fotos…
              </Button>
            </div>
          )}
        </>
      )}
      {confirm && (
        <Modal onClose={busy ? undefined : () => setConfirm(false)} width={440}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">Apagar {formatCount(selected.size)} fotos?</h2>
            <p className="mt-1.5 text-ink-2">O Fotos vai pedir sua confirmação. As fotos ficam em Apagados Recentemente por 30 dias, e somem de todos os aparelhos com a mesma Fototeca do iCloud.</p>
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button onClick={() => setConfirm(false)} disabled={busy}>
              Cancelar
            </Button>
            <Button variant="danger" busy={busy} onClick={del}>
              Continuar no Fotos
            </Button>
          </div>
        </Modal>
      )}
    </>
  );
}

export function SimilarImagesPage() {
  const [mode, setMode] = useState<"folder" | "photos">("folder");
  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Imagens parecidas"
        subtitle="Compara o conteúdo das imagens com o Vision da Apple, não só os bytes: acha cópias redimensionadas, convertidas e fotos quase iguais."
        actions={
          <div className="inline-flex rounded-lg glass p-0.5">
            {(["folder", "photos"] as const).map((m) => (
              <button key={m} onClick={() => setMode(m)} className={`h-7 rounded-md px-3 text-[12px] ${mode === m ? "bg-accent font-medium text-accent-ink" : "text-ink-2 hover:text-ink"}`}>
                {m === "folder" ? "Pasta" : "Fotos"}
              </button>
            ))}
          </div>
        }
      />
      <div className="space-y-4 px-8 pb-4">{mode === "folder" ? <FolderMode /> : <PhotosMode />}</div>
    </div>
  );
}
