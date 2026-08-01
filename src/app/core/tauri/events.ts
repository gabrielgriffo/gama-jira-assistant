import { listen, UnlistenFn } from "@tauri-apps/api/event";

export async function onEvent<T>(eventName: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(eventName, (event) => handler(event.payload));
}
