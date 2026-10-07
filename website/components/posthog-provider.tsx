"use client";

import { Suspense, useEffect } from "react";
import { usePathname, useSearchParams } from "next/navigation";
import posthog from "posthog-js";
import { isLikelyBot } from "@/lib/posthog";

const KEY = process.env.NEXT_PUBLIC_POSTHOG_KEY;
const HOST = process.env.NEXT_PUBLIC_POSTHOG_HOST ?? "https://us.i.posthog.com";

// First pageview waits for a real engagement signal (scroll/move/click) or an
// 8s dwell fallback, so a bot that fetches one URL and leaves sends nothing.
// Client-side navigations after that already imply engagement (a click fired them).
let engaged = false;

function firePageview(url: string) {
  posthog.capture("$pageview", { $current_url: url });
}

function PageViewTracker() {
  const pathname = usePathname();
  const searchParams = useSearchParams();

  useEffect(() => {
    if (!KEY || isLikelyBot()) return;
    const query = searchParams.toString();
    const url = query ? `${pathname}?${query}` : pathname;

    if (engaged) {
      firePageview(url);
      return;
    }

    let fired = false;
    function qualify() {
      if (fired) return;
      fired = true;
      engaged = true;
      clearTimeout(timeout);
      window.removeEventListener("scroll", qualify);
      window.removeEventListener("mousemove", qualify);
      window.removeEventListener("click", qualify);
      firePageview(url);
    }
    const timeout = setTimeout(qualify, 8000);
    window.addEventListener("scroll", qualify, { passive: true, once: true });
    window.addEventListener("mousemove", qualify, { once: true });
    window.addEventListener("click", qualify, { once: true });

    return () => {
      clearTimeout(timeout);
      window.removeEventListener("scroll", qualify);
      window.removeEventListener("mousemove", qualify);
      window.removeEventListener("click", qualify);
    };
  }, [pathname, searchParams]);

  return null;
}

export function PostHogProvider({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    if (!KEY || posthog.__loaded) return;
    posthog.init(KEY, {
      api_host: HOST,
      capture_pageview: false,
      capture_pageleave: true,
      person_profiles: "identified_only",
      loaded: (ph) => {
        if (isLikelyBot()) ph.opt_out_capturing();
      },
    });
  }, []);

  return (
    <>
      <Suspense fallback={null}>
        <PageViewTracker />
      </Suspense>
      {children}
    </>
  );
}
