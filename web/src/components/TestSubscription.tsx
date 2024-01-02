import { FC, useEffect, useRef } from 'react';

import { TestGrid } from './TestGrid';

export const TestSubscription: FC<{
    device_id: string;
    device_type: string;
}> = ({ device_id, device_type }) => {
    const connection = useRef();

    useEffect(() => {
        // eslint-disable-next-line no-undef
        const eventSource = new WebSocket(
            'ws://localhost:3000/events/' + device_id
        );

        eventSource.addEventListener('message', (event) => {
            if (event.data) {
                const data = JSON.parse(event.data);

                console.log(data);
            } else {
                console.log({ e: event });
            }
        });

        // @ts-ignore
        connection.current = eventSource;

        return () => {
            eventSource.close();
        };
    }, []);

    return (
        <div className="space-y-2">
            <div className="border p-2 rounded-md">{device_type}</div>
            <TestGrid ws={connection as any} device_type={device_type} />
        </div>
    );
};
