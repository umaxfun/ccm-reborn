import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "./domain";
import type { InstallProgress } from "./types";

export type InstallFailure = { operationId: string; phase: string; error: string };
type MessageKind = "neutral" | "success" | "error";

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

export function renderInstallStatus(options: {
  message: string;
  messageKind: MessageKind;
  activity: InstallProgress | null;
  failure: InstallFailure | null;
  logPath: string;
}) {
  const { message, messageKind, activity, failure, logPath } = options;
  if (!message) return "";
  const percent = activity?.downloadedBytes !== null && activity?.downloadedBytes !== undefined
    && activity.totalBytes !== null && activity?.totalBytes !== undefined && activity.totalBytes > 0
    ? Math.min(100, Math.round(activity.downloadedBytes / activity.totalBytes * 100))
    : null;
  const detail = percent !== null
    ? `${formatBytes(activity!.downloadedBytes!)} / ${formatBytes(activity!.totalBytes!)} · ${percent}%`
    : activity?.completedFiles !== null && activity?.completedFiles !== undefined
      && activity.totalFiles !== null && activity?.totalFiles !== undefined
      ? `${activity.completedFiles} / ${activity.totalFiles} files`
      : "";
  const diagnostics = messageKind === "error" && failure
    ? `<div class="diagnostics-actions"><button class="ghost compact" data-action="copy-diagnostics">Copy diagnostics</button>${logPath ? `<button class="ghost compact" data-action="open-log-folder">Open logs folder</button><code>${escapeHtml(logPath)}</code>` : ""}</div>`
    : "";
  return `<section class="status ${messageKind} ${activity ? "installing" : ""}" role="status" aria-live="polite">
    <span>${messageKind === "success" ? "✓" : messageKind === "error" ? "!" : activity ? "↻" : "i"}</span>
    <div class="status-copy"><p>${escapeHtml(message)}</p>${activity ? `<div class="install-progress"><div><small>${escapeHtml(activity.phase.replaceAll("-", " ").toUpperCase())}</small><strong>${escapeHtml(detail)}</strong></div><div class="progress-track" aria-label="Installation progress"><i style="width: ${percent ?? 100}%"></i></div></div>` : ""}${diagnostics}</div>
  </section>`;
}

export async function copyInstallDiagnostics(failure: InstallFailure, logPath: string) {
  const text = [
    "CCM Reborn installation diagnostics",
    `Operation: ${failure.operationId || "unavailable"}`,
    `Stage: ${failure.phase || "unknown"}`,
    `Error: ${failure.error}`,
    logPath ? `Log: ${logPath}` : "Log: unavailable",
  ].join("\n");
  await navigator.clipboard.writeText(text);
}

export function openInstallLogFolder() {
  return invoke("open_diagnostic_log_directory");
}

/// Copies the failure report and returns the status message to show, so the
/// application shell does not have to know the wording of either outcome.
export async function copyDiagnosticsMessage(failure: InstallFailure | null, logPath: string) {
  if (!failure) return null;
  try {
    await copyInstallDiagnostics(failure, logPath);
    return { text: "Diagnostics copied to the clipboard.", kind: "success" as const };
  } catch {
    return { text: "Could not copy diagnostics. The log path is shown below.", kind: "error" as const };
  }
}
