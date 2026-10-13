// The question asked before a batch cleanup. A screen calls `withBackupAck(...)` right before the
// destructive command: if the Mac has a recent backup and nothing is at risk it passes straight
// through; otherwise a dialog asks, and the answer travels with the command. The Rust side checks
// that answer against a fresh read, so this is a convenience, not the protection.
import { create } from "zustand";
import { api } from "../lib/ipc";
import type { BackupAck, BackupGate } from "../types";

type Pending = { gate: BackupGate; resolve: (ack: BackupAck | null) => void };

export const useBackupGateDialog = create<{ pending: Pending | null }>(() => ({ pending: null }));

/** Thrown when the user cancels the dialog: nothing was changed. */
export class GateCancelled extends Error {
  constructor() {
    super("Cancelado: nada foi alterado.");
  }
}

/** Resolves with the acknowledgement to send, or `null` if the user cancelled. Rejects if the gate cannot be read. */
export async function requestBackupAck(): Promise<BackupAck | null> {
  const gate = await api.getBackupGate(); // fail closed: no answer, no cleanup
  if (gate.level === "ok") return { acknowledged: true };
  return new Promise((resolve) => {
    useBackupGateDialog.setState({
      pending: {
        gate,
        resolve: (ack) => {
          useBackupGateDialog.setState({ pending: null });
          resolve(ack);
        },
      },
    });
  });
}

/** Runs `action` with the acknowledgement the gate requires. */
export async function withBackupAck<T>(action: (ack: BackupAck) => Promise<T>): Promise<T> {
  const ack = await requestBackupAck();
  if (!ack) throw new GateCancelled();
  return action(ack);
}
