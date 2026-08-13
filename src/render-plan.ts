import type { BankPlan, DryRunPlan, FileChangePlan, ProgressFilePlan } from "./types";

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

const formatBytes = (bytes: number) => bytes < 1024 * 1024
  ? `${Math.max(1, Math.round(bytes / 1024))} KB`
  : `${(bytes / 1024 / 1024).toFixed(bytes > 1024 * 1024 * 1024 ? 1 : 0)} MB`;

function groupByAction<T>(items: T[], getAction: (item: T) => string) {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const action = getAction(item);
    const group = groups.get(action) ?? [];
    group.push(item);
    groups.set(action, group);
  }
  return [...groups.entries()];
}

export function renderPlanDialog(plan: DryRunPlan, profileDir: string, busy: boolean) {
  const renderFileChange = (change: FileChangePlan) => `
    <article class="plan-change-row">
      <div class="plan-change-operation"><small>${escapeHtml(change.kind)}</small><strong>${escapeHtml(change.operation)}</strong></div>
      <div><small>FROM</small><code>${escapeHtml(change.source)}</code></div>
      <div><small>TO</small><code>${escapeHtml(change.destination)}</code></div>
      <div class="plan-change-meta"><small>SIZE / HASH</small><span>${formatBytes(change.size)}${change.sha256 ? ` · ${escapeHtml(change.sha256.slice(0, 12))}…` : ""}</span></div>
    </article>`;
  const renderProfileChange = (file: ProgressFilePlan) => `
    <article class="plan-change-row profile-change-row">
      <div class="plan-change-operation"><small>${escapeHtml(file.kind)}</small><strong>${escapeHtml(file.action)}</strong></div>
      <div><small>FROM</small><code>${escapeHtml(file.source)}</code></div>
      <div><small>TO</small><code>${escapeHtml(file.destination)}</code></div>
      <div class="plan-change-meta"><small>SIZE / HASH</small><span>${formatBytes(file.size)} · ${escapeHtml(file.sha256.slice(0, 12))}…</span>${file.detail ? `<em>${escapeHtml(file.detail)}</em>` : ""}</div>
    </article>`;
  const fileGroups = groupByAction(plan.fileChanges, (change) => change.operation).map(([action, changes]) => `
    <details class="plan-group"><summary><span>${escapeHtml(action)}</span><strong>${changes.length}</strong></summary>
      <div class="plan-change-list">${changes.map(renderFileChange).join("")}</div></details>`).join("");
  const profileGroups = groupByAction(plan.progressFiles, (file) => file.action).map(([action, files]) => `
    <details class="plan-group"><summary><span>${escapeHtml(action)}</span><strong>${files.length}</strong></summary>
      <div class="plan-change-list">${files.map(renderProfileChange).join("")}</div></details>`).join("");
  const bankPlans = plan.bankPlans.map((bank: BankPlan) => `
    <article class="plan-bank-row"><div><small>BANK</small><code>${escapeHtml(bank.relativePath)}</code></div>
      <div><small>FROM</small><code>${escapeHtml(bank.source)}</code></div><div><small>TO</small><code>${escapeHtml(bank.destination)}</code></div>
      <strong>${bank.sections} sections · ${bank.keys} keys · ${bank.keysChangedInPlace} keys changed in place</strong><p>${escapeHtml(bank.note)}</p></article>`).join("");
  const progressKeys = plan.progressKeys.map((key) => `
    <article class="plan-key-row"><code>${escapeHtml(key.key)}</code><span>${escapeHtml(key.currentValue)} → ${escapeHtml(key.plannedValue)}</span><small>${escapeHtml(key.action)}</small></article>`).join("");
  return `
    <dialog id="plan-dialog"><section class="dialog-card plan-card"><button class="close" data-action="close-plan" aria-label="Close">×</button><div class="plan-scroll">
      <p class="eyebrow">DRY-RUN · NO GAME FILES CHANGED</p><h2>Review ${escapeHtml(plan.title)}</h2>
      <p class="plan-subtitle">Operation ${escapeHtml(plan.operationId)} · target <code>${escapeHtml(plan.targetPath)}</code> · game <code>${escapeHtml(plan.gameDirectory)}</code></p>
      <div class="plan-stats"><div><small>PACKAGE</small><strong>${plan.packageFiles} files · ${formatBytes(plan.packageBytes)}</strong></div><div><small>WOULD CLEAR</small><strong>${plan.campaignFilesToClear} files · ${formatBytes(plan.campaignBytesToClear)}</strong></div><div><small>WOULD BACK UP</small><strong>${plan.filesToBackup} files</strong></div><div><small>ARCHIVE</small><strong>${formatBytes(plan.archiveSize)} · ${escapeHtml(plan.archiveSha256.slice(0, 12))}…</strong></div><div><small>UPDATE MODE</small><strong>${escapeHtml(plan.updateKind)} · ${plan.previousInstallFiles} old files</strong></div><div><small>SAVE PROFILE</small><strong>${plan.profileFilesToSnapshot} files · ${formatBytes(plan.profileBytesToSnapshot)}</strong></div><div><small>RESTORE PROFILE</small><strong>${plan.profileFilesToRestore} files · ${formatBytes(plan.profileBytesToRestore)}</strong></div><div><small>PROGRESS UPDATE</small><strong>${plan.progressUpdates ? `${plan.progressUpdates} node · ${plan.progressKeys.length} keys` : "not detected"}</strong></div></div>
      <section class="plan-section plan-operation-section plan-update-section"><div class="plan-section-heading"><small>UPDATE SAFETY</small><span>old files are handled first</span></div>
        ${plan.previousInstallManifest ? `<p>Previous install manifest: <code>${escapeHtml(plan.previousInstallManifest)}</code></p>` : "<p>No previous installed manifest was found; this is a first install.</p>"}
        ${plan.previousInstallCampaignId ? `<p>Previous package: <code>${escapeHtml(plan.previousInstallCampaignId)}</code>${plan.previousInstallVersion ? ` · v${escapeHtml(plan.previousInstallVersion)}` : ""}${plan.previousInstallSha256 ? ` · ${escapeHtml(plan.previousInstallSha256)}` : ""}</p>` : ""}
        ${profileDir ? `<p>Selected profile: <code>${escapeHtml(profileDir)}</code></p>` : "<p>Choose an explicit StarCraft II profile in Sources before Apply is enabled.</p>"}<p>Before copying the new archive, CCM verifies the previous managed files and restores originals or removes only files recorded as package-owned.</p></section>
      <section class="plan-section plan-operation-section"><div class="plan-section-heading"><small>ALL FILE CHANGES · ${plan.fileChanges.length}</small><span>every row is included</span></div><p>Existing files are snapshotted before clear/replace. Package files are copied to the exact destination shown below.</p>${plan.dependencyRoots.length ? `<p>Dependency roots: ${plan.dependencyRoots.map((path) => `<code>${escapeHtml(path)}</code>`).join(" · ")}</p>` : ""}<div class="plan-group-list">${fileGroups || "<p>No game-file changes.</p>"}</div></section>
      <section class="plan-section plan-operation-section"><div class="plan-section-heading"><small>PROFILE FILE MOVES · ${plan.progressFiles.length}</small><span>${plan.profileStorePath ? `store: ${escapeHtml(plan.profileStorePath)}` : "no profile store"}</span></div>${plan.profilePath ? `<p>Current profile: <code>${escapeHtml(plan.profilePath)}</code></p>` : "<p>No profile files were discovered for this branch.</p>"}<div class="plan-group-list">${profileGroups || "<p>No save, bank, or progress files will be moved.</p>"}</div></section>
      <section class="plan-section plan-operation-section"><div class="plan-section-heading"><small>CAMPAIGN PROGRESS KEYS · ${plan.progressKeys.length}</small><span>exact XML attributes</span></div><div class="plan-key-list">${progressKeys || "<p>No target CampaignProgress.xml keys will be changed.</p>"}</div></section>
      <section class="plan-section plan-operation-section"><div class="plan-section-heading"><small>BANK INVENTORY · ${plan.bankPlans.length}</small><span>whole-file swap, no in-place key edits</span></div><div class="plan-bank-list">${bankPlans || "<p>No target campaign bank was found.</p>"}</div></section>
      <section class="plan-warnings">${plan.warnings.map((warning) => `<p>ⓘ ${escapeHtml(warning)}</p>`).join("")}</section></div>
      <div class="dialog-actions"><button class="ghost" data-action="close-plan">Cancel</button><button class="primary" data-action="apply-plan" ${!profileDir || busy ? "disabled" : ""}>Apply installation</button></div></section></dialog>`;
}
