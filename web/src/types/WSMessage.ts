export type WSMessage<K extends string, U = {}> = {
    type: K;
} & U;

type Coordinates = { x: number; y: number };

export type WSMesagePress = WSMessage<'Press', Coordinates>;
export type WSMesageRelease = WSMessage<'Release', Coordinates>;
export type WSLightUpdate = WSMessage<
    'LightUpdate',
    { updates: [number, number, number][] }
>;
export type WSClearBoard = WSMessage<'ClearBoard'>;

export type WSMesage =
    | WSMesagePress
    | WSMesageRelease
    | WSLightUpdate
    | WSClearBoard;
