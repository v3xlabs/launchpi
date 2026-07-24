import { Component } from 'solid-js';

import { DeviceStatus } from '../api/inventory';

const statusClass: Record<DeviceStatus, string> = {
    connected: 'bg-emerald-400',
    connecting: 'bg-amber-400',
    unavailable: 'bg-rose-400',
    disabled: 'bg-neutral-500',
};

export const StatusDot: Component<{ status: DeviceStatus; class?: string }> = (props) => (
    <span
        classList={{ 'shrink-0': true, [statusClass[props.status]]: true, [props.class ?? '']: true }}
        aria-hidden="true"
    />
);
