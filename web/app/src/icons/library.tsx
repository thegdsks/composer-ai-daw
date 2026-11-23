// Local icons for the library sidebar. Same stroke style as the rest of the set.
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

/** A window with its left column marked, for the library sidebar. */
export function SidebarIcon(props: Props) {
  return (
    <Base {...props}>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M9 4v16" />
    </Base>
  );
}

/** A small wave, for the Sounds tab. */
export function WaveIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M3 12h2l2-6 3 12 3-9 2 5 2-2h4" />
    </Base>
  );
}

/** A folder tab, for the Samples tab. */
export function FolderIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </Base>
  );
}

/** Chevron pointing right; rotate with a class for open groups. */
export function ChevronIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M9 6l6 6-6 6" />
    </Base>
  );
}
