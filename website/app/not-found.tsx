"use client";

import Link from "next/link";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { capture, EVENTS } from "@/lib/posthog";

export default function NotFound() {
  useEffect(() => {
    capture(EVENTS.PAGE_NOT_FOUND, { path: window.location.pathname });
  }, []);

  return (
    <>
      <style>{`
        body:has([data-not-found-page]) .rfy-root { display: none; }
      `}</style>
      <div
        data-not-found-page
        className="-mt-20 mx-auto flex w-full max-w-4xl flex-1 flex-col items-start justify-center gap-6 px-4 py-24"
      >
        <p className="text-sm font-medium text-muted-foreground">404</p>
        <h1 className="max-w-xl text-4xl font-semibold tracking-tight sm:text-5xl">
          Page not found.
        </h1>
        <p className="max-w-xl text-lg text-muted-foreground">
          The page you're looking for doesn't exist or was moved.
        </p>
        <div className="flex flex-wrap gap-3">
          <Button size="lg" nativeButton={false} render={<Link href="/" />}>
            Back home
          </Button>
          <Button
            size="lg"
            variant="outline"
            nativeButton={false}
            render={<Link href="/articles" />}
          >
            Read the articles
          </Button>
        </div>
      </div>
    </>
  );
}
