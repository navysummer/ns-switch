import { defineStore } from "pinia";
import { api } from "@/utils/api";
import type { SkillItem } from "@/types";

export const useSkillsStore = defineStore("skills", {
  state: () => ({
    list: [] as SkillItem[],
  }),
  actions: {
    async load() {
      this.list = await api.skillsList();
    },
    async install(source: string) {
      const s = await api.skillInstall(source);
      await this.load();
      return s;
    },
    async remove(id: number) {
      await api.skillUninstall(id);
      await this.load();
    },
  },
});