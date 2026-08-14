import { invoke } from "@tauri-apps/api/core";
import type { Campaign, Catalog, DryRunPlan, InstallProgress, InstallResult } from "./types";
import type { InstallFailure } from "./install-status";

type MessageKind = "neutral" | "success" | "error";
type InstallFlow = {
  catalog: () => Catalog | null;
  gameDir: () => string;
  profileDir: () => string;
  pendingPlan: () => DryRunPlan | null;
  activity: () => InstallProgress | null;
  setBusy: (value: boolean) => void;
  setPendingPlan: (value: DryRunPlan | null) => void;
  setActivity: (value: InstallProgress | null) => void;
  setCampaignId: (value: string) => void;
  setFailure: (value: InstallFailure | null) => void;
  setMessage: (value: string, kind: MessageKind) => void;
  inspectDirectory: () => Promise<void>;
  render: () => void;
};

const pendingActivity = (phase: string, message: string): InstallProgress => ({
  operationId: "", phase, message,
  downloadedBytes: null, totalBytes: null, completedFiles: null, totalFiles: null,
});

const requestFor = (campaign: Campaign, gameDir: string, profileDir: string | null) => ({
  campaignId: campaign.id, title: campaign.title, author: campaign.author, version: campaign.version,
  profileDir, archiveSource: campaign.package.source, sha256: campaign.package.sha256,
  packageSize: campaign.package.size, gameDir,
});

function recordFailure(flow: InstallFlow, phase: string, error: unknown) {
  const message = String(error);
  flow.setMessage(message, "error");
  flow.setFailure({ operationId: flow.activity()?.operationId ?? "", phase: flow.activity()?.phase ?? phase, error: message });
  flow.setActivity(null);
  flow.setCampaignId("");
}

export async function planCampaignInstall(flow: InstallFlow, campaignId: string) {
  const campaign = flow.catalog()?.campaigns.find((item) => item.id === campaignId);
  const gameDir = flow.gameDir();
  if (!campaign || !gameDir) return;
  const preparing = `Preparing a safe installation plan for ${campaign.title}…`;
  flow.setBusy(true); flow.setPendingPlan(null); flow.setCampaignId(campaign.id);
  flow.setActivity(pendingActivity("preparing-plan", preparing)); flow.setFailure(null); flow.setMessage(preparing, "neutral"); flow.render();
  try {
    const result = await invoke<DryRunPlan>("plan_campaign_install", { request: requestFor(campaign, gameDir, flow.profileDir() || null) });
    flow.setPendingPlan(result);
    flow.setMessage(`Dry-run complete for ${campaign.title}. No files were changed.`, "success");
  } catch (error) {
    recordFailure(flow, "preparing-plan", error);
  } finally {
    flow.setBusy(false); flow.render();
  }
}

export async function applyCampaignInstall(flow: InstallFlow) {
  const plan = flow.pendingPlan();
  const gameDir = flow.gameDir();
  const profileDir = flow.profileDir();
  if (!plan || !gameDir || !profileDir) return;
  const campaign = flow.catalog()?.campaigns.find((item) => item.id === plan.campaignId);
  if (!campaign || !window.confirm(`Apply ${campaign.title} v${campaign.version}? StarCraft II must be fully closed. CCM will use only the selected profile and keep rollback snapshots for both profile and campaign files.`)) return;
  const preparing = `Preparing ${campaign.title} for installation…`;
  flow.setBusy(true); flow.setPendingPlan(null); flow.setCampaignId(campaign.id);
  flow.setActivity(pendingActivity("preparing-install", preparing)); flow.setFailure(null); flow.setMessage(preparing, "neutral"); flow.render();
  try {
    const result = await invoke<InstallResult>("install_campaign", { request: requestFor(campaign, gameDir, profileDir) });
    flow.setMessage(`Installed ${result.title} v${result.version} (${result.filesInstalled} files).`, "success");
    flow.setActivity(null); flow.setCampaignId("");
    await flow.inspectDirectory();
  } catch (error) {
    recordFailure(flow, "preparing-install", error);
  } finally {
    flow.setBusy(false); flow.render();
  }
}
