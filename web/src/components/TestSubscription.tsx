import { useEffect, useRef } from 'react';

import { TestGrid } from './TestGrid';

export const TestSubscription = () => {
    const connection = useRef();

    useEffect(() => {
        // eslint-disable-next-line no-undef
        const eventSource = new WebSocket('ws://localhost:3000/events');

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
        <div>
            <TestGrid ws={connection as any} />
        </div>
    );
};
