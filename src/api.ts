import { invoke } from "@tauri-apps/api/core";
import type { Request } from "./types";

export async function request<T>(value: Request): Promise<T> {
  if ("__TAURI_INTERNALS__" in window)
    return invoke<T>("signal_request", { request: value });
  const response = await fetch("/api/signal", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(value),
  });
  const reply = await response.json();
  if (!reply.ok) throw new Error(reply.error || "Engine request failed");
  return reply.data as T;
}
