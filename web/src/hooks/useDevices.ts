import useSWR from 'swr';

const fetcher = (url: string) => fetch(url).then((result) => result.json());

export type DevicesResponse = {
    devices: { name: string; id: string; connected: boolean }[];
};

export const useDevices = () =>
    useSWR<DevicesResponse>('http://localhost:3000/devices', fetcher, {
        refreshInterval: 1000,
    });
