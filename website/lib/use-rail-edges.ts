"use client";

import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Tracks whether a horizontally-scrolling element is at its start / end so a
 * carousel can disable its left / right arrow at each extreme. Callback ref so
 * it re-measures when the node mounts; also listens on scroll + resize.
 */
export function useRailEdges<T extends HTMLElement>() {
  const [node, setNode] = useState<T | null>(null);
  const [atStart, setAtStart] = useState(true);
  const [atEnd, setAtEnd] = useState(false);
  const frame = useRef<number | null>(null);

  const measure = useCallback(() => {
    if (!node) return;
    const { scrollLeft, scrollWidth, clientWidth } = node;
    setAtStart(scrollLeft <= 1);
    setAtEnd(scrollLeft + clientWidth >= scrollWidth - 1);
  }, [node]);

  const ref = useCallback((el: T | null) => setNode(el), []);

  useEffect(() => {
    if (!node) return;
    measure();
    const onScroll = () => {
      if (frame.current) cancelAnimationFrame(frame.current);
      frame.current = requestAnimationFrame(measure);
    };
    node.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", measure);
    return () => {
      node.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", measure);
      if (frame.current) cancelAnimationFrame(frame.current);
    };
  }, [node, measure]);

  return { ref, node, atStart, atEnd };
}
