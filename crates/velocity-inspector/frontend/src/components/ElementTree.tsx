import { useState } from "react";
import type { Element } from "../api/types";
import { useInspectorStore } from "../store/inspectorStore";

function TreeNode({ element, depth }: { element: Element; depth: number }) {
  const { selectedElement, setSelectedElement } = useInspectorStore();
  const [expanded, setExpanded] = useState(depth < 2);

  const hasChildren = element.children.length > 0;
  const isSelected =
    selectedElement &&
    element.platform_id === selectedElement.platform_id &&
    element.bounds.x === selectedElement.bounds.x &&
    element.bounds.y === selectedElement.bounds.y;

  const label =
    element.label || element.text || element.platform_id || element.element_type;
  const truncated = label.length > 40 ? label.slice(0, 37) + "..." : label;

  return (
    <div className="tree-node">
      <div
        className={`tree-row ${isSelected ? "selected" : ""}`}
        style={{ paddingLeft: depth * 16 }}
        onClick={() => setSelectedElement(element)}
      >
        {hasChildren && (
          <span
            className="toggle"
            onClick={(e) => {
              e.stopPropagation();
              setExpanded(!expanded);
            }}
          >
            {expanded ? "▼" : "▶"}
          </span>
        )}
        {!hasChildren && <span className="toggle-spacer" />}
        <span className="type-badge">{element.element_type.split(".").pop()}</span>
        <span className="node-label">{truncated}</span>
      </div>
      {expanded &&
        hasChildren &&
        element.children.map((child, i) => (
          <TreeNode key={`${child.platform_id}-${i}`} element={child} depth={depth + 1} />
        ))}
    </div>
  );
}

export function ElementTree() {
  const { hierarchy } = useInspectorStore();

  if (!hierarchy) {
    return <div className="element-tree empty">No hierarchy loaded</div>;
  }

  return (
    <div className="element-tree">
      <h3>Element Tree</h3>
      <div className="tree-scroll">
        <TreeNode element={hierarchy} depth={0} />
      </div>
    </div>
  );
}
