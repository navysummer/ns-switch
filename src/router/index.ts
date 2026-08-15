import { createRouter, createWebHashHistory } from "vue-router";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/providers" },
    { path: "/providers", component: () => import("@/pages/Providers.vue") },
    { path: "/mcp", component: () => import("@/pages/Mcp.vue") },
    { path: "/skills", component: () => import("@/pages/Skills.vue") },
    { path: "/prompts", component: () => import("@/pages/Prompts.vue") },
    { path: "/sessions", component: () => import("@/pages/Sessions.vue") },
    { path: "/usage", component: () => import("@/pages/Usage.vue") },
    { path: "/workspace", component: () => import("@/pages/Workspace.vue") },
    { path: "/settings", component: () => import("@/pages/Settings.vue") },
  ],
});

export default router;