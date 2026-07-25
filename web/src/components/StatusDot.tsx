import { Component } from 'solid-js';

import { DeviceStatus } from '../api/inventory';

const statusClass: Record<DeviceStatus, string> = {
    connected: 'bg-emerald-500',
    connecting: 'bg-amber-500',
    unavailable: 'bg-rose-500',
    disabled: 'bg-neutral-600',
};

export const StatusDot: Component<{ status: DeviceStatus; class?: string }> = (props) => (
    <span
        classList={{
            'status-dot': true,
            'h-2 w-2': props.class === undefined,
            [statusClass[props.status]]: true,
            [props.class ?? '']: true,
        }}
        aria-hidden="true"
    />
);

export const StatusLabel: Component<{ status: DeviceStatus }> = (props) => (
    <span class="status-label">
        <StatusDot status={props.status} />
        {props.status}
    </span>
);
