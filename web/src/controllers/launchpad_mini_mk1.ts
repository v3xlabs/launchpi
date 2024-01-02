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
    if (y === 0 && x === 8) return 'aspect-square';

    if (y === 0) return 'aspect-square bg-white rounded-full border scale-75';

    if (x === 8) return 'aspect-square bg-white rounded-full border scale-75';

    if (x === 3 && y == 4) return 'aspect-square bg-white border rounded-br-lg';

    if (x === 4 && y == 4) return 'aspect-square bg-white border rounded-bl-lg';

    if (x === 3 && y == 5) return 'aspect-square bg-white border rounded-tr-lg';

    if (x === 4 && y == 5) return 'aspect-square bg-white border rounded-tl-lg';

    return 'aspect-square bg-white border';
};

export const launchpad_mini_mk1 = {
    getCustomCSS,
};
