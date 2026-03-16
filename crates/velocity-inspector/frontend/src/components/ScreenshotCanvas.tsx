import { useRef } from "react";
import type { Element } from "../api/types";
import { useInspectorStore } from "../store/inspectorStore";

/** Collect only leaf elements and elements with meaningful (non-fullscreen) bounds */
function getInteractiveElements(root: Element): Element[] {
  const result: Element[] = [];
  const rootArea = root.bounds.width * root.bounds.height;

  function walk(el: Element) {
    const area = el.bounds.width * el.bounds.height;
    const isLeaf = el.children.length === 0;
    // Show leaves, or elements that are significantly smaller than the screen
    const isSmallEnough = area > 0 && area < rootArea * 0.8;

    if (el.visible && el.bounds.width > 0 && (isLeaf || isSmallEnough)) {
      result.push(el);
    }
    for (const child of el.children) {
      walk(child);
    }
  }

  walk(root);
  return result;
}

function findDeepestElement(
  root: Element,
  x: number,
  y: number
): Element | null {
  // Find the deepest (smallest) element containing the click point
  let best: Element | null = null;
  let bestArea = Infinity;

  function walk(el: Element) {
    const b = el.bounds;
    if (x >= b.x && x < b.x + b.width && y >= b.y && y < b.y + b.height) {
      const area = b.width * b.height;
      if (area < bestArea) {
        best = el;
        bestArea = area;
      }
      for (const child of el.children) {
        walk(child);
      }
    }
  }

  walk(root);
  return best;
}

function isElementMatch(a: Element, b: Element): boolean {
  return (
    a.platform_id === b.platform_id &&
    a.bounds.x === b.bounds.x &&
    a.bounds.y === b.bounds.y &&
    a.bounds.width === b.bounds.width &&
    a.bounds.height === b.bounds.height
  );
}

export function ScreenshotCanvas() {
  const { screenshotUrl, hierarchy, selectedElement, setSelectedElement } =
    useInspectorStore();
  const containerRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);

  const rootBounds = hierarchy
    ? { w: hierarchy.bounds.width || 1, h: hierarchy.bounds.height || 1 }
    : null;

  const handleClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!hierarchy || !rootBounds) return;
    const img = imgRef.current;
    if (!img) return;

    const rect = img.getBoundingClientRect();
    const deviceX = ((e.clientX - rect.left) / rect.width) * rootBounds.w;
    const deviceY = ((e.clientY - rect.top) / rect.height) * rootBounds.h;

    const found = findDeepestElement(hierarchy, deviceX, deviceY);
    setSelectedElement(found);
  };

  const interactiveElements = hierarchy
    ? getInteractiveElements(hierarchy)
    : [];

  return (
    <div className="screenshot-canvas" ref={containerRef}>
      {screenshotUrl ? (
        <div className="screenshot-wrapper" onClick={handleClick}>
          <img
            ref={imgRef}
            src={screenshotUrl}
            alt="Device screenshot"
            draggable={false}
          />
          {rootBounds && (
            <svg
              className="element-overlay"
              viewBox={`0 0 ${rootBounds.w} ${rootBounds.h}`}
              preserveAspectRatio="none"
            >
              {interactiveElements.map((el, i) => {
                const isSelected =
                  selectedElement && isElementMatch(el, selectedElement);
                return (
                  <rect
                    key={`${el.platform_id}-${i}`}
                    x={el.bounds.x}
                    y={el.bounds.y}
                    width={el.bounds.width}
                    height={el.bounds.height}
                    fill={isSelected ? "rgba(59,130,246,0.2)" : "transparent"}
                    stroke={isSelected ? "#3b82f6" : "rgba(59,130,246,0.25)"}
                    strokeWidth={isSelected ? 2 : 1}
                    vectorEffect="non-scaling-stroke"
                  />
                );
              })}
            </svg>
          )}
        </div>
      ) : (
        <div className="placeholder">No screenshot available</div>
      )}
    </div>
  );
}
