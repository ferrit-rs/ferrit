"use client";

import Link from "next/link";
import { ArrowLeft, ArrowRight, type LucideIcon } from "lucide-react";

import { BracketHeading } from "@/components/my-components/bracket-heading";
import { useRailEdges } from "@/lib/use-rail-edges";

/**
 * "Browse X" horizontal rail. Native `overflow-x-auto` (touch swipe + trackpad
 * for free) with proximity snap so a fast flick keeps momentum. The taxonomy
 * wrapper (`CategoryCarousel`) owns the icon-by-key map and shapes items —
 * everything visual lives here. `useRailEdges` disables the arrow at each
 * scroll extreme.
 */

export type HubCarouselItem = {
  key: string;
  label: string;
  blurb: string;
  count: number;
  href: string;
  Icon: LucideIcon;
};

const ARROW_CLASS =
  "flex size-10 items-center justify-center rounded-full border border-border bg-card text-foreground transition-[opacity,background-color] duration-300 hover:bg-muted disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-card";

export function HubCarousel({
  kicker,
  title,
  items,
}: {
  kicker: string;
  title: string;
  items: HubCarouselItem[];
}) {
  const { ref: railRef, node: rail, atStart, atEnd } = useRailEdges<HTMLDivElement>();

  function nudge(dir: 1 | -1) {
    rail?.scrollBy({ left: dir * 384, behavior: "smooth" });
  }

  return (
    <section className="mt-24">
      <div className="mb-8 flex items-end justify-between gap-4">
        <BracketHeading as="h2" muted kicker={kicker}>
          {title}
        </BracketHeading>
        <div className="hidden gap-2 sm:flex">
          <button
            type="button"
            onClick={() => nudge(-1)}
            disabled={atStart}
            aria-label={`Scroll ${title} left`}
            className={ARROW_CLASS}
          >
            <ArrowLeft className="size-4" aria-hidden />
          </button>
          <button
            type="button"
            onClick={() => nudge(1)}
            disabled={atEnd}
            aria-label={`Scroll ${title} right`}
            className={ARROW_CLASS}
          >
            <ArrowRight className="size-4" aria-hidden />
          </button>
        </div>
      </div>

      <div
        ref={railRef}
        className="flex snap-x snap-proximity gap-4 overflow-x-auto overscroll-x-contain pb-4 [-webkit-overflow-scrolling:touch] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      >
        {items.map(({ key, label, blurb, count, href, Icon }) => (
          <Link
            key={key}
            href={href}
            className="flex w-[300px] shrink-0 snap-start flex-col gap-5 rounded-3xl border border-border bg-card p-6 ring-1 ring-foreground/5 transition-transform duration-200 hover:-translate-y-0.5"
          >
            <span className="flex size-11 items-center justify-center rounded-2xl border border-border bg-muted text-foreground">
              <Icon className="size-5" aria-hidden />
            </span>
            <div className="flex flex-col gap-2">
              <h3 className="text-[20px] font-semibold tracking-tight text-foreground">
                {label}
                <span className="ml-1.5 text-[14px] font-normal tabular-nums text-muted-foreground">
                  {count}
                </span>
              </h3>
              <p className="text-[15px] leading-[1.45] text-muted-foreground">{blurb}</p>
            </div>
          </Link>
        ))}
      </div>
    </section>
  );
}
