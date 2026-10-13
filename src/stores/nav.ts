import { create } from "zustand";

export type Section = "overview" | "smartCare" | "scanner" | "spaceMap" | "largeFiles" | "downloads" | "duplicates" | "similarImages" | "cleanup" | "trash" | "apps" | "leftovers" | "updates" | "cloud" | "performance" | "security" | "projects" | "backup" | "settings";

type NavStore = {
  section: Section;
  go: (section: Section) => void;
};

export const useNav = create<NavStore>((set) => ({
  section: "overview",
  go: (section) => set({ section }),
}));
