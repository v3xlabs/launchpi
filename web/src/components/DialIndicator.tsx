import { Component } from 'solid-js';

import { RgbaColor } from '../api/inventory';
import { toHex } from '../utils/rendered';

const unsetColor = '#262626';
const trackColor = '#262626';

// The Studio ring is 24 discrete LED segments, lit from the bottom of the circle clockwise.
const ringSegments = 24;
const segmentAngle = 360 / ringSegments;
const segmentGapAngle = 2.5;
// Half a segment of offset puts the last (100%) segment centred on the bottom of the ring
// instead of a seam between two segments sitting there.
const ringStartAngle = 180 + segmentAngle / 2;

// Same integer maths as the daemon uses when it lights the ring.
export const litRingSegments = (level: number): number => Math.floor((level * ringSegments) / 100);
export const totalRingSegments = ringSegments;

const ringBackground = (color: string, level: number): string => {
    const lit = litRingSegments(level);
    const stops = Array.from({ length: ringSegments }, (_, index) => {
        const start = index * segmentAngle;
        const litStart = start + segmentGapAngle / 2;
        const litEnd = start + segmentAngle - segmentGapAngle / 2;
        const fill = index < lit ? color : trackColor;
        return [
            `transparent ${start}deg ${litStart}deg`,
            `${fill} ${litStart}deg ${litEnd}deg`,
            `transparent ${litEnd}deg ${start + segmentAngle}deg`,
        ].join(', ');
    });
    return `conic-gradient(from ${ringStartAngle}deg, ${stops.join(', ')})`;
};

type DialIndicatorProps = {
    index: number;
    color: RgbaColor | null;
    level: number;
    isPressed?: boolean;
};

// A picture of the hardware, nothing else: the knob takes the dial colour and the ring lights up
// to the level. Any readout belongs next to the editor, not on the surface preview.
export const DialIndicator: Component<DialIndicatorProps> = (props) => {
    const hex = () => toHex(props.color, unsetColor);
    const level = () => Math.min(100, Math.max(0, Math.round(props.level)));

    return (
        <div
            class="dial-ring"
            data-pressed={props.isPressed === true}
            style={{ background: ringBackground(hex(), level()), '--dial-color': hex() }}
            title={`Dial ${props.index + 1} · ${level()}% · ${litRingSegments(level())}/${ringSegments} segments · ${
                props.color === null ? 'no colour' : hex()
            }${props.isPressed === true ? ' · pressed' : ''}`}
        >
            <div class="dial-hub" />
        </div>
    );
};
