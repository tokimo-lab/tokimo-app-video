import { Tooltip } from "@tokimo/ui";
import type { ReactElement, RefObject } from "react";
import { useEffect, useRef, useState } from "react";

export function PlayerControlTooltip({
  title,
  children,
}: {
  title: string;
  children: ReactElement;
}) {
  return (
    <Tooltip
      title={title}
      mouseEnterDelay={0}
      mouseLeaveDelay={0}
      color="bg-black/65 text-white backdrop-blur-2xl ring-1 ring-white/10"
    >
      {children}
    </Tooltip>
  );
}

export function useDismissOnOutsidePointerDown(
  open: boolean,
  onDismiss: () => void,
  ignoredSelectors: string[] = [],
  extraRefs: RefObject<Element | null>[] = [],
) {
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (containerRef.current?.contains(target)) return;
      if (extraRefs.some((ref) => ref.current?.contains(target))) return;
      if (
        target instanceof Element &&
        ignoredSelectors.some((selector) => target.closest(selector))
      ) {
        return;
      }
      onDismiss();
    };

    document.addEventListener("pointerdown", handlePointerDown, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true);
    };
  }, [extraRefs, ignoredSelectors, onDismiss, open]);

  return containerRef;
}

export function useDropdownPortalPos(
  anchorRef: RefObject<HTMLDivElement | null>,
  open: boolean,
): { right: number; bottom: number } | null {
  const [pos, setPos] = useState<{ right: number; bottom: number } | null>(
    null,
  );

  useEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }

    let rafId = 0;
    const update = () => {
      const el = anchorRef.current;
      if (el) {
        const rect = el.getBoundingClientRect();
        setPos({
          right: window.innerWidth - rect.right,
          bottom: window.innerHeight - rect.top + 4,
        });
      }
      rafId = requestAnimationFrame(update);
    };

    update();
    return () => cancelAnimationFrame(rafId);
  }, [anchorRef, open]);

  return pos;
}
