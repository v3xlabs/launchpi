import { FC, MutableRefObject, useEffect, useState } from 'react';

import { launchpad_mini_mk1 } from '../controllers/launchpad_mini_mk1';
import { launchpad_mini_mk3 } from '../controllers/launchpad_mini_mk3';
import { WSMesage } from '../types/WSMessage';

const decodeXY = (index: number) => {
    const x = index % 9;
    const y = Math.floor(index / 9);

    return { x, y };
};

enum Color {
    Black = 0,
    White,
    Red,
    Yellow,
    Blue,
    Magenta,
    Brown,
    Cyan,
    Green,
}

const colorToCss = (color: Color): string | undefined => {
    switch (color) {
        case Color.Black:
            return undefined;
        case Color.White:
            return 'bg-gray-300';
        case Color.Red:
            return 'bg-red-500';
        case Color.Yellow:
            return 'bg-yellow-400';
        case Color.Blue:
            return 'bg-blue-500';
        case Color.Magenta:
            return 'bg-purple-500';
        case Color.Brown:
            return 'bg-yellow-800';
        case Color.Cyan:
            return 'bg-cyan-400';
        case Color.Green:
            return 'bg-green-500';
    }
};

const init = (): Color[][] => {
    return Array.from({ length: 9 }).map(() =>
        Array.from({ length: 9 }).map(() => Color.Black)
    );
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
    const [grid, setGrid] = useState<Color[][]>(
        // eslint-disable-next-line no-undef
        init()
    );

    // eslint-disable-next-line sonarjs/cognitive-complexity
    useEffect(() => {
        if (ws.current) {
            console.log('hooked');
            const x = (event: MessageEvent<string>) => {
                if (event.data) {
                    const data: WSMesage = JSON.parse(event.data);

                    if (data.type == 'Press') {
                        // const { x, y } = data;
                        //
                        // const newGrid = [...grid];
                        //
                        // newGrid[y][x] = true;
                        //
                        // setGrid(newGrid);
                    }

                    if (data.type == 'Release') {
                        // const { x, y } = data;
                        //
                        // const newGrid = [...grid];
                        //
                        // newGrid[y][x] = false;
                        //
                        // setGrid(newGrid);
                    }

                    if (data.type == 'LightUpdate') {
                        const { updates } = data;

                        setGrid((oldGrid) => {
                            const newGrid = [...oldGrid];

                            for (const [x, y, palette] of updates) {
                                newGrid[y][x] = palette;
                            }

                            return newGrid;
                        });
                    }

                    if (data.type == 'ClearBoard') {
                        setGrid(init());
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
    }, [ws, ws.current, recheck, setGrid]);

    useEffect(() => {
        console.log('grid update', grid);
    }, [grid]);

    return (
        <div className="w-full p-4 border rounded-md bg-neutral-800">
            <div className="w-full grid-cols-9 grid gap-1 grid-rows-9">
                {[...Array.from({ length: 81 })].map((_, index) => {
                    const { x, y } = decodeXY(index);

                    return (
                        <div
                            key={index}
                            className={
                                grid?.[y]?.[x]
                                    ? `border ${colorToCss(grid[y][x])}`
                                    : controller?.getCustomCSS(x, y)
                            }
                        ></div>
                    );
                })}
            </div>
        </div>
    );
};
