import type { LocalModDraft } from "./local-mods";
import type { LocalPackageInspection } from "./types";

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

const formatBytes = (bytes: number) => bytes < 1024 * 1024
  ? `${Math.max(1, Math.round(bytes / 1024))} KB`
  : `${(bytes / 1024 / 1024).toFixed(bytes > 1024 * 1024 * 1024 ? 1 : 0)} MB`;

/// The fields come from the archive's own `metadata.txt`. They stay editable
/// because real CCM packages frequently ship without an author or a version.
export function renderAddLocalDialog(
  inspection: LocalPackageInspection,
  draft: LocalModDraft,
  archivePath: string,
) {
  const field = (name: "title" | "author" | "version", label: string, placeholder: string) => `
    <label>${label}
      <input id="local-mod-${name}" spellcheck="false" value="${escapeHtml(draft[name])}" placeholder="${escapeHtml(placeholder)}" />
    </label>`;
  return `
    <dialog id="add-local-mod-dialog">
      <form method="dialog" class="dialog-card">
        <button class="close" value="cancel" data-action="cancel-local-mod" aria-label="Close">×</button>
        <p class="eyebrow">ADD A MOD FROM YOUR COMPUTER</p>
        <h2>${escapeHtml(inspection.title)}</h2>
        <div class="metadata">
          <div><small>BRANCH</small><strong>${escapeHtml(inspection.campaign)}</strong></div>
          <div><small>PACKAGE</small><strong>${formatBytes(inspection.size)}</strong></div>
          <div><small>FILES</small><strong>${inspection.files}</strong></div>
        </div>
        ${field("title", "Name", "How this mod should appear in your list")}
        ${field("author", "Author", "Leave empty if the archive does not say")}
        ${field("version", "Version", "Leave empty if the archive does not say")}
        <p class="dialog-note">
          CCM read <code>metadata.txt</code> inside the archive and will install this mod into
          <strong>${escapeHtml(inspection.targetPath)}</strong>. It keeps its own copy of
          <code>${escapeHtml(archivePath)}</code>, so the mod keeps working after you move or delete that file.
          The SHA-256 below is taken from your file: it protects the copy from changing silently,
          it does not vouch for where the mod came from.
        </p>
        <p class="dialog-hash"><small>SHA-256 FROM YOUR FILE</small><code>${escapeHtml(inspection.sha256)}</code></p>
        <p class="dialog-note">Adding changes nothing in StarCraft II. You choose Install afterwards.</p>
        <div class="dialog-actions">
          <button class="ghost" value="cancel" data-action="cancel-local-mod">Cancel</button>
          <button class="primary" value="default" data-action="confirm-local-mod">Add to my list</button>
        </div>
      </form>
    </dialog>`;
}
