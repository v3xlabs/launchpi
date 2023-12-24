import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
    plugins: [react()],
    define: { global: 'globalThis' },
    build: {
        rollupOptions: {},
        commonjsOptions: {
            transformMixedEsModules: true,
        },
    },
});
