import { useCallback, useEffect, useRef, useState, type UIEvent } from "react";
import { classifyLaunchLogLine, launchLogLevelClass, type LaunchLogLine } from "../lib/launch-log";

const LOG_LINE_HEIGHT = 18;
const LOG_OVERSCAN = 12;

export function VirtualizedLogList({ lines }: { lines: readonly LaunchLogLine[] }) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const [viewportHeight, setViewportHeight] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const updateViewportHeight = () => setViewportHeight(viewport.clientHeight);
    updateViewportHeight();

    const observer = new ResizeObserver(updateViewportHeight);
    observer.observe(viewport);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || !stickToBottomRef.current) return;

    viewport.scrollTop = viewport.scrollHeight;
    setScrollTop(viewport.scrollTop);
  }, [lines]);

  const handleScroll = useCallback((event: UIEvent<HTMLDivElement>) => {
    const viewport = event.currentTarget;
    const distanceFromBottom = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight;
    stickToBottomRef.current = distanceFromBottom <= LOG_LINE_HEIGHT * 2;
    setScrollTop(viewport.scrollTop);
  }, []);

  const firstVisibleLine = Math.max(0, Math.floor(scrollTop / LOG_LINE_HEIGHT) - LOG_OVERSCAN);
  const lastVisibleLine = Math.min(lines.length, Math.ceil((scrollTop + Math.max(viewportHeight, LOG_LINE_HEIGHT)) / LOG_LINE_HEIGHT) + LOG_OVERSCAN);

  return (
    <div
      ref={viewportRef}
      role="log"
      aria-label="Launcher console output"
      tabIndex={0}
      onScroll={handleScroll}
      className="min-h-0 flex-1 overflow-auto bg-black/60 px-3 py-2 font-mono text-[11px]"
    >
      {lines.length === 0 ? (
        <div className="text-muted-foreground">No log output yet.</div>
      ) : (
        <div
          style={{
            height: lines.length * LOG_LINE_HEIGHT,
            position: "relative",
          }}
        >
          {lines.slice(firstVisibleLine, lastVisibleLine).map((entry, offset) => {
            const lineIndex = firstVisibleLine + offset;
            return (
              <div
                key={lineIndex}
                className={`${launchLogLevelClass(classifyLaunchLogLine(entry))} whitespace-pre`}
                style={{
                  height: LOG_LINE_HEIGHT,
                  left: 0,
                  lineHeight: `${LOG_LINE_HEIGHT}px`,
                  minWidth: "100%",
                  overflow: "hidden",
                  position: "absolute",
                  top: lineIndex * LOG_LINE_HEIGHT,
                  width: "max-content",
                }}
              >
                {entry.line}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
