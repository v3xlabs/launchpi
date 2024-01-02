export type WSMessage<K extends string, U> = {
    type: K;
} & U;

export type WSMesagePress = WSMessage<'Press', { x: number; y: number }>;
export type WSMesageRelease = WSMessage<'Release', { x: number; y: number }>;

export type WSMesageAny = WSMesagePress | WSMesageRelease;
