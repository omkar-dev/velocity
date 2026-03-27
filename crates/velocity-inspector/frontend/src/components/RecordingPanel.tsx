import { useState } from "react";
import * as api from "../api/client";
import { useInspectorStore } from "../store/inspectorStore";

export function RecordingPanel() {
  const {
    isRecording,
    recordedSteps,
    removeRecordedStep,
    clearRecordedSteps,
    setError,
  } = useInspectorStore();
  const [flowName, setFlowName] = useState("recorded_flow");
  const [appId, setAppId] = useState("com.example.app");
  const [saving, setSaving] = useState(false);
  const [savedPath, setSavedPath] = useState<string | null>(null);

  if (!isRecording && recordedSteps.length === 0) return null;

  const fullYaml = buildFullYaml(flowName, appId, recordedSteps.map((s) => s.yaml));

  const copyAll = () => {
    navigator.clipboard.writeText(fullYaml);
  };

  const handleSave = async () => {
    if (recordedSteps.length === 0) return;
    setSaving(true);
    setSavedPath(null);
    try {
      const result = await api.saveFlow(
        flowName,
        appId,
        recordedSteps.map((s) => s.yaml)
      );
      setSavedPath(result.path);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="recording-panel">
      <div className="recording-header">
        <h3>
          {isRecording && <span className="rec-dot" />}
          {isRecording ? "Recording" : "Recorded Flow"}
        </h3>
        <span className="step-count">{recordedSteps.length} steps</span>
      </div>

      {/* Step list */}
      <div className="recorded-steps">
        {recordedSteps.length === 0 ? (
          <div className="recording-hint">
            Interact with the device to record steps...
          </div>
        ) : (
          recordedSteps.map((step, i) => (
            <div key={i} className="recorded-step">
              <span className="step-index">{i + 1}</span>
              <span className="step-action">{step.action}</span>
              <span className="step-label">{step.label}</span>
              <button
                className="step-remove"
                onClick={() => removeRecordedStep(i)}
                title="Remove step"
              >
                x
              </button>
            </div>
          ))
        )}
      </div>

      {/* YAML preview */}
      {recordedSteps.length > 0 && (
        <>
          <div className="recording-config">
            <label>
              Name
              <input
                type="text"
                value={flowName}
                onChange={(e) => setFlowName(e.target.value)}
              />
            </label>
            <label>
              App ID
              <input
                type="text"
                value={appId}
                onChange={(e) => setAppId(e.target.value)}
              />
            </label>
          </div>

          <div className="yaml-block">
            <div className="yaml-header">
              <span>Flow YAML</span>
              <button className="copy-btn" onClick={copyAll}>
                Copy
              </button>
            </div>
            <pre>{fullYaml}</pre>
          </div>

          <div className="recording-actions">
            <button onClick={handleSave} disabled={saving || recordedSteps.length === 0}>
              {saving ? "Saving..." : "Save to File"}
            </button>
            <button onClick={clearRecordedSteps} className="secondary">
              Clear
            </button>
          </div>

          {savedPath && (
            <div className="save-success">
              Saved to {savedPath}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function buildFullYaml(name: string, appId: string, stepYamls: string[]): string {
  const stepsBlock = stepYamls
    .map((y) =>
      y
        .split("\n")
        .map((line) => `  ${line}`)
        .join("\n")
    )
    .join("\n");

  return `name: ${yamlDoubleQuoted(name)}
appId: ${yamlDoubleQuoted(appId)}

steps:
${stepsBlock}
`;
}

function yamlDoubleQuoted(value: string): string {
  return `"${value
    .replace(/\\/g, "\\\\")
    .replace(/\"/g, '\\\"')
    .replace(/\n/g, "\\n")
    .replace(/\r/g, "\\r")}"`;
}
