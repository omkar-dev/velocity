import { useInspectorStore } from "../store/inspectorStore";

export function PropertiesPanel() {
  const { selectedElement } = useInspectorStore();

  if (!selectedElement) {
    return (
      <div className="properties-panel empty">
        Click an element to inspect
      </div>
    );
  }

  const el = selectedElement;

  return (
    <div className="properties-panel">
      <h3>Properties</h3>
      <table>
        <tbody>
          <tr>
            <td className="prop-key">Type</td>
            <td>{el.element_type}</td>
          </tr>
          <tr>
            <td className="prop-key">Platform ID</td>
            <td className="mono">{el.platform_id || "—"}</td>
          </tr>
          <tr>
            <td className="prop-key">Label</td>
            <td>{el.label ?? "—"}</td>
          </tr>
          <tr>
            <td className="prop-key">Text</td>
            <td>{el.text ?? "—"}</td>
          </tr>
          <tr>
            <td className="prop-key">Bounds</td>
            <td className="mono">
              ({el.bounds.x}, {el.bounds.y}) {el.bounds.width}x{el.bounds.height}
            </td>
          </tr>
          <tr>
            <td className="prop-key">Enabled</td>
            <td>{el.enabled ? "Yes" : "No"}</td>
          </tr>
          <tr>
            <td className="prop-key">Visible</td>
            <td>{el.visible ? "Yes" : "No"}</td>
          </tr>
          <tr>
            <td className="prop-key">Children</td>
            <td>{el.children.length}</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
