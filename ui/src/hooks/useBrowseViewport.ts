import { useCallback, useLayoutEffect, useRef, useState } from "react";

/** Keep the current list's layout and selected card position across detail views. */
export function useBrowseViewport(active: boolean) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const gridWrapperRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const anchorRef = useRef<{
    card: HTMLElement;
    offset: number;
    scrollTop: number;
    width: number;
  } | null>(null);
  const returningRef = useRef(false);

  useLayoutEffect(() => {
    if (!active) {
      returningRef.current = true;
      return;
    }
    const el = gridWrapperRef.current;
    if (!el) return;
    const measure = (next: number) => {
      if (next > 0) setWidth(next);
    };
    measure(el.getBoundingClientRect().width);
    const observer = new ResizeObserver((entries) => {
      measure(entries[0]?.contentRect.width ?? 0);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [active]);

  useLayoutEffect(() => {
    if (!active || !returningRef.current) return;
    const scroll = scrollRef.current;
    const anchor = anchorRef.current;
    if (!scroll || !anchor) {
      returningRef.current = false;
      return;
    }
    // Wait for the measured width to update the grid columns before restoring.
    const measured = gridWrapperRef.current?.getBoundingClientRect().width;
    if (measured && measured !== width) return;
    scroll.scrollTop =
      width === anchor.width || !anchor.card.isConnected
        ? anchor.scrollTop
        : anchor.card.offsetTop - anchor.offset;
    if (anchor.card.isConnected) {
      anchor.card
        .querySelector<HTMLButtonElement>("button")
        ?.focus({ preventScroll: true });
    }
    returningRef.current = false;
  }, [active, width]);

  const rememberCard = useCallback(
    (card: HTMLElement) => {
      const scroll = scrollRef.current;
      if (!scroll) return;
      anchorRef.current = {
        card,
        offset: card.offsetTop - scroll.scrollTop,
        scrollTop: scroll.scrollTop,
        width,
      };
    },
    [width],
  );

  const resetScroll = useCallback(() => {
    anchorRef.current = null;
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
  }, []);

  return { scrollRef, gridWrapperRef, width, rememberCard, resetScroll };
}
