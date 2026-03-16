import { ActionToolbar } from "./components/ActionToolbar";
import { DevicePicker } from "./components/DevicePicker";
import { ElementTree } from "./components/ElementTree";
import { PropertiesPanel } from "./components/PropertiesPanel";
import { ScreenshotCanvas } from "./components/ScreenshotCanvas";
import { SelectorDisplay } from "./components/SelectorDisplay";
import { useWebSocket } from "./hooks/useWebSocket";
import { useInspectorStore } from "./store/inspectorStore";

export default function App() {
  useWebSocket();
  const { error, setError } = useInspectorStore();

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <h1>Velocity Inspector</h1>
          <DevicePicker />
        </div>
        <ActionToolbar />
      </header>

      {error && (
        <div className="error-bar" onClick={() => setError(null)}>
          {error}
        </div>
      )}

      <main className="app-main">
        <section className="panel-left">
          <ScreenshotCanvas />
        </section>
        <section className="panel-right">
          <ElementTree />
          <PropertiesPanel />
          <SelectorDisplay />
        </section>
      </main>
    </div>
  );
}
