// Local icons for the timeline lanes, tap tempo and clip loop. Same stroke style as the rest of the set.
import type { SVGProps } from 'react';

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

/** Three stacked rows, for the lanes menu. */
export function LanesIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M4 6h16" />
      <path d="M4 12h16" />
      <path d="M4 18h10" />
      <circle cx="18" cy="18" r="1.5" />
    </Base>
  );
}

/** A hand tapping a beat: a dot with two rings. */
export function TapIcon(props: Props) {
  return (
    <Base {...props}>
      <circle cx="12" cy="12" r="2" />
      <circle cx="12" cy="12" r="6" />
      <circle cx="12" cy="12" r="10" />
    </Base>
  );
}

/** A flag on a pole, for a marker. */
export function FlagIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M6 21V4" />
      <path d="M6 5h11l-2.5 4L17 13H6" />
    </Base>
  );
}
