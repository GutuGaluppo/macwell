import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, PackageSearch } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { useNav } from "../../stores/nav";
import { Badge, Button, Card, EmptyState, ErrorNote, PageHeader } from "../../components/ui";
import { useHome } from "../scanner/ScannerPage";
import { formatCount, shortPath } from "../../lib/format";
import type { BrewChange, BrewPackage, BrewRisk } from "../../types";

const groups: { risk: BrewRisk; title: string; note: string; tone: "danger" | "review" | "safe" | "neutral" }[] = [
  { risk: "high", title: "Risco alto", note: "Salto de versão maior, ou um runtime/banco de dados que os projetos usam.", tone: "danger" },
  { risk: "medium", title: "Risco médio", note: "Versão menor, ou uma mudança que não dá para comparar.", tone: "review" },
  { risk: "low", title: "Risco baixo", note: "Correções e revisões.", tone: "safe" },
  { risk: "pinned", title: "Fixados", note: "O brew upgrade ignora estes pacotes: você os fixou.", tone: "neutral" },
];

const changeLabel: Record<BrewChange, string> = { major: "versão maior", minor: "versão menor", patch: "correção", other: "mudança não comparável" };

function Row({ pkg, home }: { pkg: BrewPackage; home?: string }) {
  const [open, setOpen] = useState(false);
  const exactCount = pkg.projects.filter((p) => p.exact).length;
  return (
    <li>
      <div className="grid grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)_auto_auto] items-center gap-3 px-4 py-2.5">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="font-medium">{pkg.name}</span>
            <Badge>{pkg.kind === "cask" ? "app" : "pacote"}</Badge>
            {pkg.runtime && <Badge tone="accent">runtime</Badge>}
          </div>
          <div className="mt-0.5 select-text font-mono text-[11px] text-ink-3">{pkg.upgradeCommand}</div>
        </div>
        <div className="text-[12.5px]">
          <span className="font-mono text-ink-2">{pkg.installed.join(", ") || "?"}</span> <span className="text-ink-3">→</span> <span className="font-mono">{pkg.current}</span>
          <div className="text-[11.5px] text-ink-3">{changeLabel[pkg.change]}</div>
        </div>
        <div className="text-[12px] text-ink-2">
          {pkg.projects.length === 0 ? (
            <span className="text-ink-3">nenhum projeto</span>
          ) : (
            <button className="flex items-center gap-1 hover:text-ink" onClick={() => setOpen(!open)} aria-expanded={open}>
              {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
              {formatCount(pkg.projects.length)} {pkg.projects.length === 1 ? "projeto" : "projetos"}
            </button>
          )}
        </div>
        <div className="w-[88px] text-right">{exactCount === 0 && pkg.projects.length > 0 && <Badge>estimativa</Badge>}</div>
      </div>
      {open && (
        <ul className="space-y-0.5 bg-black/5 px-4 py-2 pl-8">
          {pkg.projects.map((p) => (
            <li key={p.repo} className="flex items-center gap-2 text-[11.5px]">
              <span className="min-w-0 truncate font-mono text-ink-2 selectable" title={p.repo}>
                {shortPath(p.repo, home)}
              </span>
              <span className="text-ink-3">· {p.via}</span>
              <Badge tone={p.exact ? "safe" : "neutral"}>{p.exact ? "no Brewfile" : "estimativa"}</Badge>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

export function HomebrewPage() {
  const home = useHome();
  const go = useNav((s) => s.go);
  const { data, error, isFetching, refetch } = useQuery({ queryKey: ["homebrew"], queryFn: api.analyzeHomebrew, enabled: false, staleTime: Infinity });

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Homebrew"
        subtitle="Pacotes desatualizados, do mais arriscado de atualizar ao menos, e os projetos que provavelmente dependem de cada um. Só leitura: o app não atualiza nem instala nada; você copia o comando se quiser."
        actions={
          <Button variant={data ? "secondary" : "primary"} busy={isFetching} onClick={() => refetch()}>
            {data ? "Analisar de novo" : "Analisar"}
          </Button>
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {error && <ErrorNote>{errorMessage(error)}</ErrorNote>}
        {!data && !error && (
          <EmptyState icon={<PackageSearch className="size-6" />} title="Veja o que vale a pena atualizar">
            Roda <span className="font-mono">brew outdated</span> sem atualizar nada e cruza o resultado com as suas pastas de projetos.
          </EmptyState>
        )}
        {data && !data.installed && <EmptyState icon={<PackageSearch className="size-6" />} title="O Homebrew não está instalado neste Mac" />}
        {data?.error && <ErrorNote>{data.error}</ErrorNote>}
        {data && data.installed && !data.error && data.packages.length === 0 && <EmptyState icon={<PackageSearch className="size-6" />} title="Tudo em dia">Nenhum pacote do Homebrew está desatualizado.</EmptyState>}

        {data && data.packages.length > 0 && (
          <>
            <div className="flex flex-wrap items-center gap-2 text-[12.5px] text-ink-2">
              {formatCount(data.packages.length)} {data.packages.length === 1 ? "pacote desatualizado" : "pacotes desatualizados"}
              {groups.map((g) => {
                const n = data.packages.filter((p) => p.risk === g.risk).length;
                return n > 0 && <Badge key={g.risk} tone={g.tone}>{n} {g.title.toLowerCase()}</Badge>;
              })}
            </div>
            {groups.map((g) => {
              const list = data.packages.filter((p) => p.risk === g.risk);
              if (!list.length) return null;
              return (
                <Card key={g.risk} className="overflow-hidden">
                  <div className="border-b border-line px-4 py-2.5">
                    <span className="font-medium">{g.title}</span>
                    <span className="ml-2 text-[12px] text-ink-3">{g.note}</span>
                  </div>
                  <ul className="divide-y divide-line">
                    {list.map((p) => (
                      <Row key={`${p.kind}:${p.name}`} pkg={p} home={home} />
                    ))}
                  </ul>
                </Card>
              );
            })}
            <p className="text-[11.5px] text-ink-3">
              Os projetos são uma estimativa: vêm de arquivos como <span className="font-mono">package.json</span> ou <span className="font-mono">Cargo.toml</span>. Um{" "}
              <span className="font-mono">Brewfile</span> é exato e aparece marcado.
              {data.reposScanned === 0 && (
                <>
                  {" "}Nenhum projeto foi lido: adicione pastas em{" "}
                  <button className="underline underline-offset-2 hover:text-ink" onClick={() => go("projects")}>
                    Projetos
                  </button>
                  .
                </>
              )}
            </p>
          </>
        )}
      </div>
    </div>
  );
}
