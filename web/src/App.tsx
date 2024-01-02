import { FC } from 'react';

import { DeviceList } from './components/DeviceList';
import { TestSubscription } from './components/TestSubscription';

export const App: FC = () => {
    return (
        <div className="w-full max-w-xl mx-auto p-4 lg:mt-4 flex flex-col gap-4">
            <h1>Launchpi</h1>
            <DeviceList />
            <h2>Sessions</h2>
            <div className="w-full grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                <TestSubscription />
                <TestSubscription />
                <TestSubscription />
            </div>
        </div>
    );
};
