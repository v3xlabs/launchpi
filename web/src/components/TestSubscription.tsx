import { useEffect, useState } from 'react';

export const TestSubscription = () => {
    const [] = useState([]);

    useEffect(() => {
        const eventSource = new EventSource('http://localhost:3000/events');

        eventSource.onmessage = (event) => {
            console.log(event);
        };

        return () => {
            eventSource.close();
        };
    }, []);

    return <div>hi</div>;
};
