import { defineStore } from "pinia";
import { api } from "@/utils/api";
import type { McpServer, McpServerInput } from "@/types";

export const useMcpStore = defineStore("mcp", {
  state: () => ({
    list: [] as McpServer[],
    loaded: false,
  }),
  actions: {
    async load() {
      this.list = await api.mcpList();
      this.loaded = true;
    },
    async save(input: McpServerInput) {
      await api.mcpSave(input);
      await this.load();
    },
    async remove(id: number) {
      await api.mcpDelete(id);
      await this.load();
    },
    async sync() {
      const r = await api.mcpSync();
      return r;
    },
  },
});