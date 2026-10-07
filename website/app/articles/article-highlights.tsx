import Link from "next/link";

import type { ArticleSummary } from "@/lib/articles";
import { categoryLabel } from "@/lib/article-categories";
import { ContentCardThumbnail } from "@/components/my-components/content-card-thumbnail";
import { PanelLabel } from "@/components/my-components/panel-label";

/**
 * Server-rendered highlight cards for the `/articles` hub — FeaturedCard (big,
 * one) + SpotlightRow (compact list, four). No click tracking here (matches the
 * grid cards' `article_card_clicked` staying on the filterable listing only).
 */

export { PanelLabel };

const CARD = "border border-border bg-card ring-1 ring-foreground/5";

export function FeaturedCard({ article }: { article: ArticleSummary }) {
  return (
    <Link
      href={article.url}
      className={`group block rounded-2xl p-2 transition-transform duration-200 hover:-translate-y-0.5 ${CARD}`}
    >
      <div className="relative aspect-[1200/630] w-full overflow-hidden rounded-xl bg-muted">
        <ContentCardThumbnail
          title={article.thumbnailLabel}
          image={article.image}
          size="feature"
          priority
        />
      </div>
      <div className="grid gap-4 p-4 sm:grid-cols-2">
        <div className="flex flex-col gap-2">
          <span className="w-fit rounded-md border border-border bg-muted px-1.5 py-px text-[12px] font-medium uppercase tracking-wide text-muted-foreground">
            {categoryLabel(article.category)}
          </span>
          <h3 className="text-[26px] font-semibold leading-[1.12] tracking-tight text-foreground">
            {article.title}
          </h3>
        </div>
        <p className="line-clamp-4 self-center text-[15px] leading-[1.5] text-muted-foreground">
          {article.description}
        </p>
      </div>
    </Link>
  );
}

export function SpotlightRow({ article }: { article: ArticleSummary }) {
  return (
    <Link
      href={article.url}
      className={`grid grid-cols-[128px_minmax(0,1fr)] items-stretch gap-0 rounded-2xl p-2 transition-transform duration-200 hover:-translate-y-0.5 ${CARD}`}
    >
      <div className="relative aspect-[16/10] overflow-hidden rounded-xl bg-muted">
        <ContentCardThumbnail
          title={article.thumbnailLabel}
          image={article.image}
          size="spot"
        />
      </div>
      <div className="flex flex-col justify-start gap-1.5 pl-4 pr-2 pt-1">
        <span className="w-fit rounded-md border border-border bg-muted px-1.5 py-px text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          {categoryLabel(article.category)}
        </span>
        <h3 className="line-clamp-2 text-[15px] font-semibold leading-[1.3] tracking-tight text-foreground">
          {article.title}
        </h3>
      </div>
    </Link>
  );
}
