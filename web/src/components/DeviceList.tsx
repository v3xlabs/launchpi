import { useDevices } from '../hooks/useDevices';

export const DeviceList = () => {
    const { data } = useDevices();

    return (
        <div className="w-full border rounded-lg bg-white p-4">
            <p>Please select a device you wish to start your session with</p>
            <div className="space-y-1">
                {data?.devices?.map((device) => (
                    <div
                        className="border px-3 py-2 rounded-md flex justify-between"
                        key={device.name}
                    >
                        <div>{device.name}</div>
                        <div>
                            {device.connected ? (
                                <span className="text-green-500">
                                    connected
                                </span>
                            ) : (
                                <form
                                    action="http://localhost:3000/connect"
                                    method="GET"
                                    onSubmit={(event) => {
                                        // This is the best button implementation you will ever see
                                        event.preventDefault();

                                        fetch(
                                            'http://localhost:3000/connect/' +
                                                device.id
                                        );
                                    }}
                                >
                                    <button>connect</button>
                                </form>
                            )}
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
};
