export default function Loading() {
  return (
    <div className="mx-auto max-w-[1200px] px-6 pb-24 pt-16">
      <header className="grid gap-10 md:grid-cols-[minmax(0,1fr)_auto] md:items-start">
        <div className="flex flex-col gap-6">
          <div className="flex gap-2">
            <div className="h-6 w-20 animate-pulse rounded-full bg-muted" />
            <div className="h-6 w-24 animate-pulse rounded-full bg-muted" />
          </div>
          <div className="flex flex-col gap-3">
            <div className="h-9 w-full animate-pulse rounded bg-muted" />
            <div className="h-9 w-2/3 animate-pulse rounded bg-muted" />
          </div>
          <div className="flex items-center gap-3">
            <div className="size-11 animate-pulse rounded-full bg-muted" />
            <div className="h-4 w-32 animate-pulse rounded bg-muted" />
          </div>
        </div>
        <div className="aspect-[1200/675] w-full animate-pulse rounded-[16px] bg-muted md:w-[420px]" />
      </header>

      <div className="mt-16 grid gap-12 lg:grid-cols-[262px_minmax(0,1fr)]">
        <div className="hidden flex-col gap-2 lg:flex">
          <div className="h-4 w-40 animate-pulse rounded bg-muted" />
          <div className="h-4 w-32 animate-pulse rounded bg-muted" />
          <div className="h-4 w-36 animate-pulse rounded bg-muted" />
        </div>
        <div className="flex flex-col gap-4">
          <div className="h-4 w-full animate-pulse rounded bg-muted" />
          <div className="h-4 w-full animate-pulse rounded bg-muted" />
          <div className="h-4 w-2/3 animate-pulse rounded bg-muted" />
          <div className="h-4 w-full animate-pulse rounded bg-muted" />
          <div className="h-4 w-3/4 animate-pulse rounded bg-muted" />
        </div>
      </div>
    </div>
  );
}
