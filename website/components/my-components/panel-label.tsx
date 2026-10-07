import type * as React from "react";

/**
 * Small uppercase caption above a panel's content ("Featured", "Spotlight").
 */
export function PanelLabel({ children }: { children: React.ReactNode }) {
  return (
    <p className="pb-2 text-[12px] font-medium uppercase tracking-wide text-muted-foreground">
      {children}
    </p>
  );
}
