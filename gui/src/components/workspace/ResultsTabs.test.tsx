/**
 * `ResultsTabs` tests. Verifies the four-tab strip renders, the
 * mini-meta numbers surface, and clicking a tab fires `onMode`
 * with the matching id. The actual view switching happens in
 * `Workspace.tsx`; this component owns only the tab UI.
 */
import { describe, it, expect, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { ResultsTabs } from './ResultsTabs';

describe('ResultsTabs', () => {
  it('renders all four tabs + mini-meta', () => {
    render(
      <ResultsTabs
        mode="graph"
        onMode={() => {}}
        rowCount={42}
        nodeCount={7}
        ms={123}
        planner="heuristic"
      />,
    );
    expect(screen.getByRole('tab', { name: /Graph/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Table/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /JSON/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Plan/ })).toBeInTheDocument();
    // Counts surface in the badges.
    expect(screen.getByText('42', { selector: '.badge' })).toBeInTheDocument();
    expect(screen.getByText('7', { selector: '.badge' })).toBeInTheDocument();
    expect(screen.getByText('123ms')).toBeInTheDocument();
  });

  it('marks the active tab with aria-selected', () => {
    render(
      <ResultsTabs
        mode="json"
        onMode={() => {}}
        rowCount={0}
        nodeCount={0}
        ms={0}
      />,
    );
    const json = screen.getByRole('tab', { name: /JSON/ });
    expect(json.getAttribute('aria-selected')).toBe('true');
    const graph = screen.getByRole('tab', { name: /Graph/ });
    expect(graph.getAttribute('aria-selected')).toBe('false');
  });

  it('clicking a tab fires onMode with the matching id', () => {
    const onMode = vi.fn();
    render(
      <ResultsTabs
        mode="graph"
        onMode={onMode}
        rowCount={0}
        nodeCount={0}
        ms={0}
      />,
    );
    fireEvent.click(screen.getByRole('tab', { name: /Table/ }));
    expect(onMode).toHaveBeenCalledWith('table');
    fireEvent.click(screen.getByRole('tab', { name: /Plan/ }));
    expect(onMode).toHaveBeenCalledWith('plan');
  });
});
