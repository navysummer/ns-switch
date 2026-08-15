import { invoke } from "@tauri-apps/api/core";

export async function cmd<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return (await invoke(name, args)) as T;
  } catch (e) {
    const message = typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
    throw new Error(message);
  }
}

export const api = {
  getTools: () => cmd<import("@/types").ToolInfo[]>("get_tools"),
  setToolVisible: (toolId: string, visible: boolean) =>
    cmd<void>("set_tool_visible", { toolId, visible }),

  getProviders: (toolId: string) => cmd<import("@/types").Provider[]>("get_providers", { toolId }),
  getActiveProvider: (toolId: string) =>
    cmd<import("@/types").Provider | null>("get_active_provider", { toolId }),
  createProvider: (input: import("@/types").ProviderInput) =>
    cmd<import("@/types").Provider>("create_provider", { input }),
  updateProvider: (input: import("@/types").ProviderInput) =>
    cmd<import("@/types").Provider>("update_provider", { input }),
  deleteProvider: (id: number) => cmd<void>("delete_provider", { id }),
  duplicateProvider: (id: number) => cmd<import("@/types").Provider>("duplicate_provider", { id }),
  switchProvider: (toolId: string, providerId: number) =>
    cmd<import("@/types").Provider>("switch_provider", { toolId, providerId }),
  getPresets: (toolId: string) => cmd<import("@/types").Provider[]>("get_presets", { toolId }),
  importFromCli: (toolId: string) =>
    cmd<import("@/types").Provider>("import_from_cli", { toolId }),
  importAllFromCli: () => cmd<import("@/types").Provider[]>("import_all_from_cli"),
  onboardingStatus: () => cmd<boolean>("onboarding_status"),
  completeOnboarding: () => cmd<void>("complete_onboarding"),
  exportProviders: () => cmd<string>("export_providers"),
  importProviders: (data: string) => cmd<number>("import_providers", { data }),

  mcpList: () => cmd<import("@/types").McpServer[]>("mcp_list"),
  mcpSave: (input: import("@/types").McpServerInput) =>
    cmd<import("@/types").McpServer>("mcp_save", { input }),
  mcpDelete: (id: number) => cmd<void>("mcp_delete", { id }),
  mcpSync: () => cmd<Record<string, string>>("mcp_sync"),

  skillsList: () => cmd<import("@/types").SkillItem[]>("skills_list"),
  skillInstall: (source: string, mode: string = "symlink") =>
    cmd<import("@/types").SkillItem>("skill_install", { source, mode }),
  skillUninstall: (id: number) => cmd<void>("skill_uninstall", { id }),

  promptsList: () => cmd<import("@/types").PromptItem[]>("prompts_list"),
  promptSave: (input: {
    id?: number;
    name: string;
    content: string;
    fileType: string;
  }) => cmd<import("@/types").PromptItem>("prompt_save", { input }),
  promptDelete: (id: number) => cmd<void>("prompt_delete", { id }),
  promptActivate: (id: number) => cmd<void>("prompt_activate", { id }),
  promptDeactivate: (id: number) => cmd<void>("prompt_deactivate", { id }),

  getSettings: () => cmd<import("@/types").AppSettings>("get_settings"),
  setSettings: (settings: Partial<import("@/types").AppSettings>) =>
    cmd<import("@/types").AppSettings>("set_settings", { settings }),
  backupNow: () => cmd<import("@/types").BackupInfo>("backup_now"),
  listBackups: () => cmd<import("@/types").BackupInfo[]>("list_backups"),
  restoreBackup: (path: string) => cmd<void>("restore_backup", { path }),

  listSessions: (toolId: string) =>
    cmd<import("@/types").SessionItem[]>("list_sessions", { toolId }),
  openPath: (path: string) => cmd<void>("open_path", { path }),

  proxyStatuses: () => cmd<import("@/types").ProxyStatus[]>("proxy_statuses"),
  proxyStart: (toolId: string) => cmd<void>("proxy_start", { toolId }),
  proxyStop: (toolId: string) => cmd<void>("proxy_stop", { toolId }),
  reorderProviders: (toolId: string, ids: number[]) =>
    cmd<void>("reorder_providers", { toolId, ids }),
  resetToOfficial: (toolId: string) => cmd<void>("reset_to_official", { toolId }),
  getConfigSnapshot: (toolId: string) => cmd<string>("get_config_snapshot", { toolId }),

  skillSourcesList: () => cmd<import("@/types").SkillSource[]>("skill_sources_list"),
  skillSourceAdd: (name: string, url: string) =>
    cmd<import("@/types").SkillSource>("skill_source_add", { input: { name, url } }),
  skillSourceRemove: (id: number) => cmd<void>("skill_source_remove", { id }),

  usageOverview: () => cmd<import("@/types").UsageOverview>("usage_overview"),
  usageByDay: (days: number) => cmd<import("@/types").UsagePoint[]>("usage_by_day", { days }),
  usageLogs: (limit: number) => cmd<import("@/types").UsageLogItem[]>("usage_logs", { limit }),
  usageEvents: (limit: number) => cmd<Array<Record<string, unknown>>>("usage_events", { limit }),
  usageByModel: (limit: number) => cmd<import("@/types").UsageModelRow[]>("usage_by_model", { limit }),
  usagePricingGet: () => cmd<string>("usage_pricing_get"),
  usagePricingSet: (data: string) => cmd<void>("usage_pricing_set", { data }),
  usageScanSessions: () => cmd<number>("usage_scan_sessions"),
  usageReset: () => cmd<void>("usage_reset"),

  importDeepLinks: (text: string) =>
    cmd<import("@/types").DeepLinkResult>("import_deep_links", { text }),
  registerScheme: () => cmd<string>("register_scheme"),

  cloudGet: () => cmd<import("@/types").CloudSettings>("cloud_get"),
  cloudSet: (s: import("@/types").CloudSettings) => cmd<import("@/types").CloudSettings>("cloud_set", { settings: s }),
  cloudInfo: () => cmd<{ url: string; enabled: boolean; dataDir: string }>("cloud_info"),
  cloudTest: () => cmd<string>("cloud_test"),
  cloudPush: () => cmd<string>("cloud_push"),
  cloudPull: () => cmd<string>("cloud_pull"),
  cloudSetConfigDir: (dir: string) => cmd<string>("cloud_set_config_dir", { newDir: dir }),

  workspaceFiles: (dir?: string) => cmd<import("@/types").WorkspaceFile[]>("workspace_files", { dir }),
  workspaceRead: (path: string) => cmd<string>("workspace_read", { path }),
  workspaceSave: (path: string, content: string) => cmd<void>("workspace_save", { path, content }),

  checkUpdates: () => cmd<import("@/types").UpdateResult>("check_updates"),
  restoreSession: (path: string) => cmd<string>("restore_session", { path }),
  proxyHealth: (toolId: string) =>
    cmd<import("@/types").ProxyHealthItem[]>("proxy_health", { toolId }),
};


export function listen<T>(event: string, handler: (payload: T) => void): { unlisten: () => void } {
  let unlisten: (() => void) | undefined;
  void (async () => {
    unlisten = await import("@tauri-apps/api/event").then((m) =>
      m.listen<T>(event, (e) => handler(e.payload)),
    );
  })();
  return {
    unlisten() {
      unlisten?.();
    },
  };
}