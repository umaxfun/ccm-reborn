import type { StarcraftProfileCandidate } from "./types";
import { profileName } from "./resume";

type SettingsDialogContext = {
  catalogSourceDraft: string;
  gameDir: string;
  profileCandidates: StarcraftProfileCandidate[];
  profileDirDraft: string;
  showLocalDevCatalog: boolean;
};

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

function profileOptionLabel(path: string, index: number) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  const account = parts.at(-2);
  const label = profileName(path);
  return `${account ? `${account} / ` : ""}${label}${index === 0 ? " (recent)" : ""}`;
}

export function renderSettingsDialog(context: SettingsDialogContext) {
  const {
    catalogSourceDraft,
    gameDir,
    profileCandidates,
    profileDirDraft,
    showLocalDevCatalog,
  } = context;
  return `
    <dialog id="settings-dialog">
      <form method="dialog" class="dialog-card">
        <button class="close" value="cancel" aria-label="Close">×</button>
        <p class="eyebrow">SOURCES</p><h2>Catalog & game location</h2>
        <label>Catalog source
          <input id="catalog-source" spellcheck="false" value="${escapeHtml(catalogSourceDraft)}" placeholder="/path/to/catalog.json or https://…/catalog.json" />
          <small>Releases use the community cloud catalog by default. You can choose another HTTPS catalog or a local development catalog here.</small>
        </label>
        <div class="directory-actions"><button type="button" class="ghost" data-action="use-cloud-catalog">Use community cloud</button>${showLocalDevCatalog ? '<button type="button" class="ghost" data-action="use-local-catalog">Use local dev catalog</button>' : ""}</div>
        <section class="detected-directory">
          <small>STARCRAFT II DIRECTORY</small>
          <strong>${gameDir ? escapeHtml(gameDir) : "No installation detected"}</strong>
          <span>Auto-detection runs when CCM Reborn starts. Choose a folder only if it missed your installation.</span>
        </section>
        <div class="directory-actions">
          <button type="button" class="ghost" data-action="choose-directory">Choose folder…</button>
          <button type="button" class="ghost" data-action="detect-directory">Detect again</button>
        </div>
        <label>StarCraft II profile
          <select id="profile-directory">
            <option value="">Choose before applying an install…</option>
            ${profileCandidates.map((profile, index) => `<option value="${escapeHtml(profile.path)}" ${profile.path === profileDirDraft ? "selected" : ""}>${escapeHtml(profileOptionLabel(profile.path, index))}</option>`).join("")}
          </select>
          <small>This is the single account profile whose bank, saves, and campaign progress CCM may switch. The first detected profile is selected automatically; choose the other one only if that is the account you play on.</small>
          ${profileDirDraft ? `<small class="profile-path">${escapeHtml(profileDirDraft)}</small>` : ""}
        </label>
        <div class="directory-actions">
          <button type="button" class="ghost" data-action="choose-profile">Choose profile…</button>
          <button type="button" class="ghost" data-action="detect-profiles">Find profiles</button>
        </div>
        <div class="dialog-actions">
          <button class="ghost" value="cancel">Cancel</button>
          <button class="primary" value="default" data-action="save-settings">Reload catalog</button>
        </div>
      </form>
    </dialog>`;
}
