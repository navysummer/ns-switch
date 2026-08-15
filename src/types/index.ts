export interface ToolInfo {
  id: string;
  name: string;
  visible: boolean;
  hasConfig: boolean;
  configPath?: string;
}

export interface Provider {
  id: number;
  toolId: string;
  name: string;
  baseUrl: string;
  apiKey: string;
  model: string;
  headers: string;
  isPreset: boolean;
  isActive: boolean;
  isUniversal: boolean;
  sharedConfig: string;
  protocol: string;
}

export interface ProviderInput {
  id?: number;
  toolId: string;
  name: string;
  baseUrl: string;
  apiKey: string;
  model: string;
  headers: string;
  isUniversal?: boolean;
  sharedConfig?: string;
  protocol?: string;
}

export interface McpServer {
  id: number;
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  cwd: string;
  transport: string;
  isActive: boolean;
  tools: string[];
}

export interface McpServerInput {
  id?: number;
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  cwd: string;
  transport: string;
  isActive: boolean;
  tools: string[];
}

export interface SkillItem {
  id: number;
  name: string;
  description: string;
  source: string;
  status: string;
  installMode: string;
  installedAt: string;
}

export interface SkillSource {
  id?: number;
  name: string;
  url: string;
}

export interface UsageOverview {
  totalCost: number;
  todayCost: number;
  todayRequests: number;
  totalRequests: number;
  models: number;
}

export interface UsagePoint {
  day: string;
  cost: number;
  requests: number;
  inputTokens: number;
  outputTokens: number;
}

export interface UsageLogItem {
  id: number;
  toolId: string;
  providerId: number;
  providerName: string;
  model: string;
  day: string;
  requests: number;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
}

export interface UsageModelRow {
  model: string;
  requests: number;
  inputTokens: number;
  outputTokens: number;
  cost: number;
}

export interface WorkspaceFile {
  name: string;
  path: string;
  isDir: boolean;
}

export interface CloudSettings {
  url: string;
  username: string;
  password: string;
  enabled: boolean;
}

export interface UpdateResult {
  current: string;
  latest: string | null;
  hasUpdate: boolean;
  url: string | null;
}

export interface DeepLinkResult {
  action: string;
  count: number;
  message: string;
}

export interface ProxyHealthItem {
  providerId: number;
  name: string;
  ok: boolean;
  latencyMs: number;
  error?: string;
}

export interface PromptItem {
  id: number;
  name: string;
  content: string;
  fileType: string;
  isActive: boolean;
}

export interface SessionItem {
  id: string;
  toolId: string;
  path: string;
  cwd: string;
  title: string;
  updatedAt: string;
  messageCount: number;
}

export interface BackupInfo {
  path: string;
  name: string;
  created: string;
  size: number;
}

export interface ProxyStatus {
  toolId: string;
  running: boolean;
  port: number;
  activeProvider?: string;
  error?: string;
}

export interface AppSettings {
  theme: "light" | "dark" | "system";
  language: string;
  dataDir: string;
  autostart: boolean;
  proxyPorts: Record<string, number>;
  proxyEnabled: Record<string, boolean>;
  notifications: boolean;
  openaiProxyPath: string;
}