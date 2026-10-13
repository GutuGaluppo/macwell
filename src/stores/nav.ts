import { create } from "zustand";

export type Section = "overview" | "smartCare" | "scanner" | "spaceMap" | "largeFiles" | "downloads" | "duplicates" | "similarImages" | "cleanup" | "trash" | "quarantine" | "apps" | "leftovers" | "updates" | "homebrew" | "cloud" | "performance" | "security" | "projects" | "backup" | "report" | "settings";

type NavStore = {
  section: Section;
  go: (section: Section) => void;
};

export const useNav = create<NavStore>((set) => ({
  section: "overview",
  go: (section) => set({ section }),
}));
