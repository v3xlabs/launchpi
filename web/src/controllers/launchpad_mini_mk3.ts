export const LAUNCHPAD_MINI_MK3_SIZE = {
    width: 9,
    height: 9,
};

export const isPressableButton = (x: number, y: number) => {
    return !(x === 8 && y === 0);
};

export const isControllableLight = (x: number, y: number) => {
    return !(x === 8 && y === 0);
};

export const getCustomCSS = (x: number, y: number) => {
    if (y === 0 && x === 8) return 'aspect-square border rotate-45 scale-75';

    if (y === 0) return 'aspect-square bg-black border';

    if (x === 8) return 'aspect-square bg-black border';

    if (x === 3 && y == 4) return 'aspect-square bg-white border rounded-br-md';

    if (x === 4 && y == 4) return 'aspect-square bg-white border rounded-bl-md';

    if (x === 3 && y == 5) return 'aspect-square bg-white border rounded-tr-md';

    if (x === 4 && y == 5) return 'aspect-square bg-white border rounded-tl-md';

    return 'aspect-square bg-white border';
};

export const launchpad_mini_mk3 = {
    getCustomCSS,
};
