import { useEffect, useRef } from "react";
import { InspectorSocket } from "../api/websocket";
import { useInspectorStore } from "../store/inspectorStore";

export function useWebSocket() {
  const { currentDeviceId, setScreenshotUrl, setHierarchy, setError, addPerfSample } =
    useInspectorStore();
  const socketRef = useRef<InspectorSocket | null>(null);

  useEffect(() => {
    if (!currentDeviceId) return;

    const socket = new InspectorSocket(currentDeviceId);
    socketRef.current = socket;

    socket.onMessage((msg) => {
      switch (msg.type) {
        case "screenshot":
          if (msg.url) setScreenshotUrl(msg.url);
          break;
        case "hierarchy":
          if (msg.root) setHierarchy(msg.root);
          break;
        case "performance":
          if (
            msg.javaHeapKb != null &&
            msg.nativeHeapKb != null &&
            msg.totalPssKb != null &&
            msg.cpuPercent != null
          ) {
            addPerfSample({
              timestamp: Date.now(),
              javaHeapKb: msg.javaHeapKb,
              nativeHeapKb: msg.nativeHeapKb,
              totalPssKb: msg.totalPssKb,
              cpuPercent: msg.cpuPercent,
            });
          }
          break;
        case "error":
          if (msg.message) setError(msg.message);
          break;
      }
    });

    socket.connect();

    return () => {
      socket.disconnect();
      socketRef.current = null;
    };
  }, [currentDeviceId, setScreenshotUrl, setHierarchy, setError, addPerfSample]);

  return socketRef;
}
