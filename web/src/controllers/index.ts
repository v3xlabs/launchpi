import { LAUNCHPAD_MINI_MK3_SIZE } from './launchpad_mini_mk3';

export const decodeGrid = (_controller: string, index: number) => {
    const controller_size = LAUNCHPAD_MINI_MK3_SIZE;

    const x = index % controller_size.width;
    const y = Math.floor(index / controller_size.height);

    return { x, y };
};
