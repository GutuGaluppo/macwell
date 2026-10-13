// Where removed items go: the Trash (default) or the quarantine. One answer for the whole UI, so the
// words on every review screen match what the Rust side will actually do.
import { useQuery } from "@tanstack/react-query";
import { api } from "./ipc";
import type { RemovalMode } from "../types";

export function useRemovalMode() {
  const { data } = useQuery({ queryKey: ["removalMode"], queryFn: api.getRemovalMode, staleTime: 5_000 });
  const mode: RemovalMode = data ?? "trash";
  return {
    mode,
    quarantine: mode === "quarantine",
    /** "a Lixeira" or "a Quarentena", to go after "para". */
    destination: mode === "quarantine" ? "a Quarentena" : "a Lixeira",
  };
}
