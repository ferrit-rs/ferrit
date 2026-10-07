"use client";

import { Button } from "@/components/ui/button";

export default function ErrorPage({
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <div className="mx-auto flex w-full max-w-4xl flex-1 flex-col items-start justify-center gap-6 px-4 py-24">
      <p className="text-sm font-medium text-muted-foreground">Error</p>
      <h1 className="max-w-xl text-4xl font-semibold tracking-tight sm:text-5xl">
        Something went wrong.
      </h1>
      <p className="max-w-xl text-lg text-muted-foreground">
        An unexpected error occurred. Try again, or head back home.
      </p>
      <Button size="lg" onClick={() => reset()}>
        Try again
      </Button>
    </div>
  );
}
