export const MISSING_CREDENTIALS_MARKER = "nenhuma credencial";
export const MISSING_CREDENTIALS_CODE = "credentials_not_found";

export class CommandError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.name = "CommandError";
  }

  override toString(): string {
    return this.message;
  }
}

export function toCommandError(raw: unknown): unknown {
  if (raw && typeof raw === "object" && "code" in raw && "message" in raw) {
    const { code, message } = raw as { code: unknown; message: unknown };
    if (typeof code === "string" && typeof message === "string") return new CommandError(code, message);
  }
  return raw;
}

export function isMissingCredentials(err: unknown): boolean {
  if (err instanceof CommandError) return err.code === MISSING_CREDENTIALS_CODE;
  return String(err).includes(MISSING_CREDENTIALS_MARKER);
}
