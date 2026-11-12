import { useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useListKeys } from "@/lib/hooks";
import { cn } from "@/lib/utils";

/** Virtualized list with j/k keyboard navigation. Only visible rows are rendered (5,000+ items are fine). */
export function VirtualList<T>({ items, rowHeight = 44, getKey, renderRow, active, onActive, onOpen, className, bottomPad = 24, keys = true }: {
  items: T[];
  rowHeight?: number;
  getKey(item: T): string;
  renderRow(item: T, state: { active: boolean; index: number }): React.ReactNode;
  active: number;
  onActive(i: number): void;
  onOpen?(item: T): void;
  className?: string;
  bottomPad?: number;
  keys?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const virt = useVirtualizer({ count: items.length, getScrollElement: () => ref.current, estimateSize: () => rowHeight, overscan: 12 });
  useListKeys({ count: items.length, index: active, setIndex: onActive, onEnter: (i) => items[i] && onOpen?.(items[i]), enabled: keys });
  useEffect(() => { if (active >= 0 && active < items.length) virt.scrollToIndex(active, { align: "auto" }); }, [active]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div ref={ref} className={cn("min-h-0 flex-1 overflow-y-auto", className)}>
      <div style={{ height: virt.getTotalSize() + bottomPad }} className="relative w-full">
        {virt.getVirtualItems().map((v) => {
          const item = items[v.index]!;
          return (
            <div key={getKey(item)} style={{ transform: `translateY(${v.start}px)`, height: v.size }} className="absolute left-0 top-0 w-full">
              {renderRow(item, { active: v.index === active, index: v.index })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
