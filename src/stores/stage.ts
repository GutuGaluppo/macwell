import { create } from "zustand";

// Whether the page in view is a module stage past its intro (scanning or results). The sidebar
// shrinks to an icon rail while this is set, so the module has the whole window.
type StageStore = {
  compact: boolean;
  setCompact: (value: boolean) => void;
};

export const useStage = create<StageStore>((set) => ({
  compact: false,
  setCompact: (compact) => set({ compact }),
}));
