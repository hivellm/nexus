/**
 * App shell. Renders the chrome (Titlebar, ActivityRail, StatusBar,
 * Tweaks) around the body grid. Left panel, workspace, and right
 * drawer are filled in by §5–§7 of `phase5_gui-redesign-mockup-v2`.
 */
import { Titlebar } from '../components/chrome/Titlebar';
import { ActivityRail } from '../components/chrome/ActivityRail';
import { StatusBar } from '../components/chrome/StatusBar';
import { Tweaks } from '../components/chrome/Tweaks';
import { LeftColumn } from '../components/panels/LeftColumn';
import { Workspace } from '../components/workspace/Workspace';
import { RightDrawer } from '../components/drawer/RightDrawer';

export function App() {
  return (
    <div className="app">
      <Titlebar />
      <div className="body">
        <ActivityRail />
        <LeftColumn />
        <Workspace />
        <RightDrawer />
      </div>
      <StatusBar />
      <Tweaks />
    </div>
  );
}
