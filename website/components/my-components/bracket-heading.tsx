import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * Hero / section block: a mono `[ KICKER ]` eyebrow (square brackets in the
 * brand `primary` colour) stacked over a display heading. Presentational, no
 * hooks — server component.
 *
 *   as     — heading tag + size ramp: "h1" (hero) | "h2" (section) | "h3"
 *   align  — "left" (default) | "center" (hero)
 *   muted  — dims the kicker label
 *
 * Any hero subcopy `<p>` is a sibling of this block, not a child.
 */

type Level = "h1" | "h2" | "h3";

const HEADING_CLASS: Record<Level, string> = {
  h1: "text-[clamp(36px,6vw,52px)] font-semibold leading-[1.05] tracking-tight text-foreground",
  h2: "text-[clamp(30px,5vw,44px)] font-semibold leading-[1.05] tracking-tight text-foreground",
  h3: "text-[clamp(22px,3vw,28px)] font-semibold leading-[1.1] tracking-tight text-foreground",
};

export function BracketHeading({
  kicker,
  children,
  as = "h2",
  align = "left",
  muted = false,
  className = "",
}: {
  /** bare word for the `[ … ]` eyebrow, e.g. "Articles" */
  kicker: React.ReactNode;
  children: React.ReactNode;
  as?: Level;
  align?: "left" | "center";
  muted?: boolean;
  className?: string;
}) {
  const Tag = as;
  return (
    <div
      className={cn(
        "flex flex-col gap-3",
        align === "center" && "items-center text-center",
        className,
      )}
    >
      <span
        className={cn(
          "font-mono text-[13px] uppercase tracking-wide",
          muted ? "text-muted-foreground" : "text-foreground",
        )}
      >
        <span className="text-primary">[</span> {kicker}{" "}
        <span className="text-primary">]</span>
      </span>
      <Tag className={HEADING_CLASS[as]}>{children}</Tag>
    </div>
  );
}
