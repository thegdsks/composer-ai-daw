// Small monochrome icons for the six effect devices, same stroke style as the rest of the set.
import type { SVGProps } from 'react';
import type { EffectKind } from '../model';

type Props = SVGProps<SVGSVGElement> & { size?: number };

function Base({ size = 16, children, ...rest }: Props) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      {children}
    </svg>
  );
}

/** Three fading bars: an echo. */
function EchoIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M5 8v8" />
      <path d="M10 9.5v5" opacity="0.75" />
      <path d="M15 10.5v3" opacity="0.55" />
      <path d="M19.5 11.5v1" opacity="0.4" />
    </Base>
  );
}

/** A wave clipped flat at the top and bottom. */
function DistortionIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M3 17l4-10h4l2 10h4l4-10" />
      <path d="M3 7h18M3 17h18" opacity="0.35" />
    </Base>
  );
}

/** A staircase: fewer steps, coarser sound. */
function CrushIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M3 17h4v-4h4V9h4V6h6" />
    </Base>
  );
}

/** A door in a wall: a gate. */
function GateIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M4 20V5M20 20V5" />
      <path d="M4 20h16" />
      <path d="M9 20v-8h6v8" />
    </Base>
  );
}

/** A wave swelling between two sides. */
function TidalIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M3 12c3-8 6-8 9 0s6 8 9 0" />
      <path d="M3 20h18" opacity="0.4" />
    </Base>
  );
}

/** Left and right bars with a center mark. */
function BalanceIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M12 4v16" />
      <path d="M4 9v6M8 7v10M16 7v10M20 9v6" />
    </Base>
  );
}

const ICONS: Record<EffectKind, (p: Props) => React.JSX.Element> = {
  delay: EchoIcon,
  waveshaper: DistortionIcon,
  crusher: CrushIcon,
  gate: GateIcon,
  tidal: TidalIcon,
  stereo_tool: BalanceIcon,
};

export function DeviceIcon(props: Props & { kind: EffectKind }) {
  const { kind, ...rest } = props;
  const Icon = ICONS[kind];
  return <Icon {...rest} />;
}
