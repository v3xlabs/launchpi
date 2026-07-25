import { Component, createResource, Show } from 'solid-js';

import { RenderedState } from '../api/inventory';
import { renderedKeyImageUrl } from '../api/render';

export const KeyImage: Component<{ state: RenderedState }> = (props) => {
    const renderKey = () =>
        JSON.stringify([props.state.text, props.state.foreground_color, props.state.background_color]);
    const [url] = createResource(renderKey, () => renderedKeyImageUrl(props.state));
    return (
        <Show when={url()}>
            {(src) => (
                <img src={src()} alt="" class="pointer-events-none absolute inset-0 h-full w-full object-cover" />
            )}
        </Show>
    );
};
