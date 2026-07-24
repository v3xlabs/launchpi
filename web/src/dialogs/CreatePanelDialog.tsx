import * as Dialog from '@kobalte/core/dialog';
import { useNavigate } from '@tanstack/solid-router';
import { Component, createSignal, For, JSX } from 'solid-js';
import { TbFillCircleX as TbX } from 'solid-icons/tb';

import { Capabilities, capabilityLabels, emptyCapabilities } from '../api/inventory';
import { useInventory } from '../context/InventoryContext';

export const CreatePanelDialog: Component<{ trigger: JSX.Element }> = (props) => {
    const store = useInventory();
    const navigate = useNavigate();
    const [isOpen, setIsOpen] = createSignal(false);
    const [name, setName] = createSignal('');
    const [columns, setColumns] = createSignal('4');
    const [rows, setRows] = createSignal('3');
    const [capabilities, setCapabilities] = createSignal<Capabilities>(emptyCapabilities);

    const reset = () => {
        setName('');
        setColumns('4');
        setRows('3');
        setCapabilities(emptyCapabilities);
    };

    const submit = async (event: SubmitEvent) => {
        event.preventDefault();
        const panel = await store.createPanel({
            name: name().trim(),
            layout: { columns: Number(columns()), rows: Number(rows()) },
            capabilities: capabilities(),
            controls: [],
        });
        if (panel) {
            reset();
            setIsOpen(false);
            navigate({ to: '/panels/$panelId', params: { panelId: panel.panel_id } });
        }
    };

    return (
        <Dialog.Root open={isOpen()} onOpenChange={setIsOpen}>
            <Dialog.Trigger as="div" class="contents">
                {props.trigger}
            </Dialog.Trigger>
            <Dialog.Portal>
                <Dialog.Overlay class="dialog-overlay" />
                <div class="dialog-positioner">
                    <Dialog.Content class="dialog-content">
                        <div class="dialog-heading">
                            <div>
                                <Dialog.Title class="text-lg font-semibold">Create panel</Dialog.Title>
                                <Dialog.Description class="mt-1 text-sm text-neutral-400">
                                    Define the grid and the capabilities a device must support.
                                </Dialog.Description>
                            </div>
                            <Dialog.CloseButton class="icon-button" aria-label="Close create panel dialog">
                                <TbX />
                            </Dialog.CloseButton>
                        </div>
                        <form class="mt-6 grid gap-4" onSubmit={submit}>
                            <label class="field-label">
                                Panel name
                                <input
                                    class="field-input"
                                    value={name()}
                                    onInput={(event) => setName(event.currentTarget.value)}
                                    placeholder="Playback"
                                    required
                                />
                            </label>
                            <div class="grid grid-cols-2 gap-4">
                                <label class="field-label">
                                    Columns
                                    <input
                                        class="field-input"
                                        type="number"
                                        min="1"
                                        value={columns()}
                                        onInput={(event) => setColumns(event.currentTarget.value)}
                                        required
                                    />
                                </label>
                                <label class="field-label">
                                    Rows
                                    <input
                                        class="field-input"
                                        type="number"
                                        min="1"
                                        value={rows()}
                                        onInput={(event) => setRows(event.currentTarget.value)}
                                        required
                                    />
                                </label>
                            </div>
                            <fieldset>
                                <legend class="field-label">Required capabilities</legend>
                                <div class="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-3">
                                    <For each={capabilityLabels}>
                                        {({ key, label }) => (
                                            <label class="capability-toggle">
                                                <input
                                                    type="checkbox"
                                                    checked={capabilities()[key]}
                                                    onInput={(event) =>
                                                        setCapabilities((current) => ({
                                                            ...current,
                                                            [key]: event.currentTarget.checked,
                                                        }))
                                                    }
                                                />
                                                {label}
                                            </label>
                                        )}
                                    </For>
                                </div>
                            </fieldset>
                            <div class="dialog-actions">
                                <Dialog.CloseButton class="secondary-button" type="button">
                                    Cancel
                                </Dialog.CloseButton>
                                <button class="primary-button" type="submit" disabled={store.isSaving()}>
                                    {store.isSaving() ? 'Creating…' : 'Create panel'}
                                </button>
                            </div>
                        </form>
                    </Dialog.Content>
                </div>
            </Dialog.Portal>
        </Dialog.Root>
    );
};
