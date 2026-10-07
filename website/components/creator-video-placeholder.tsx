import { Play } from "lucide-react";

export function CreatorVideoPlaceholder() {
  return (
    <div
      aria-label="Creator video placeholder"
      className="relative aspect-video overflow-hidden rounded-2xl border border-border bg-[linear-gradient(135deg,var(--muted),var(--card))] ring-1 ring-foreground/5"
      data-video-placeholder="creator-vsl"
      role="img"
    >
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_70%_25%,color-mix(in_oklab,var(--primary)_14%,transparent),transparent_34%),linear-gradient(135deg,transparent_35%,color-mix(in_oklab,var(--foreground)_4%,transparent))]" />
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-4 text-center">
        <span className="flex size-16 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-lg shadow-primary/20">
          <Play className="ml-1 size-7 fill-current" />
        </span>
        <div className="flex flex-col gap-1">
          <span className="font-semibold text-foreground">Creator video placeholder</span>
          <span className="text-sm text-muted-foreground">Replace with personal VSL</span>
        </div>
      </div>
    </div>
  );
}
