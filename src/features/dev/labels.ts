// pt-BR wording for what the Rust side reports as neutral codes.
import { formatBytes } from "../../lib/format";
import type { ArtifactNote, Blocker, RemoteKind, RepoReport, SecretKind, Visibility } from "../../types";

export const artifactKindLabel: Record<string, string> = {
  "node-modules": "node_modules",
  "rust-target": "target (Rust)",
  "maven-target": "target (Maven)",
  "next-build": ".next",
  "turbo-cache": ".turbo",
  "js-dist": "dist",
  "build-output": "build",
  "python-venv": ".venv",
  cocoapods: "Pods",
  "swiftpm-build": ".build (Swift)",
  "xcode-derived-data": "DerivedData",
};

/** Why an artifact is "revisar" instead of "seguro". */
export const noteLabel: Record<ArtifactNote, string> = {
  noLockfile: "Sem arquivo de versões travadas (lockfile): recriar pode trazer versões diferentes.",
  tracked: "Tem arquivos versionados no git: não é só saída de build.",
  notIgnored: "O git não ignora esta pasta: pode ter sido criada à mão ou ser versionada.",
  gitUnknown: "Não foi possível consultar o git para confirmar.",
};

export const secretKindLabel: Record<SecretKind, string> = {
  envFile: "Arquivo .env",
  privateKeyFile: "Arquivo de chave",
  credentialsFile: "Arquivo de credenciais",
  privateKeyBlock: "Chave privada no texto",
  awsAccessKey: "Chave de acesso da AWS",
  githubToken: "Token do GitHub",
  apiKey: "Chave de API (sk-…)",
  slackToken: "Token do Slack",
  stripeKey: "Chave do Stripe",
  googleApiKey: "Chave de API do Google",
  jwt: "Token JWT",
  npmToken: "Token do npm",
};

export function blockerText(b: Blocker): string {
  switch (b.code) {
    case "secrets":
      return `${b.count} ${b.count === 1 ? "arquivo novo parece conter" : "arquivos novos parecem conter"} segredos. Deixe-${b.count === 1 ? "o" : "os"} de fora do snapshot para continuar.`;
    case "fileTooBig":
      return `${b.path} tem ${formatBytes(b.size)}: arquivos novos acima de 50 MB precisam ficar de fora, ou o .git cresceria para sempre.`;
    case "totalTooBig":
      return `Os arquivos novos somam ${formatBytes(b.bytes)}, acima do limite de 500 MB. Deixe alguns de fora.`;
    case "filtersConfigured":
      return "Este repositório usa um filtro do git (como o Git LFS) que executaria programas externos ao salvar. Por segurança, o snapshot não roda aqui.";
    case "nothingToSave":
      return "Não há nada a salvar: o trabalho já está no último commit.";
    case "noFreeRefName":
      return "Já existem três snapshots com o nome de hoje neste repositório. Tente de novo mais tarde.";
  }
}

export function remoteLabel(kind: RemoteKind): string {
  switch (kind.kind) {
    case "github":
      return "GitHub";
    case "local":
      return "Neste Mac";
    case "other":
      return kind.host;
  }
}

export const visibilityLabel: Record<Visibility, string> = {
  public: "Público",
  privateOrMissing: "Privado ou inexistente",
  unknown: "Não foi possível saber",
};

/** Reasons a repository still holds work that exists only on this Mac. */
export function unsyncedReasons(r: RepoReport): string[] {
  const s = r.status;
  if (!s) return [];
  const reasons: string[] = [];
  const dirty = s.changed + s.untracked + s.conflicted;
  if (dirty > 0) reasons.push(`${dirty} ${dirty === 1 ? "alteração" : "alterações"} sem commit`);
  if ((s.ahead ?? 0) > 0) reasons.push(`${s.ahead} ${s.ahead === 1 ? "commit" : "commits"} não ${s.ahead === 1 ? "enviado" : "enviados"}`);
  if (!s.noCommits) {
    if (r.remotes.length === 0) reasons.push("sem remote");
    else if (!s.upstream && !s.detached) reasons.push("branch sem upstream");
  }
  return reasons;
}
