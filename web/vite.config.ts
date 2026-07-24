import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

export default defineConfig({
    plugins: [solid()],
    define: { global: 'globalThis' },
    server: {
        proxy: {
            '/api': { target: 'http://localhost:3000', ws: true },
        },
    },
    build: {
        rollupOptions: {},
        commonjsOptions: {
            transformMixedEsModules: true,
        },
    },
});
