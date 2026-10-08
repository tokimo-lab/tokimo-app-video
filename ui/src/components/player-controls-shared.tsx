import { Tooltip } from "@tokimo/ui";
import type { CSSProperties, ReactElement, RefObject } from "react";
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

export interface PlayerDropdownPosition {
  style: CSSProperties;
  portalTarget: Element;
  compact: boolean;
}

export function useDropdownPortalPos(
  anchorRef: RefObject<HTMLDivElement | null>,
  open: boolean,
): PlayerDropdownPosition | null {
  const [pos, setPos] = useState<PlayerDropdownPosition | null>(null);

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
        const player = el.closest(".player-subtitle-host");
        const compact = !!el.closest('[data-compact="true"]');
        let style: CSSProperties = {
          right: window.innerWidth - rect.right,
          bottom: window.innerHeight - rect.top + 4,
          maxHeight: "min(400px, 60vh)",
        };
        if (compact && player) {
          const bounds = player.getBoundingClientRect();
          const viewport = window.visualViewport;
          const left = Math.max(bounds.left, viewport?.offsetLeft ?? 0) + 8;
          const right =
            Math.min(
              bounds.right,
              (viewport?.offsetLeft ?? 0) +
                (viewport?.width ?? window.innerWidth),
            ) - 8;
          const top = Math.max(bounds.top, viewport?.offsetTop ?? 0) + 8;
          const toolbar = player.querySelector(".player-toolbar");
          const bottom = Math.min(
            (toolbar?.getBoundingClientRect().top ?? rect.top) - 4,
            (viewport?.offsetTop ?? 0) +
              (viewport?.height ?? window.innerHeight) -
              8,
          );
          const width = Math.min(480, Math.max(1, right - left));
          style = {
            position: "absolute",
            left: left + (right - left - width) / 2 - bounds.left,
            bottom: bounds.bottom - Math.max(top, bottom),
            width,
            maxHeight: Math.max(1, bottom - top),
          };
        }
        const portalTarget = compact && player ? player : document.body;
        setPos((previous) => {
          if (
            previous?.compact === compact &&
            previous.portalTarget === portalTarget &&
            previous.style.left === style.left &&
            previous.style.right === style.right &&
            previous.style.bottom === style.bottom &&
            previous.style.width === style.width &&
            previous.style.maxHeight === style.maxHeight
          )
            return previous;
          return { style, portalTarget, compact };
        });
      }
      rafId = requestAnimationFrame(update);
    };

    update();
    return () => cancelAnimationFrame(rafId);
  }, [anchorRef, open]);

  return pos;
}
