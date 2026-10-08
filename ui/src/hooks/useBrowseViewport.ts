import { useCallback, useLayoutEffect, useRef, useState } from "react";

/** Keep the current list's layout and selected card position across detail views. */
export function useBrowseViewport(active: boolean, documentScroll = false) {
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
  const getScrollElement = useCallback(
    () => (documentScroll ? document.scrollingElement : scrollRef.current),
    [documentScroll],
  );

  useLayoutEffect(() => {
    if (documentScroll && document.scrollingElement) {
      document.scrollingElement.scrollTop = 0;
    }
  }, [documentScroll]);

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
    const scroll = getScrollElement();
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
        : (documentScroll
            ? anchor.card.getBoundingClientRect().top + scroll.scrollTop
            : anchor.card.offsetTop) - anchor.offset;
    if (anchor.card.isConnected) {
      anchor.card
        .querySelector<HTMLButtonElement>("button")
        ?.focus({ preventScroll: true });
    }
    returningRef.current = false;
  }, [active, width, documentScroll, getScrollElement]);

  const rememberCard = useCallback(
    (card: HTMLElement) => {
      const scroll = getScrollElement();
      if (!scroll) return;
      anchorRef.current = {
        card,
        offset: documentScroll
          ? card.getBoundingClientRect().top
          : card.offsetTop - scroll.scrollTop,
        scrollTop: scroll.scrollTop,
        width,
      };
    },
    [width, documentScroll, getScrollElement],
  );

  const resetScroll = useCallback(() => {
    anchorRef.current = null;
    const scroll = getScrollElement();
    if (scroll) scroll.scrollTop = 0;
  }, [getScrollElement]);

  return { scrollRef, gridWrapperRef, width, rememberCard, resetScroll };
}
