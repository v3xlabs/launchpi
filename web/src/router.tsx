import { createRootRoute, createRoute, createRouter, redirect } from "@tanstack/solid-router";

import { DevicesPage } from "./pages/DevicesPage";
import { PanelsPage } from "./pages/PanelsPage";
import { PluginsPage } from "./pages/PluginsPage";
import { RootLayout } from "./pages/RootLayout";
import { SupportedDevicesPage } from "./pages/SupportedDevicesPage";

const rootRoute = createRootRoute({ component: RootLayout });

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/devices" });
  },
});

const devicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "devices",
  component: () => <DevicesPage />,
});

const supportedDevicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "devices/supported",
  component: () => <SupportedDevicesPage />,
});

const deviceRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "devices/$surfaceId",
  component: () => {
    const parameters = deviceRoute.useParams();

    return <DevicesPage surfaceId={parameters().surfaceId} />;
  },
});

const panelsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "panels",
  component: () => <PanelsPage />,
});

const panelRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "panels/$panelId",
  component: () => {
    const parameters = panelRoute.useParams();

    return <PanelsPage panelId={parameters().panelId} />;
  },
});

const pluginsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "plugins",
  component: () => <PluginsPage />,
});

const pluginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "plugins/$integrationId",
  component: () => {
    const parameters = pluginRoute.useParams();

    return <PluginsPage integrationId={parameters().integrationId} />;
  },
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  devicesRoute,
  supportedDevicesRoute,
  deviceRoute,
  panelsRoute,
  panelRoute,
  pluginsRoute,
  pluginRoute,
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/solid-router" {
  interface Register {
    router: typeof router;
  }
}
