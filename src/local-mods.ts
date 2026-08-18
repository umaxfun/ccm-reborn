import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Campaign, Catalog, LocalModEntry, LocalPackageInspection } from "./types";
import { renderAddLocalDialog } from "./render-add-local-dialog";

export type LocalModsContext = {
  busy: () => boolean;
  setBusy: (value: boolean) => void;
  setMessage: (value: string, kind: "neutral" | "success" | "error") => void;
  setHighlight: (campaignId: string) => void;
  // Re-reads the local list, rebuilds the merged catalog and re-inspects the
  // game directory, so a freshly added mod is immediately installable.
  reload: () => Promise<void>;
  render: () => void;
};

export type LocalModDraft = {
  title: string;
  author: string;
  version: string;
};

let mods: LocalModEntry[] = [];
let pendingPath = "";
let pendingInspection: LocalPackageInspection | null = null;
let draft: LocalModDraft = { title: "", author: "", version: "" };

export function localModEntries() {
  return mods;
}

export async function refreshLocalMods() {
  try {
    mods = await invoke<LocalModEntry[]>("list_local_mods");
  } catch {
    // A local list CCM cannot read must not stop the cloud catalogue from
    // rendering; the backend already recorded the reason in its log.
    mods = [];
  }
  return mods;
}

function localCampaign(mod: LocalModEntry): Campaign {
  return {
    id: mod.id,
    title: mod.title,
    author: mod.author || "Unknown author",
    version: mod.version || "unversioned",
    description: mod.description
      || "You added this mod from your computer. CCM keeps its own copy of the archive.",
    tags: [],
    requirements: { campaign: mod.campaign },
    package: { source: mod.archivePath, sha256: mod.sha256, size: mod.size },
    isLocal: true,
    addedAt: mod.addedAt,
  };
}

/// Cloud and local entries live in one list. Local entries are read from disk,
/// so they stay visible when the cloud catalogue cannot be reached.
export function mergeCatalog(cloud: Catalog | null, entries: LocalModEntry[]): Catalog | null {
  const local = entries.map(localCampaign);
  if (!cloud) {
    return local.length
      ? { name: "Your local mods", updatedAt: "", sourceKind: "local", campaigns: local }
      : null;
  }
  const known = new Set(cloud.campaigns.map((campaign) => campaign.id));
  return { ...cloud, campaigns: [...cloud.campaigns, ...local.filter((campaign) => !known.has(campaign.id))] };
}

export function localModDialog() {
  return pendingInspection ? renderAddLocalDialog(pendingInspection, draft, pendingPath) : "";
}

export function closeLocalModDialog(context: LocalModsContext) {
  pendingInspection = null;
  pendingPath = "";
  context.render();
}

async function chooseLocalPackage(context: LocalModsContext) {
  const selected = await open({
    multiple: false,
    title: "Choose a CCM mod archive",
    filters: [{ name: "CCM package", extensions: ["zip"] }],
  });
  if (typeof selected !== "string") return;
  context.setBusy(true);
  context.setMessage("Reading the archive…", "neutral");
  context.render();
  try {
    const inspection = await invoke<LocalPackageInspection>("inspect_local_package", { path: selected });
    pendingInspection = inspection;
    pendingPath = selected;
    draft = { title: inspection.title, author: inspection.author, version: inspection.version };
    context.setMessage("", "neutral");
  } catch (error) {
    pendingInspection = null;
    pendingPath = "";
    context.setMessage(String(error), "error");
  } finally {
    context.setBusy(false);
    context.render();
  }
}

async function confirmAddLocalMod(context: LocalModsContext) {
  if (!pendingInspection || !pendingPath) return;
  const path = pendingPath;
  const overrides = {
    title: draft.title.trim(),
    author: draft.author.trim(),
    version: draft.version.trim(),
  };
  pendingInspection = null;
  pendingPath = "";
  context.setBusy(true);
  context.setMessage("Adding the mod to your list…", "neutral");
  context.render();
  try {
    const entry = await invoke<LocalModEntry>("add_local_mod", { path, overrides });
    await context.reload();
    context.setHighlight(entry.id);
    // The wash only has to catch the eye once; a permanent marker would just
    // become decoration on a row the player already found.
    window.setTimeout(() => {
      context.setHighlight("");
      context.render();
    }, 4000);
    context.setMessage(
      `${entry.title} was added to ${entry.campaign}. Nothing in StarCraft II changed yet — choose Install when you want to play it.`,
      "success",
    );
  } catch (error) {
    context.setMessage(String(error), "error");
  } finally {
    context.setBusy(false);
    context.render();
  }
}

async function removeLocalMod(context: LocalModsContext, campaign: Campaign, installed: boolean) {
  const warning = installed
    ? " It is installed in StarCraft II right now: that campaign stays exactly as it is, but Repair will not work until you add the archive again."
    : "";
  if (!window.confirm(`Remove ${campaign.title} from your list? CCM deletes its own copy of the archive and nothing else.${warning}`)) {
    return;
  }
  context.setBusy(true);
  context.render();
  try {
    await invoke<boolean>("remove_local_mod", { id: campaign.id });
    await context.reload();
    context.setMessage(`${campaign.title} was removed from your list.`, "success");
  } catch (error) {
    context.setMessage(String(error), "error");
  } finally {
    context.setBusy(false);
    context.render();
  }
}

export function localModsHeaderButton(busy: boolean) {
  return `<button class="ghost compact" data-action="add-local-mod" ${busy ? "disabled" : ""}>Add local mod…</button>`;
}

export function bindLocalModEvents(context: LocalModsContext, campaigns: Campaign[], installedIds: string[]) {
  document.querySelector<HTMLButtonElement>("[data-action='add-local-mod']")
    ?.addEventListener("click", () => void chooseLocalPackage(context));
  document.querySelector<HTMLButtonElement>("[data-action='cancel-local-mod']")
    ?.addEventListener("click", (event) => {
      event.preventDefault();
      closeLocalModDialog(context);
    });
  document.querySelector<HTMLButtonElement>("[data-action='confirm-local-mod']")
    ?.addEventListener("click", (event) => {
      event.preventDefault();
      void confirmAddLocalMod(context);
    });
  (["title", "author", "version"] as const).forEach((field) => {
    const input = document.querySelector<HTMLInputElement>(`#local-mod-${field}`);
    input?.addEventListener("input", () => {
      draft = { ...draft, [field]: input.value };
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action='remove-local-mod']").forEach((button) => {
    button.addEventListener("click", () => {
      const campaign = campaigns.find((item) => item.id === button.dataset.campaign);
      if (campaign) void removeLocalMod(context, campaign, installedIds.includes(campaign.id));
    });
  });
}
