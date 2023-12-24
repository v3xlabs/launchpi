import { FC } from 'react';

export const App: FC = () => {
    return (
        <div className="w-full max-w-xl mx-auto p-4 lg:mt-4 flex flex-col gap-4">
            <h1>Launchpi</h1>
            <div className="w-full border rounded-lg bg-white p-4">
                <p>
                    Please select a device you wish to start your session with
                </p>
            </div>
            <div className="w-full grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
                <h2>Sessions</h2>
            </div>
        </div>
    );
};
