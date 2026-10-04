import { createRouter, createWebHashHistory } from "vue-router";
import Overview from "./views/Overview.vue";
import Profiles from "./views/Profiles.vue";
import Templates from "./views/Templates.vue";
import Proxies from "./views/Proxies.vue";
import Resources from "./views/Resources.vue";
import Connections from "./views/Connections.vue";
import Logs from "./views/Logs.vue";
import Settings from "./views/Settings.vue";
import Service from "./views/Service.vue";
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/overview" },
    { path: "/overview", component: Overview },
    { path: "/profiles", component: Profiles },
    { path: "/templates", component: Templates },
    { path: "/proxies", component: Proxies },
    { path: "/providers", component: Resources, props: { kind: "providers" } },
    { path: "/rules", component: Resources, props: { kind: "rules" } },
    { path: "/connections", component: Connections },
    { path: "/logs", component: Logs },
    { path: "/settings", component: Settings },
    { path: "/service", component: Service },
    { path: "/:pathMatch(.*)*", redirect: "/overview" },
  ],
});
