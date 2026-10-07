"use client";

import { useEffect } from "react";
import { usePathname } from "next/navigation";
import { capture, EVENTS, slugFromPath } from "@/lib/posthog";

export function ArticleTracking() {
  const pathname = usePathname();
  const slug = slugFromPath(pathname);

  useEffect(() => {
    const startTs = Date.now();
    const fired = new Set<number>();
    const milestoneTs: Record<number, number> = {};
    let maxDepth = 0;
    let exited = false;

    function onScroll() {
      const pct =
        ((window.scrollY + window.innerHeight) / document.documentElement.scrollHeight) *
        100;
      maxDepth = Math.max(maxDepth, Math.round(pct));
      for (const depth of [25, 50, 75, 100]) {
        if (pct >= depth && !fired.has(depth)) {
          fired.add(depth);
          const now = Date.now();
          const prev = depth - 25;
          const props: Record<string, unknown> = { article_slug: slug, depth };
          if (prev > 0 && milestoneTs[prev]) {
            props.seconds_in_quartile = Math.round((now - milestoneTs[prev]) / 1000);
          }
          milestoneTs[depth] = now;
          capture(EVENTS.ARTICLE_SCROLL_DEPTH, props);
        }
      }
    }

    function onClick(e: MouseEvent) {
      const anchor = (e.target as HTMLElement)?.closest(
        "a[href]",
      ) as HTMLAnchorElement | null;
      if (!anchor) return;
      const href = anchor.getAttribute("href") ?? "";

      if (href.startsWith("http")) {
        let toDomain = "";
        try {
          toDomain = new URL(href).hostname;
        } catch {
          return;
        }
        if (toDomain === window.location.hostname) return;
        capture(EVENTS.ARTICLE_OUTBOUND_LINK_CLICKED, {
          from_slug: slug,
          to_domain: toDomain,
          to_href: href,
        });
        return;
      }

      if (href.startsWith("/articles/") || href.startsWith("/tags/")) {
        capture(EVENTS.ARTICLE_INTERNAL_LINK_CLICKED, { from_slug: slug, to_href: href });
      }
    }

    // Single time-on-page event, fired once by whichever trigger fires first.
    // pagehide covers tab close/navigation; visibilitychange-hidden catches cases
    // where pagehide doesn't fire (some mobile backgrounding).
    function onExit() {
      if (exited) return;
      exited = true;
      const seconds = Math.round((Date.now() - startTs) / 1000);
      if (seconds > 0) {
        capture(EVENTS.ARTICLE_TIME_ON_PAGE, {
          article_slug: slug,
          seconds,
          max_scroll_depth: maxDepth,
        });
      }
    }
    function onVisibilityChange() {
      if (document.visibilityState === "hidden") onExit();
    }

    window.addEventListener("scroll", onScroll, { passive: true });
    document.addEventListener("click", onClick, true);
    window.addEventListener("pagehide", onExit);
    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      window.removeEventListener("scroll", onScroll);
      document.removeEventListener("click", onClick, true);
      window.removeEventListener("pagehide", onExit);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      // Fires when leaving via SPA navigation (e.g. clicking to another
      // article), since pagehide/visibilitychange don't trigger in that case.
      onExit();
    };
  }, [slug]);

  return null;
}
