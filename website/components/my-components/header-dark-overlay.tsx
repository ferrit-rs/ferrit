"use client";

import * as React from "react";

/**
 * Header readability aid — fully self-contained.
 *
 * The header is transparent with theme-coloured text, so it can disappear when a
 * contrasting block scrolls under it. `mix-blend-mode` can't fix that (a
 * `position: fixed` header is an isolation boundary, the blend never reaches the
 * page). Instead this watches for any `[data-header-invert]` block crossing the
 * header band and fades in a full-width frosted bar, so the text always sits on
 * the page background. Full width => no half-over / half-under seam.
 *
 * Usage: drop `<HeaderDarkOverlay />` as the FIRST child of the fixed header
 * element (it renders a `position: absolute; inset: 0` layer that must sit
 * under the nav row's `z-index: 1`).
 *
 * Opt out: delete the import and the `<HeaderDarkOverlay />` line, then delete
 * this file. Nothing else in the header references it.
 */
export function HeaderDarkOverlay({
  selector = "[data-header-invert]",
  offset = 80,
}: {
  /** Blocks to treat as contrasting while under the header. */
  selector?: string;
  /** Height of the header band, in px from the viewport top. */
  offset?: number;
}) {
  const [over, setOver] = React.useState(false);

  React.useEffect(() => {
    const check = () => {
      let hit = false;
      for (const el of document.querySelectorAll<HTMLElement>(selector)) {
        const r = el.getBoundingClientRect();
        if (r.top < offset && r.bottom > 0) {
          hit = true;
          break;
        }
      }
      setOver(hit);
    };

    check();
    window.addEventListener("scroll", check, { passive: true });
    window.addEventListener("resize", check);
    return () => {
      window.removeEventListener("scroll", check);
      window.removeEventListener("resize", check);
    };
  }, [selector, offset]);

  return (
    <div
      aria-hidden
      style={{
        position: "absolute",
        inset: 0,
        zIndex: 0,
        pointerEvents: "none",
        opacity: over ? 1 : 0,
        transition: "opacity 0.45s cubic-bezier(0.32, 0.72, 0, 1)",
        background: "color-mix(in oklab, var(--background) 90%, transparent)",
        backdropFilter: "blur(10px)",
        WebkitBackdropFilter: "blur(10px)",
      }}
    />
  );
}
