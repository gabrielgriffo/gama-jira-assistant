import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { toCommandError } from "./errors";

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (err) {
    throw toCommandError(err);
  }
}
