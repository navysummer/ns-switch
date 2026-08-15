import { defineStore } from "pinia";
import { api } from "@/utils/api";
import type { Provider, ProviderInput, ToolInfo } from "@/types";

export const useProvidersStore = defineStore("providers", {
  state: () => ({
    tools: [] as ToolInfo[],
    providersByTool: {} as Record<string, Provider[]>,
    presets: [] as Provider[],
    activeToolId: "claude_code",
    loading: false,
  }),
  getters: {
    visibleTools: (s) => s.tools.filter((t) => t.visible),
    activeProviders(): Provider[] {
      return this.providersByTool[this.activeToolId] ?? [];
    },
    activeProvider(): Provider | undefined {
      return this.activeProviders.find((p) => p.isActive);
    },
  },
  actions: {
    async loadTools() {
      this.tools = await api.getTools();
    },
    async setToolVisible(toolId: string, visible: boolean) {
      await api.setToolVisible(toolId, visible);
      const t = this.tools.find((x) => x.id === toolId);
      if (t) t.visible = visible;
    },
    async loadProviders(toolId?: string) {
      const id = toolId ?? this.activeToolId;
      this.loading = true;
      try {
        this.providersByTool[id] = await api.getProviders(id);
      } finally {
        this.loading = false;
      }
    },
    async selectTool(id: string) {
      this.activeToolId = id;
      if (!this.providersByTool[id]) {
        this.providersByTool[id] = await api.getProviders(id);
      }
    },
    async loadPresets(toolId: string) {
      this.presets = await api.getPresets(toolId);
    },
    async create(input: ProviderInput) {
      const p = await api.createProvider(input);
      this.providersByTool[input.toolId] = await api.getProviders(input.toolId);
      return p;
    },
    async update(input: ProviderInput) {
      const p = await api.updateProvider(input);
      await this.loadProviders(input.toolId);
      return p;
    },
    async remove(toolId: string, id: number) {
      await api.deleteProvider(id);
      await this.loadProviders(toolId);
    },
    async switchProvider(toolId: string, providerId: number) {
      const p = await api.switchProvider(toolId, providerId);
      await this.loadProviders(toolId);
      return p;
    },
    async importData(data: string) {
      const n = await api.importProviders(data);
      await this.refreshAll();
      return n;
    },
    async refreshAll() {
      const ids = Object.keys(this.providersByTool);
      for (const id of ids) await this.loadProviders(id);
    },
  },
});