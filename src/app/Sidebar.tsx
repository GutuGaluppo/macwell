import { Settings } from "lucide-react";
import { useNav, type Section } from "../stores/nav";
import { useJobs } from "../stores/jobs";
import { useStage } from "../stores/stage";
import { appIcon, home, modules, smartCare, tools, type Module } from "./modules";

// Rows sit on the window gradient; the active one becomes a glass pill.
const rowClass = (active: boolean) =>
  `flex w-full items-center gap-2.5 rounded-lg px-2 text-left text-[13px] text-ink transition-colors duration-150 ${
    active ? "glass font-semibold" : "font-medium border border-transparent hover:bg-white/10"
  }`;

function useRunning(jobs: (string | undefined)[]) {
  return useJobs((s) => jobs.some((j) => j && s.jobs[j]?.status === "running"));
}

function Pulse() {
  return <span className="size-1.5 shrink-0 rounded-full bg-safe animate-pulse" aria-label="em andamento" />;
}

function SubItem({ id, label, job }: { id: Section; label: string; job?: string }) {
  const { section, go } = useNav();
  const running = useRunning([job]);
  const active = section === id;
  return (
    <button
      onClick={() => go(id)}
      className={`flex h-7 w-full items-center gap-2 rounded-md pr-2 pl-[50px] text-left text-[12.5px] transition-colors duration-150 ${
        active ? "bg-white/15 font-semibold text-ink" : "text-ink-2 hover:bg-white/8 hover:text-ink"
      }`}
    >
      <span className="flex-1 truncate">{label}</span>
      {running && <Pulse />}
    </button>
  );
}

function ModuleItem({ module, compact }: { module: Module; compact: boolean }) {
  const { section, go } = useNav();
  const running = useRunning(module.sections.map((s) => s.job));
  const inside = module.sections.some((s) => s.id === section);
  const expanded = inside && module.sections.length > 1 && !compact;
  return (
    <div>
      <button
        onClick={() => go(module.sections[0].id)}
        title={compact ? module.label : undefined}
        className={`${rowClass(inside)} relative h-[38px] ${compact ? "justify-center" : ""}`}
      >
        <img src={module.icon} alt="" className="size-[30px] shrink-0 object-contain" draggable={false} />
        {!compact && <span className="flex-1 truncate">{module.label}</span>}
        {!compact && module.badge && <small className="rounded bg-white/18 px-1.5 py-px text-[9px] font-medium text-ink-2">{module.badge}</small>}
        {running && !expanded && (compact ? <span className="absolute top-1 right-1.5"><Pulse /></span> : <Pulse />)}
      </button>
      {expanded && (
        <div className="mt-[3px] space-y-px">
          {module.sections.map((s) => (
            <SubItem key={s.id} {...s} />
          ))}
        </div>
      )}
    </div>
  );
}

function Group({ label, items, compact }: { label?: string; items: Module[]; compact: boolean }) {
  return (
    <div>
      {label && (compact ? <div className="mx-2 mt-[11px] mb-1.5 border-t border-white/12" /> : <div className="mx-2 mt-[19px] mb-1.5 text-[11px] font-bold text-ink-2">{label}</div>)}
      <div className="space-y-[3px]">
        {items.map((m) => (
          <ModuleItem key={m.id} module={m} compact={compact} />
        ))}
      </div>
    </div>
  );
}

export function Sidebar() {
  const { section, go } = useNav();
  const compact = useStage((s) => s.compact);
  return (
    <nav
      className={`drag flex h-full shrink-0 flex-col pt-[52px] pb-3.5 transition-[width,padding] duration-300 [text-shadow:0_1px_3px_rgb(0_0_0/0.3)] ${compact ? "w-[76px] px-3" : "w-[212px] px-3"}`}
    >
      <div className={`flex items-center gap-2.5 pb-5 text-[15px] font-semibold text-ink ${compact ? "justify-center" : "px-2"}`}>
        <img src={appIcon} alt="" className="size-[30px] object-contain" draggable={false} />
        {!compact && "OrganizeMyMac"}
      </div>
      <div className="-mx-1 flex-1 overflow-y-auto px-1 [-webkit-app-region:no-drag]">
        <Group items={[home, smartCare]} compact={compact} />
        <Group label="Módulos" items={modules} compact={compact} />
        <Group label="Ferramentas" items={tools} compact={compact} />
      </div>
      <div className="pt-3 [-webkit-app-region:no-drag]">
        <button onClick={() => go("settings")} title={compact ? "Ajustes" : undefined} className={`${rowClass(section === "settings")} h-[38px] ${compact ? "justify-center" : ""}`}>
          <span className="grid size-[30px] shrink-0 place-items-center text-ink-2">
            <Settings className="size-[18px]" />
          </span>
          {!compact && <span className="flex-1 truncate">Ajustes</span>}
        </button>
      </div>
    </nav>
  );
}
