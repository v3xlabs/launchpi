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

        // @ts-expect-error
        connection.current = eventSource;

        return () => {
            eventSource.close();
        };
    }, []);

    return (
        <div className="space-y-2">
            <div className="border flex justify-between p-2 rounded-md">
                <div>{device_type}</div>

                <form
                    action="http://localhost:3000/connect"
                    method="GET"
                    onSubmit={(event) => {
                        // This is the best button implementation you will ever see
                        event.preventDefault();

                        fetch('http://localhost:3000/run/' + device_id);
                    }}
                >
                    <button className="border px-2 rounded-md">run</button>
                </form>
            </div>
            <TestGrid ws={connection as any} device_type={device_type} />
        </div>
    );
};
