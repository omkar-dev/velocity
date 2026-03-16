import { useEffect } from "react";
import { generateSelector } from "../api/client";
import { useInspectorStore } from "../store/inspectorStore";

function selectorToString(sel: Record<string, unknown>): string {
  const entries = Object.entries(sel);
  if (entries.length === 0) return "?";
  const [key, value] = entries[0];
  if (typeof value === "string") return `${key}: "${value}"`;
  return `${key}: ${JSON.stringify(value)}`;
}

export function SelectorDisplay() {
  const { selectedElement, generated, setGenerated, setError } =
    useInspectorStore();

  useEffect(() => {
    if (!selectedElement) {
      setGenerated(null);
      return;
    }
    generateSelector(selectedElement)
      .then(setGenerated)
      .catch((e) => setError(e.message));
  }, [selectedElement, setGenerated, setError]);

  if (!generated) {
    return (
      <div className="selector-display empty">
        Select an element to generate a selector
      </div>
    );
  }

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  return (
    <div className="selector-display">
      <h3>Selector</h3>
      <div className="selector-value">
        <code>{selectorToString(generated.selector as unknown as Record<string, unknown>)}</code>
      </div>
      <h3>YAML Snippets</h3>
      <div className="yaml-block">
        <div className="yaml-header">
          <span>Tap</span>
          <button className="copy-btn" onClick={() => copyToClipboard(generated.yaml_tap)}>
            Copy
          </button>
        </div>
        <pre>{generated.yaml_tap}</pre>
      </div>
      <div className="yaml-block">
        <div className="yaml-header">
          <span>Assert Visible</span>
          <button className="copy-btn" onClick={() => copyToClipboard(generated.yaml_assert)}>
            Copy
          </button>
        </div>
        <pre>{generated.yaml_assert}</pre>
      </div>
    </div>
  );
}
