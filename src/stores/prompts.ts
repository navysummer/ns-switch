import { defineStore } from "pinia";
import { api } from "@/utils/api";
import type { PromptItem } from "@/types";

export const usePromptsStore = defineStore("prompts", {
  state: () => ({
    list: [] as PromptItem[],
  }),
  actions: {
    async load() {
      this.list = await api.promptsList();
    },
    async save(input: { id?: number; name: string; content: string; fileType: string }) {
      await api.promptSave(input);
      await this.load();
    },
    async remove(id: number) {
      await api.promptDelete(id);
      await this.load();
    },
    async activate(id: number) {
      await api.promptActivate(id);
      await this.load();
    },
    async deactivate(id: number) {
      await api.promptDeactivate(id);
      await this.load();
    },
  },
});