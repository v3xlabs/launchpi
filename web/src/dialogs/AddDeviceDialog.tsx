import * as Dialog from '@kobalte/core/dialog';
import { Component, createSignal, For, JSX, Show } from 'solid-js';
import { TbFillCircleX as TbX } from 'solid-icons/tb';

import { DeviceKind, deviceKindLabels } from '../api/inventory';
import { DeviceImage } from '../components/DeviceImage';
import { useInventory } from '../context/InventoryContext';

export const AddDeviceDialog: Component<{ trigger: JSX.Element }> = (props) => {
    const store = useInventory();
    const [isOpen, setIsOpen] = createSignal(false);
    const [name, setName] = createSignal('');
    const [kind, setKind] = createSignal<DeviceKind>('studio');
    const [host, setHost] = createSignal('');
    const [port, setPort] = createSignal('5343');

    const reset = () => {
        setName('');
        setKind('studio');
        setHost('');
        setPort('5343');
    };

    const submit = async (event: SubmitEvent) => {
        event.preventDefault();
        const parsedPort = Number(port());
        const created = await store.addDevice({
            name: name().trim() || 'Network device',
            host: host().trim(),
            port: Number.isInteger(parsedPort) && parsedPort > 0 ? parsedPort : undefined,
            serial_number: null,
            kind: kind(),
        });
        if (created) {
            reset();
            setIsOpen(false);
        }
    };

    const activeHint = () => deviceKindLabels.find((entry) => entry.value === kind())?.hint;

    return (
        <Dialog.Root open={isOpen()} onOpenChange={setIsOpen}>
            <Dialog.Trigger as="div" class="contents">
                {props.trigger}
            </Dialog.Trigger>
            <Dialog.Portal>
                <Dialog.Overlay class="dialog-overlay" />
                <div class="dialog-positioner">
                    <Dialog.Content class="dialog-content">
                        <div class="dialog-head">
                            <div>
                                <Dialog.Title class="dialog-title">Add device by address</Dialog.Title>
                                <Dialog.Description class="dialog-description">
                                    Connect a device that discovery cannot reach — for example a dock on another
                                    subnet.
                                </Dialog.Description>
                            </div>
                            <Dialog.CloseButton class="icon-button" aria-label="Close add device dialog">
                                <TbX class="h-4 w-4" />
                            </Dialog.CloseButton>
                        </div>
                        <form onSubmit={submit}>
                            <div class="dialog-body">
                                <label class="field-label">
                                    Name
                                    <input
                                        class="field-input"
                                        value={name()}
                                        onInput={(event) => setName(event.currentTarget.value)}
                                        placeholder="Control room dock"
                                    />
                                </label>
                                <label class="field-label">
                                    Device type
                                    <select
                                        class="field-input"
                                        value={kind()}
                                        onChange={(event) => setKind(event.currentTarget.value as DeviceKind)}
                                    >
                                        <For each={deviceKindLabels}>
                                            {(entry) => <option value={entry.value}>{entry.label}</option>}
                                        </For>
                                    </select>
                                </label>
                                <Show when={activeHint()}>
                                    {(hint) => (
                                        <div class="flex items-center gap-3">
                                            <DeviceImage
                                                model={
                                                    deviceKindLabels.find((entry) => entry.value === kind())
                                                        ?.label ?? ''
                                                }
                                                class="h-10 w-16"
                                            />
                                            <p class="hint">{hint()}</p>
                                        </div>
                                    )}
                                </Show>
                                <div class="grid grid-cols-[minmax(0,1fr)_7rem] gap-3">
                                    <label class="field-label">
                                        Host
                                        <input
                                            class="field-input"
                                            value={host()}
                                            onInput={(event) => setHost(event.currentTarget.value)}
                                            placeholder="192.168.1.42"
                                            required
                                        />
                                    </label>
                                    <label class="field-label">
                                        Port
                                        <input
                                            class="field-input"
                                            value={port()}
                                            onInput={(event) => setPort(event.currentTarget.value)}
                                            inputMode="numeric"
                                        />
                                    </label>
                                </div>
                            </div>
                            <div class="dialog-actions">
                                <Dialog.CloseButton class="secondary-button" type="button">
                                    Cancel
                                </Dialog.CloseButton>
                                <button class="primary-button" type="submit" disabled={store.isSaving()}>
                                    {store.isSaving() ? 'Adding…' : 'Add device'}
                                </button>
                            </div>
                        </form>
                    </Dialog.Content>
                </div>
            </Dialog.Portal>
        </Dialog.Root>
    );
};
