import { useState } from "react";
import * as api from "../api/client";
import type { Direction } from "../api/types";
import { useInspectorStore } from "../store/inspectorStore";

export function ActionToolbar() {
  const { currentDeviceId, generated, setError } = useInspectorStore();
  const [typeText, setTypeText] = useState("");

  if (!currentDeviceId) return null;

  const handleTap = async () => {
    if (!generated) return;
    try {
      await api.tap(currentDeviceId, { selector: generated.selector });
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Tap failed");
    }
  };

  const handleType = async () => {
    if (!generated || !typeText) return;
    try {
      await api.typeText(currentDeviceId, generated.selector, typeText);
      setTypeText("");
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Type failed");
    }
  };

  const handleSwipe = async (direction: Direction) => {
    try {
      await api.swipe(currentDeviceId, direction);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Swipe failed");
    }
  };

  return (
    <div className="action-toolbar">
      <div className="action-group">
        <button onClick={handleTap} disabled={!generated} title="Tap selected element">
          Tap
        </button>
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
