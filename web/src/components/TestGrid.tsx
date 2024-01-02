import { FC, MutableRefObject, useEffect, useState } from 'react';

import { WSMesageAny } from '../types/WSMessage';

const decodeXY = (index: number) => {
    const x = index % 9;
    const y = Math.floor(index / 9);

    return { x, y };
};

const gridDefault = (index: number) => {
    const { x, y } = decodeXY(index);

    if (y === 0) return '#000';

    if (x === 8) return '#000';

    return '#fff';
};

export const TestGrid: FC<{
    // eslint-disable-next-line no-undef
    ws: MutableRefObject<WebSocket>;
}> = ({ ws }) => {
    const [recheck, setRecheck] = useState(0);
    const [grid, setGrid] = useState<boolean[][]>(
        // eslint-disable-next-line no-undef
        Array.from({ length: 9 }).map(() => Array.from({ length: 9 }))
    );

    useEffect(() => {
        if (ws.current) {
            console.log('hooked');
            const x = (event: MessageEvent<string>) => {
                if (event.data) {
                    const data: WSMesageAny = JSON.parse(event.data);

                    if (data.type == 'Press') {
                        const { x, y } = data;

                        const newGrid = [...grid];

                        newGrid[y][x] = true;

                        setGrid(newGrid);
                    }

                    if (data.type == 'Release') {
                        const { x, y } = data;

                        const newGrid = [...grid];

                        newGrid[y][x] = false;

                        setGrid(newGrid);
                    }
                } else {
                    console.log({ event });
                }
            };

            ws.current.addEventListener('message', x);

            return () => {
                console.log('unhooked');
                ws.current?.removeEventListener('message', x);
            };
        } else {
            console.log('no current yet');

            setTimeout(() => {
                setRecheck(Date.now());
            }, 100);
        }
    }, [ws, ws.current, recheck]);

    return (
        <div className="w-full p-4 border rounded-md">
            <div className="w-full grid-cols-9 grid gap-1 grid-rows-9">
                {[...Array.from({ length: 81 })].map((_, index) => {
                    const { x, y } = decodeXY(index);

                    return (
                        <div
                            key={index}
                            className="border aspect-square"
                            style={{
                                backgroundColor: grid?.[y]?.[x]
                                    ? '#f00'
                                    : gridDefault(index),
                            }}
                        ></div>
                    );
                })}
            </div>
        </div>
    );
};
