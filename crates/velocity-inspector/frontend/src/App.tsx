import { ActionToolbar } from "./components/ActionToolbar";
import { DevicePicker } from "./components/DevicePicker";
import { ElementTree } from "./components/ElementTree";
import { PerformanceChart } from "./components/PerformanceChart";
import { PropertiesPanel } from "./components/PropertiesPanel";
import { RecordingPanel } from "./components/RecordingPanel";
import { ScreenshotCanvas } from "./components/ScreenshotCanvas";
import { SelectorDisplay } from "./components/SelectorDisplay";
import { useWebSocket } from "./hooks/useWebSocket";
import { useInspectorStore } from "./store/inspectorStore";

export default function App() {
  useWebSocket();
  const { error, setError, isRecording, recordedSteps } = useInspectorStore();
  const showRecording = isRecording || recordedSteps.length > 0;

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
          <PerformanceChart />
        </section>
        <section className={`panel-right ${showRecording ? "with-recording" : ""}`}>
          <ElementTree />
          <PropertiesPanel />
          <SelectorDisplay />
          <RecordingPanel />
        </section>
      </main>
    </div>
  );
}
