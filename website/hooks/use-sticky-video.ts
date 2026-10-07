"use client";

import { useEffect, useRef, useState, type CSSProperties } from "react";

type UseStickyVideoOptions = {
  bottom?: number;
  right?: number;
  width?: number;
};

export function useStickyVideo({
  bottom = 16,
  right = 16,
  width = 280,
}: UseStickyVideoOptions = {}) {
  const spacerRef = useRef<HTMLDivElement>(null);
  const [spacerHeight, setSpacerHeight] = useState<number>();
  const [isFloating, setIsFloating] = useState(false);

  useEffect(() => {
    const spacer = spacerRef.current;
    if (!spacer) return;

    const updateSpacerHeight = () => {
      setSpacerHeight(spacer.getBoundingClientRect().height);
    };

    updateSpacerHeight();
    const resizeObserver = new ResizeObserver(updateSpacerHeight);
    resizeObserver.observe(spacer);

    const intersectionObserver = new IntersectionObserver(
      ([entry]) => {
        setIsFloating(!entry.isIntersecting && entry.boundingClientRect.top < 0);
      },
      { threshold: 0 },
    );
    intersectionObserver.observe(spacer);

    return () => {
      resizeObserver.disconnect();
      intersectionObserver.disconnect();
    };
  }, []);

  const spacerStyle: CSSProperties | undefined = spacerHeight
    ? { height: spacerHeight }
    : undefined;
  const videoStyle: CSSProperties = isFloating
    ? {
        position: "fixed",
        right,
        bottom,
        width: `min(${width}px, calc(100vw - 32px))`,
        zIndex: 40,
      }
    : {
        position: "relative",
        width: "100%",
      };

  return { isFloating, spacerRef, spacerStyle, videoStyle };
}
