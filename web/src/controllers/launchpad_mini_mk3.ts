export const LAUNCHPAD_MINI_MK3_SIZE = {
    width: 9,
    height: 9,
};

export const isButton = (x: number, y: number) => {
    return !(x === 8 && y === 0);
};

export const isLight = (x: number, y: number) => {
    return !(x === 8 && y === 0);
};
