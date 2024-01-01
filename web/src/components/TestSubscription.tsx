import { useEffect, useRef } from 'react';

export const TestSubscription = () => {
    const connection = useRef(null);

    useEffect(() => {
        // eslint-disable-next-line no-undef
        const eventSource = new WebSocket('ws://localhost:3000/events');

        eventSource.addEventListener('message', (e) => {
            if (e.data) {
                const data = JSON.parse(e.data);

                console.log(data);
            } else {
                console.log({ e });
            }
        });

        // @ts-ignore
        connection.current = eventSource;

        return () => {
            eventSource.close();
        };
    }, []);

    return <div>hi</div>;
};
