import { createRootRoute, createRoute, createRouter, redirect } from '@tanstack/solid-router';

import { DevicesPage } from './pages/DevicesPage';
import { PanelsPage } from './pages/PanelsPage';
import { RootLayout } from './pages/RootLayout';

const rootRoute = createRootRoute({ component: RootLayout });

const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/',
    beforeLoad: () => {
        throw redirect({ to: '/devices' });
    },
});

const devicesRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: 'devices',
    component: () => <DevicesPage />,
});

const deviceRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: 'devices/$surfaceId',
    component: () => {
        const params = deviceRoute.useParams();
        return <DevicesPage surfaceId={params().surfaceId} />;
    },
});

const panelsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: 'panels',
    component: () => <PanelsPage />,
});

const panelRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: 'panels/$panelId',
    component: () => {
        const params = panelRoute.useParams();
        return <PanelsPage panelId={params().panelId} />;
    },
});

const routeTree = rootRoute.addChildren([
    indexRoute,
    devicesRoute,
    deviceRoute,
    panelsRoute,
    panelRoute,
]);

export const router = createRouter({ routeTree });

declare module '@tanstack/solid-router' {
    interface Register {
        router: typeof router;
    }
}
