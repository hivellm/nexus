/**
 * Tweaks — floating panel anchored bottom-right (above the status
 * bar). Exposes the dark/light theme toggle as a segmented control;
 * `theme` change writes to `layoutStore` and the
 * `bindThemeToHtml()` subscriber in `main.tsx` flips the
 * `data-theme` attribute on `<html>` so every CSS variable in
 * `tokens.css` re-resolves live.
 *
 * Visibility is controlled by `layoutStore.tweaksVisible` — both
 * the rail's tweaks button and the titlebar's settings icon flip
 * the flag.
 */
import { useLayoutStore, type Theme } from '../../stores/layoutStore';
import { CloseIcon } from '../../icons';

const THEMES: ReadonlyArray<{ id: Theme; label: string }> = [
  { id: 'dark', label: 'Dark' },
  { id: 'light', label: 'Light' },
];

export function Tweaks() {
  const visible = useLayoutStore((s) => s.tweaksVisible);
  const theme = useLayoutStore((s) => s.theme);
  const setTheme = useLayoutStore((s) => s.setTheme);
  const toggleTweaks = useLayoutStore((s) => s.toggleTweaks);

  if (!visible) return null;

  return (
    <div className="tweaks" role="dialog" aria-label="Tweaks">
      <div className="tw-head">
        <span style={{ flex: 1 }}>Tweaks</span>
        <button
          type="button"
          className="hd-btn"
          onClick={toggleTweaks}
          aria-label="Close tweaks"
        >
          <CloseIcon />
        </button>
      </div>
      <div className="tw-body">
        <div className="tw-row">
          <span>Theme</span>
          <div className="seg" role="group" aria-label="Theme">
            {THEMES.map((t) => (
              <button
                key={t.id}
                type="button"
                className={theme === t.id ? 'on' : ''}
                onClick={() => setTheme(t.id)}
                aria-pressed={theme === t.id}
              >
                {t.label}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
