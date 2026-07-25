import { Link, Outlet } from '@tanstack/solid-router';
import { Component, Show } from 'solid-js';
import {
    TbFillCircleX as TbX,
    TbFillExchange as TbRefresh,
    TbFillLayoutGrid as TbLayoutGrid,
} from 'solid-icons/tb';

import { Sidebar } from '../components/Sidebar';
import { useInventory } from '../context/InventoryContext';

export const RootLayout: Component = () => {
    const store = useInventory();

    return (
        <div class="app-shell">
            <header class="topbar">
                <Link to="/devices" class="brand">
                    <span class="brand-mark">
                        <TbLayoutGrid class="h-3.5 w-3.5" />
                    </span>
                    Launchpi
                </Link>
                <div class="ml-auto flex items-center gap-3">
                    <span class="connection">
                        <span
                            classList={{
                                'status-dot': true,
                                'h-2 w-2': true,
                                'bg-emerald-500': store.isConnected(),
                                'bg-amber-500': !store.isConnected(),
                            }}
                            aria-hidden="true"
                        />
                        {store.isConnected() ? 'Daemon online' : 'Reconnecting'}
                    </span>
                    <button
                        type="button"
                        class="icon-button"
                        onClick={() => void store.refresh()}
                        aria-label="Refresh inventory"
                        title="Refresh inventory"
                    >
                        <TbRefresh class="h-4 w-4" />
                    </button>
                </div>
            </header>

            <div class="workspace">
                <aside class="workspace-nav">
                    <Sidebar />
                </aside>
                <main class="min-w-0">
                    <Show when={store.error()}>
                        {(message) => (
                            <div class="px-4 pt-4 sm:px-6">
                                <div role="alert" class="alert">
                                    <span>{message()}</span>
                                    <button
                                        type="button"
                                        onClick={() => store.setError(null)}
                                        aria-label="Dismiss error"
                                    >
                                        <TbX class="h-4 w-4" />
                                    </button>
                                </div>
                            </div>
                        )}
                    </Show>
                    <Show when={store.isLoading()}>
                        <p class="px-4 pt-4 text-xs text-neutral-500 sm:px-6">Loading workspace…</p>
                    </Show>
                    <Outlet />
                </main>
            </div>
        </div>
    );
};
