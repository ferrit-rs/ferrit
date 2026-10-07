import Link from "next/link";

import { Button } from "@/components/ui/button";

/**
 * Closing-CTA block — centered heading + primary button. Presentational, no
 * client hooks, so it stays a server component; each caller wraps it for
 * analytics and passes `onCtaClick` (fires before navigation).
 *
 * Copy defaults are written as fill-in prompts (like the home page), so the
 * layout is visible and every slot is an obvious swap for real content.
 * Defaults point at the template's `/contact`.
 */
export function ClosingCta({
  heading = "Closing call to action headline goes here?",
  body = "One or two lines restating the promise of the page and telling the reader exactly what happens when they hit the button.",
  ctaLabel = "Primary action label",
  ctaHref = "/contact",
  ctaClassName,
  onCtaClick,
}: {
  heading?: string;
  body?: string;
  ctaLabel?: string;
  ctaHref?: string;
  ctaClassName?: string;
  onCtaClick?: () => void;
}) {
  return (
    <section className="mt-24 flex flex-col items-center gap-5 rounded-3xl border border-border bg-card px-6 py-16 text-center ring-1 ring-foreground/5">
      <h2 className="max-w-[24ch] text-[clamp(28px,4vw,40px)] font-semibold leading-tight tracking-tight text-foreground">
        {heading}
      </h2>
      <p className="max-w-[46ch] text-[16px] leading-[1.5] text-muted-foreground">
        {body}
      </p>
      <Button
        className={ctaClassName}
        size="lg"
        nativeButton={false}
        onClick={onCtaClick}
        render={<Link href={ctaHref} />}
      >
        {ctaLabel}
      </Button>
    </section>
  );
}
