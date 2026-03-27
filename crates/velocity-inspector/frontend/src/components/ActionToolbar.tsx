import { useState } from "react";
import * as api from "../api/client";
import type { Direction, RecordedStep } from "../api/types";
import { useInspectorStore } from "../store/inspectorStore";

export function ActionToolbar() {
  const {
    currentDeviceId,
    generated,
    setError,
    isRecording,
    startRecording,
    stopRecording,
    addRecordedStep,
  } = useInspectorStore();
  const [typeText, setTypeText] = useState("");

  if (!currentDeviceId) return null;

  const record = (step: RecordedStep) => {
    if (isRecording) {
      addRecordedStep(step);
    }
  };

  const handleTap = async () => {
    if (!generated) return;
    try {
      await api.tap(currentDeviceId, { selector: generated.selector });
      record({
        action: "tap",
        label: selectorLabel(generated.selector),
        yaml: generated.yaml_tap,
        timestamp: Date.now(),
      });
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Tap failed");
    }
  };

  const handleType = async () => {
    if (!generated || !typeText) return;
    try {
      await api.typeText(currentDeviceId, generated.selector, typeText);
      const serializedText = yamlDoubleQuoted(typeText);
      // Generate inputText YAML inline
      const yaml = `- inputText:\n    selector:\n      ${selectorToYaml(generated.selector)}\n    text: ${serializedText}`;
      record({
        action: "inputText",
        label: `"${typeText}" into ${selectorLabel(generated.selector)}`,
        yaml,
        timestamp: Date.now(),
      });
      setTypeText("");
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Type failed");
    }
  };

  const handleSwipe = async (direction: Direction) => {
    try {
      await api.swipe(currentDeviceId, direction);
      record({
        action: "swipe",
        label: direction,
        yaml: `- swipe:\n    direction: ${direction}`,
        timestamp: Date.now(),
      });
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Swipe failed");
    }
  };

  const handleAssert = () => {
    if (!generated) return;
    record({
      action: "assertVisible",
      label: selectorLabel(generated.selector),
      yaml: generated.yaml_assert,
      timestamp: Date.now(),
    });
  };

  return (
    <div className="action-toolbar">
      <div className="action-group">
        <button
          className={`record-btn ${isRecording ? "recording" : ""}`}
          onClick={isRecording ? stopRecording : startRecording}
          title={isRecording ? "Stop recording" : "Start recording"}
        >
          {isRecording ? "Stop" : "Record"}
        </button>
      </div>
      <div className="action-group">
        <button onClick={handleTap} disabled={!generated} title="Tap selected element">
          Tap
        </button>
        {isRecording && (
          <button onClick={handleAssert} disabled={!generated} title="Assert element is visible">
            Assert
          </button>
        )}
        <div className="type-action">
          <input
            type="text"
            value={typeText}
            onChange={(e) => setTypeText(e.target.value)}
            placeholder="Text to type..."
            onKeyDown={(e) => e.key === "Enter" && handleType()}
          />
          <button onClick={handleType} disabled={!generated || !typeText}>
            Type
          </button>
        </div>
      </div>
      <div className="action-group swipe-group">
        <button onClick={() => handleSwipe("up")} title="Swipe Up">Up</button>
        <button onClick={() => handleSwipe("down")} title="Swipe Down">Down</button>
        <button onClick={() => handleSwipe("left")} title="Swipe Left">Left</button>
        <button onClick={() => handleSwipe("right")} title="Swipe Right">Right</button>
      </div>
    </div>
  );
}

/** Extract a human-readable label from a selector object. */
function selectorLabel(sel: Record<string, unknown>): string {
  const entries = Object.entries(sel);
  if (entries.length === 0) return "?";
  const [, value] = entries[0];
  if (typeof value === "string") return value.length > 30 ? value.slice(0, 30) + "..." : value;
  return JSON.stringify(value);
}

/** Convert a selector object to inline YAML (for inputText steps). */
function selectorToYaml(sel: Record<string, unknown>): string {
  const entries = Object.entries(sel);
  if (entries.length === 0) return "?";
  const [key, value] = entries[0];
  const yamlKey = key.charAt(0).toLowerCase() + key.slice(1);
  if (typeof value === "string") return `${yamlKey}: ${yamlDoubleQuoted(value)}`;
  return `${yamlKey}: ${JSON.stringify(value)}`;
}

function yamlDoubleQuoted(value: string): string {
  return `"${value
    .replace(/\\/g, "\\\\")
    .replace(/\"/g, '\\\"')
    .replace(/\n/g, "\\n")
    .replace(/\r/g, "\\r")
    .replace(/\t/g, "\\t")}"`;
}
