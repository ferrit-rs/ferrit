"use client";

import { useMemo, useState } from "react";
import { Search } from "lucide-react";

import type { ArticleSummary } from "@/lib/articles";
import type { ArticleCategory } from "@/lib/article-categories";
import { capture, EVENTS } from "@/lib/posthog";
import { Chips, type ChipItem } from "@/components/my-components/chips";
import { ArticlePosterCard } from "@/components/my-components/article-poster-card";

/**
 * Client half of the `/articles` surface — sidebar (search + CATEGORIES chips +
 * reset + count) beside a 3-col card grid with "Load more". Pure client-side
 * filtering over the full list handed down by the server page. Chips are
 * multi-select (OR); search matches title / description on top.
 *
 * PostHog: `article_card_clicked` (in the card), `article_category_load_more`.
 */

export type CategoryChip = ChipItem<ArticleCategory>;

const PAGE_SIZE = 9;

export function ArticlesListing({
  articles,
  chips,
  initialCategory = null,
}: {
  articles: ArticleSummary[];
  chips: CategoryChip[];
  /** seeds the selected chip set from `?category=<key>`; null when absent */
  initialCategory?: ArticleCategory | null;
}) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<ArticleCategory>>(
    () => new Set(initialCategory ? [initialCategory] : []),
  );
  const [visible, setVisible] = useState(PAGE_SIZE);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return articles.filter((a) => {
      if (selected.size > 0 && !selected.has(a.category)) return false;
      if (!q) return true;
      return a.title.toLowerCase().includes(q) || a.description.toLowerCase().includes(q);
    });
  }, [articles, query, selected]);

  const shown = filtered.slice(0, visible);
  const hasMore = visible < filtered.length;
  const isFiltering = query.trim().length > 0 || selected.size > 0;
  const eventCategory = selected.size > 0 ? [...selected].sort().join(",") : "all";

  function pickCategory(key: ArticleCategory) {
    setSelected((cur) => {
      const next = new Set(cur);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
    setVisible(PAGE_SIZE);
  }

  function reset() {
    setQuery("");
    setSelected(new Set());
    setVisible(PAGE_SIZE);
  }

  function loadMore() {
    const next = Math.min(visible + PAGE_SIZE, filtered.length);
    setVisible(next);
    capture(EVENTS.ARTICLE_CATEGORY_LOAD_MORE, {
      category: eventCategory,
      visible_after: next,
    });
  }

  return (
    <div className="mt-16 grid gap-8 lg:grid-cols-[260px_minmax(0,1fr)]">
      <aside className="flex h-max flex-col gap-6 lg:sticky lg:top-24">
        <div className="relative flex items-center rounded-2xl border border-border bg-card">
          <Search
            className="pointer-events-none absolute left-3 size-4 text-muted-foreground"
            aria-hidden
          />
          <input
            type="search"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setVisible(PAGE_SIZE);
            }}
            placeholder="Search articles"
            className="h-[42px] w-full rounded-2xl bg-transparent pl-[38px] pr-4 text-[14px] text-foreground outline-none placeholder:text-muted-foreground"
          />
        </div>

        <div className="flex flex-col gap-3">
          <p className="border-b border-border pb-2 text-[12px] font-medium uppercase tracking-wide text-muted-foreground">
            Categories
          </p>
          <Chips items={chips} selected={selected} onToggle={pickCategory} />
        </div>

        <div className="flex items-center justify-between">
          <button
            type="button"
            onClick={reset}
            disabled={!isFiltering}
            className="text-[13px] font-medium text-muted-foreground underline-offset-2 hover:underline disabled:opacity-40 disabled:no-underline"
          >
            Reset filters
          </button>
          <span className="text-[13px] tabular-nums text-muted-foreground">
            {filtered.length} of {articles.length}
          </span>
        </div>
      </aside>

      <div className="flex flex-col gap-8">
        {shown.length > 0 ? (
          <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
            {shown.map((a, i) => (
              <ArticlePosterCard
                key={a.slug}
                article={a}
                eventCategory={eventCategory}
                position={i + 1}
              />
            ))}
          </div>
        ) : (
          <p className="py-16 text-center text-[15px] text-muted-foreground">
            No articles match that filter.
          </p>
        )}

        {hasMore ? (
          <div className="flex justify-center">
            <button
              type="button"
              onClick={loadMore}
              className="rounded-lg border border-border bg-card px-4 py-1.5 text-[14px] font-medium text-foreground transition-colors hover:bg-muted"
            >
              Load more
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
