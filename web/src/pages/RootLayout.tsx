import { Outlet } from '@tanstack/solid-router';
import { Component, Show } from 'solid-js';
import { TbFillCircleX as TbX } from 'solid-icons/tb';

import { Sidebar } from '../components/Sidebar';
import { useInventory } from '../context/InventoryContext';

export const RootLayout: Component = () => {
    const store = useInventory();

    return (
        <div class="min-h-screen bg-neutral-950 text-neutral-100 md:grid md:grid-cols-[17rem_minmax(0,1fr)]">
            <aside class="border-b border-neutral-800 md:sticky md:top-0 md:h-screen md:border-b-0 md:border-r">
                <Sidebar />
            </aside>
            <main class="min-w-0">
                <Show when={store.error()}>
                    {(message) => (
                        <div class="px-6 pt-6">
                            <div role="alert" class="alert">
                                <span>{message()}</span>
                                <button type="button" onClick={() => store.setError(null)} aria-label="Dismiss error">
                                    <TbX />
                                </button>
                            </div>
                        </div>
                    )}
                </Show>
                <Show when={store.isLoading()}>
                    <p class="px-6 pt-6 text-sm text-neutral-500">Loading workspace…</p>
                </Show>
                <Outlet />
            </main>
        </div>
    );
};
