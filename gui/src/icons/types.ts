/**
 * Shared SVG icon prop type. Mirrors the loose `(p) => <svg {...p}/>`
 * shape from the original mockup (`gui/assets/icons.jsx`) but typed
 * for React + TypeScript so call sites get autocomplete on
 * `aria-label`, `className`, `onClick`, etc.
 */
import type { SVGProps } from 'react';

export type IconProps = SVGProps<SVGSVGElement>;
