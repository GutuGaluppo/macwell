import { useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, ShieldCheck, Trash2, XCircle } from "lucide-react";
import { Badge, Button } from "./ui";
import { api, errorMessage } from "../lib/ipc";
import { formatBytes, formatCount, shortPath } from "../lib/format";
import { useRemovalMode } from "../lib/removalMode";
import type { Confidence, OperationOutcome } from "../types";

export type ReviewItem = { path: string; size: number; label?: ReactNode; confidence?: Confidence };

const confidenceBadge: Record<Confidence, ReactNode> = {
  safe: <Badge tone="safe">Seguro</Badge>,
  review: <Badge tone="review">Revisar</Badge>,
  danger: <Badge tone="danger">Risco</Badge>,
};

export function Modal({ children, onClose, width = 620 }: { children: ReactNode; onClose?: () => void; width?: number }) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50 p-6 backdrop-blur-[3px]" onMouseDown={(e) => e.target === e.currentTarget && onClose?.()}>
      <div className="flex max-h-[86vh] w-full flex-col glass-strong modal-legible rounded-2xl" style={{ maxWidth: width }}>
        {children}
      </div>
    </div>
  );
}

/**
 * Last step before anything is removed: every path and size, the total, and one clearly labelled
 * action. Items go to the Trash; the Rust side validates each one again before moving it.
 */
export function ReviewDialog({
  items,
  scanRoot,
  home,
  title,
  execute,
  onClose,
  onDone,
}: {
  /** Custom executor (e.g. leftovers, which the backend re-validates differently). */
  execute?: (items: { path: string; size: number }[]) => Promise<OperationOutcome[]>;
  items: ReviewItem[];
  scanRoot?: string;
  home?: string;
  title?: string;
  onClose: () => void;
  onDone: (outcomes: OperationOutcome[]) => void;
}) {
  const qc = useQueryClient();
  const { quarantine, destination } = useRemovalMode();
  const [busy, setBusy] = useState(false);
  const [outcomes, setOutcomes] = useState<OperationOutcome[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const total = items.reduce((s, i) => s + i.size, 0);

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const plain = items.map((i) => ({ path: i.path, size: i.size }));
      const out = execute ? await execute(plain) : await api.moveToTrash({ items: plain, scanRoot });
      setOutcomes(out);
      qc.invalidateQueries({ queryKey: ["operations"] });
      qc.invalidateQueries({ queryKey: ["metrics"] });
      qc.invalidateQueries({ queryKey: ["volumes"] });
      onDone(out);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (outcomes) {
    const ok = outcomes.filter((o) => o.ok);
    const failed = outcomes.filter((o) => !o.ok);
    return (
      <Modal onClose={onClose}>
        <div className="p-6">
          <div className="flex items-center gap-2 text-[16px] font-semibold">
            <CheckCircle2 className="size-5 text-safe" />
            {formatCount(ok.length)} {ok.length === 1 ? "item movido" : "itens movidos"} para {destination}
          </div>
          <p className="mt-1 text-ink-2">
            {quarantine
              ? `${formatBytes(ok.reduce((s, o) => s + o.size, 0))} continuam no disco, guardados na Quarentena. Abra Quarentena para restaurar ou enviar à Lixeira.`
              : `${formatBytes(ok.reduce((s, o) => s + o.size, 0))} serão liberados quando a Lixeira for esvaziada. Até lá, dá para recuperar tudo pelo Finder.`}
          </p>
          {failed.length > 0 && (
            <div className="mt-4 rounded-lg border border-danger/30 bg-danger-soft p-3">
              <div className="mb-1 flex items-center gap-1.5 text-[12.5px] font-medium text-danger">
                <XCircle className="size-4" /> {failed.length} {failed.length === 1 ? "item não foi movido" : "itens não foram movidos"}
              </div>
              <ul className="max-h-40 space-y-1 overflow-y-auto text-[12px] selectable">
                {failed.map((f) => (
                  <li key={f.path}>
                    <span className="text-ink">{shortPath(f.path, home)}</span> — <span className="text-ink-2">{f.error}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
        <div className="flex justify-end border-t border-line px-6 py-3">
          <Button variant="primary" onClick={onClose}>
            OK
          </Button>
        </div>
      </Modal>
    );
  }

  return (
    <Modal onClose={busy ? undefined : onClose}>
      <div className="border-b border-line px-6 pt-5 pb-4">
        <h2 className="text-[16px] font-semibold">{title ?? `Mover para ${destination}`}</h2>
        <p className="mt-1 text-ink-2">
          {formatCount(items.length)} {items.length === 1 ? "item" : "itens"} · <span className="font-medium text-ink">{formatBytes(total)}</span>. Confira os
          caminhos antes de continuar.
        </p>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-3">
        {items.map((i) => (
          <div key={i.path} className="grid grid-cols-[minmax(0,1fr)_auto_80px] items-center gap-3 border-b border-line py-1.5 last:border-0">
            <div className="min-w-0">
              <div className="truncate text-[12.5px]">{i.label ?? i.path.split("/").pop()}</div>
              <div className="truncate text-[11px] text-ink-3 selectable" title={i.path}>
                {shortPath(i.path, home)}
              </div>
            </div>
            <div>{i.confidence && confidenceBadge[i.confidence]}</div>
            <div className="tabular text-right text-[12.5px]">{formatBytes(i.size)}</div>
          </div>
        ))}
      </div>
      {error && <div className="mx-6 mb-2 rounded-lg bg-danger-soft px-3 py-2 text-[12px] text-danger">{error}</div>}
      <div className="flex items-center gap-3 border-t border-line px-6 py-3">
        <div className="flex flex-1 items-center gap-1.5 text-[11.5px] text-ink-3">
          <ShieldCheck className="size-3.5" />{" "}
          {quarantine ? "Fica na Quarentena, no mesmo disco, e pode ser restaurado." : "Vai para a Lixeira e pode ser recuperado."} Pastas do sistema são sempre bloqueadas.
        </div>
        <Button onClick={onClose} disabled={busy}>
          Cancelar
        </Button>
        <Button variant="danger" busy={busy} icon={<Trash2 className="size-3.5" />} onClick={run}>
          Mover {formatBytes(total)} para {destination}
        </Button>
      </div>
    </Modal>
  );
}

/** Sticky bar with the selection total and the review button. */
export function SelectionBar({ count, bytes, onReview, extra }: { count: number; bytes: number; onReview: () => void; extra?: ReactNode }) {
  const { destination } = useRemovalMode();
  return (
    <div className="sticky bottom-0 z-10 -mx-8 mt-4 flex items-center gap-3 glass-bar px-8 py-3">
      <div className="flex-1 text-[12.5px] text-ink-2">
        {count ? (
          <>
            <span className="font-medium text-ink">{formatCount(count)}</span> {count === 1 ? "item selecionado" : "itens selecionados"} ·{" "}
            <span className="tabular font-medium text-ink">{formatBytes(bytes)}</span>
          </>
        ) : (
          "Selecione o que deseja remover."
        )}
      </div>
      {extra}
      <Button variant="primary" disabled={!count} onClick={onReview} icon={<Trash2 className="size-3.5" />}>
        Revisar e mover para {destination}…
      </Button>
    </div>
  );
}
