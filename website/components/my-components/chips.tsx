"use client";

import { cn } from "@/lib/utils";

/**
 * Multi-select filter chips. Presentational only: the caller owns the
 * selection Set and toggles it. Generic over the key type so a closed enum
 * keeps its literal type end-to-end.
 */

export type ChipItem<T extends string = string> = {
  key: T;
  label: string;
  count?: number;
};

export function Chips<T extends string>({
  items,
  selected,
  onToggle,
  className,
}: {
  items: ChipItem<T>[];
  selected: ReadonlySet<T>;
  onToggle: (key: T) => void;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-wrap gap-1.5", className)}>
      {items.map((c) => {
        const on = selected.has(c.key);
        return (
          <button
            key={c.key}
            type="button"
            onClick={() => onToggle(c.key)}
            aria-pressed={on}
            className={cn(
              "rounded-full border px-3 py-1 text-[12px] font-medium uppercase tracking-wide transition-colors",
              on
                ? "border-transparent bg-primary text-primary-foreground"
                : "border-border bg-muted text-muted-foreground hover:bg-muted/70",
            )}
          >
            {c.label}
            {typeof c.count === "number" ? (
              <span className="ml-1 tabular-nums opacity-60">{c.count}</span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
