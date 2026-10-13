// The user-facing modules. Each one groups one or more sections; the sidebar and the home page
// both read from here so they stay in step.
import type { Section } from "../stores/nav";
import inicio from "../assets/icons/inicio.webp";
import cuidadoInteligente from "../assets/icons/cuidado-inteligente.webp";
import limpeza from "../assets/icons/limpeza.webp";
import seguranca from "../assets/icons/seguranca.webp";
import desempenho from "../assets/icons/desempenho.webp";
import aplicativos from "../assets/icons/aplicativos.webp";
import minhaBagunca from "../assets/icons/minha-bagunca.webp";
import mapaDeEspaco from "../assets/icons/mapa-de-espaco.webp";
import scanner from "../assets/icons/scanner.webp";
export { default as appIcon } from "../assets/icons/app-icon.webp";

export type Module = {
  id: string;
  label: string;
  icon: string;
  badge?: string;
  /** What the module looks at, shown large on the home page cards. */
  tagline?: string;
  /** Sections inside the module; the first one opens when the module is chosen. */
  sections: { id: Section; label: string; job?: string }[];
};

export const home: Module = { id: "home", label: "Início", icon: inicio, sections: [{ id: "overview", label: "Início" }] };

export const smartCare: Module = {
  id: "smartCare",
  label: "Cuidado inteligente",
  icon: cuidadoInteligente,
  sections: [{ id: "smartCare", label: "Cuidado inteligente", job: "smartCare" }],
};

export const modules: Module[] = [
  {
    id: "cleanup",
    label: "Limpeza",
    tagline: "Caches, logs e sobras",
    icon: limpeza,
    sections: [
      { id: "cleanup", label: "Limpeza avançada" },
      { id: "downloads", label: "Downloads", job: "downloads" },
      { id: "cloud", label: "Nuvem", job: "cloud" },
      { id: "trash", label: "Lixeira" },
    ],
  },
  {
    id: "security",
    label: "Segurança",
    tagline: "Auditoria do sistema",
    icon: seguranca,
    badge: "beta",
    sections: [{ id: "security", label: "Auditoria de segurança", job: "security" }],
  },
  {
    id: "dev",
    label: "Projetos",
    tagline: "Repositórios e builds",
    icon: scanner, // placeholder until the module gets its own glass icon
    sections: [
      { id: "projects", label: "Projetos", job: "dev" },
      { id: "backup", label: "Backup" },
    ],
  },
  {
    id: "performance",
    label: "Desempenho",
    tagline: "CPU, memória e bateria",
    icon: desempenho,
    sections: [{ id: "performance", label: "Desempenho" }],
  },
  {
    id: "apps",
    label: "Aplicativos",
    tagline: "Apps e o que eles deixam",
    icon: aplicativos,
    sections: [
      { id: "apps", label: "Aplicativos", job: "apps" },
      { id: "leftovers", label: "Restos de apps", job: "leftovers" },
      { id: "updates", label: "Atualizações", job: "updates" },
    ],
  },
  {
    id: "clutter",
    label: "Minha bagunça",
    tagline: "Grandes, antigos e cópias",
    icon: minhaBagunca,
    sections: [
      { id: "largeFiles", label: "Grandes e antigos", job: "largeFiles" },
      { id: "duplicates", label: "Duplicados", job: "duplicates" },
      { id: "similarImages", label: "Imagens parecidas", job: "similarImages" },
    ],
  },
];

export const tools: Module[] = [
  { id: "spaceMap", label: "Mapa de espaço", icon: mapaDeEspaco, sections: [{ id: "spaceMap", label: "Mapa de espaço" }] },
  { id: "scanner", label: "Scanner", icon: scanner, sections: [{ id: "scanner", label: "Scanner", job: "scanner" }] },
];

export const moduleOf = (section: Section) => [home, smartCare, ...modules, ...tools].find((m) => m.sections.some((s) => s.id === section));
