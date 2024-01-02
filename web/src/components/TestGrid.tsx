import { FC, MutableRefObject, useEffect, useState } from 'react';

import { launchpad_mini_mk1 } from '../controllers/launchpad_mini_mk1';
import { launchpad_mini_mk3 } from '../controllers/launchpad_mini_mk3';
import { WSMesageAny } from '../types/WSMessage';

const decodeXY = (index: number) => {
    const x = index % 9;
    const y = Math.floor(index / 9);

    return { x, y };
};

export const TestGrid: FC<{
    // eslint-disable-next-line no-undef
    ws: MutableRefObject<WebSocket>;
    device_type: string;
}> = ({ ws, device_type }) => {
    const controller = {
        'Launchpad Mini Mk3': launchpad_mini_mk3,
        'Launchpad Mini Mk1': launchpad_mini_mk1,
    }[device_type];

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
                            className={
                                grid?.[y]?.[x]
                                    ? 'bg-red-500'
                                    : controller?.getCustomCSS(x, y)
                            }
                        ></div>
                    );
                })}
            </div>
        </div>
    );
};
