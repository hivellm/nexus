/**
 * App shell. Renders the chrome (Titlebar, ActivityRail, StatusBar,
 * Tweaks) around the body grid. Left panel, workspace, and right
 * drawer are filled in by §5–§7 of `phase5_gui-redesign-mockup-v2`.
 */
import { Titlebar } from '../components/chrome/Titlebar';
import { ActivityRail } from '../components/chrome/ActivityRail';
import { StatusBar } from '../components/chrome/StatusBar';
import { Tweaks } from '../components/chrome/Tweaks';

export function App() {
  return (
    <div className="app">
      <Titlebar />
      <div className="body">
        <ActivityRail />
        <aside className="panel" aria-label="Left panel" />
        <main className="workspace" />
        <aside className="right-col" aria-label="Right drawer" />
      </div>
      <StatusBar />
      <Tweaks />
    </div>
  );
}
