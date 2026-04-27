/**
 * `Sparkline` tests. The renderer is pure, so we exercise its
 * three branches: empty data → dashed baseline placeholder, single
 * point → polyline + last-point dot, varied data → monotonic SVG
 * path. We do not snapshot — the path math is exercised through
 * structural assertions so the test stays robust against
 * whitespace / coord-precision changes.
 */
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { Sparkline } from './Sparkline';

describe('Sparkline', () => {
  it('renders a dashed baseline when data is empty', () => {
    const { container } = render(<Sparkline data={[]} />);
    const svg = container.querySelector('svg');
    expect(svg).toBeTruthy();
    expect(svg?.classList.contains('empty')).toBe(true);
    // No <path> when empty — the empty branch only draws the line.
    expect(container.querySelectorAll('path')).toHaveLength(0);
  });

  it('renders a fill area + line + last-point dot when data exists', () => {
    const { container } = render(<Sparkline data={[1, 4, 2, 8, 5]} />);
    expect(container.querySelectorAll('path')).toHaveLength(2); // area + line
    const dot = container.querySelector('circle');
    expect(dot).toBeTruthy();
    // Default width = 120 → last point lands on x = 120
    // (formatted as "120" once Number.toFixed strips trailing
    // zeros via the renderer's parseFloat round-trip).
    expect(parseFloat(dot?.getAttribute('cx') ?? '0')).toBe(120);
  });

  it('respects width / height props', () => {
    const { container } = render(
      <Sparkline data={[1, 2, 3]} width={200} height={50} />,
    );
    const svg = container.querySelector('svg');
    expect(svg?.getAttribute('width')).toBe('200');
    expect(svg?.getAttribute('height')).toBe('50');
  });

  it('respects color override', () => {
    const { container } = render(
      <Sparkline data={[1, 2]} color="#ff0000" />,
    );
    const line = container.querySelectorAll('path')[1];
    expect(line.getAttribute('stroke')).toBe('#ff0000');
  });
});
