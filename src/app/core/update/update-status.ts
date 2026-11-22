export type UpdatePhase =
  "idle" | "checking" | "downloading" | "ready" | "installing" | "up-to-date" | "error" | "unavailable";

export interface UpdateStatus {
  phase: UpdatePhase;
  version?: string;
  downloaded?: number;
  total?: number;
  error?: string;
}

export function downloadPercent(downloaded: number, total: number | undefined): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.round((downloaded / total) * 100));
}

export function updateStatusText(status: UpdateStatus, currentVersion: string): string {
  switch (status.phase) {
    case "checking":
      return "Verificando atualizações…";
    case "downloading": {
      const percent = downloadPercent(status.downloaded ?? 0, status.total);
      const base = `Baixando a versão ${status.version}…`;
      return percent === null ? base : `${base} ${percent}%`;
    }
    case "ready":
      return `A versão ${status.version} foi baixada e está pronta para instalar.`;
    case "installing":
      return `Instalando a versão ${status.version}…`;
    case "up-to-date":
      return `Versão ${currentVersion}. Esta é a versão mais recente.`;
    case "error":
      return `Não foi possível atualizar: ${status.error}`;
    case "unavailable":
      return `Versão ${currentVersion}. A atualização automática não funciona no modo de desenvolvimento.`;
    default:
      return `Versão ${currentVersion}`;
  }
}
