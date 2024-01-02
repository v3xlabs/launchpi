import { useDevices } from '../hooks/useDevices';
import { TestSubscription } from './TestSubscription';

export const TestSessions = () => {
    const { data } = useDevices();

    return (
        <div className="space-y-2">
            <h2>Sessions</h2>
            <div className="w-full grid grid-cols-1 md:grid-cols-2 2xl:grid-cols-3 gap-4">
                {data?.devices
                    ?.filter((device) => device.connected)
                    .map((device) => (
                        <TestSubscription
                            device_id={device.id}
                            key={device.id}
                            device_type={device.name}
                        />
                    ))}
            </div>
        </div>
    );
};
