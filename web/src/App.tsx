import { FC } from 'react';

import { DeviceList } from './components/DeviceList';
import { TestSessions } from './components/TestSession';

export const App: FC = () => {
    return (
        <div className="w-full max-w-xl mx-auto p-4 lg:mt-4 flex flex-col gap-4">
            <h1>Launchpi</h1>
            <DeviceList />
            <TestSessions />
        </div>
    );
};
