import Image from "next/image";

/**
 * Content card thumbnail. Most template articles have no hand-made hero art
 * (they point at `/images/default-article.webp`), so for those we render a
 * dependency-free DOM panel — inverted `bg-foreground` ground, the label
 * centered in bold `text-background`. No generated PNG, no extra request, and
 * it flips cleanly with the light / dark theme.
 *
 * A real `image` (anything not `default-article`) always wins and renders as a
 * normal optimised <Image>.
 *
 * Fills its positioned parent (parent must be `relative` + set the aspect +
 * `overflow-hidden` + rounding).
 *
 * The dark DOM panel carries `data-header-invert` so `HeaderDarkOverlay` fades
 * the frosted bar in whenever a card scrolls under the transparent header (the
 * real-<Image> branch does not — a hand-made hero can be any brightness).
 */

type Size = "feature" | "grid" | "spot";

const SIZES_BY_SIZE: Record<Size, string> = {
  feature: "(min-width: 1024px) 620px, 100vw",
  grid: "(min-width: 1280px) 280px, (min-width: 640px) 45vw, 100vw",
  spot: "128px",
};

const FRAME_LAYOUT: Record<Size, { pad: string; title: string }> = {
  feature: { pad: "p-10", title: "text-[18px] leading-[1.3]" },
  grid: { pad: "p-6", title: "text-[12px] leading-[1.3]" },
  spot: { pad: "p-3", title: "text-[8px] leading-[1.3]" },
};

function hasRealImage(image?: string): image is string {
  return !!image && !image.includes("default-article");
}

export function ContentCardThumbnail({
  title,
  image,
  size,
  priority,
}: {
  title: string;
  image?: string;
  size: Size;
  priority?: boolean;
}) {
  if (hasRealImage(image)) {
    return (
      <Image
        src={image}
        alt={title}
        fill
        sizes={SIZES_BY_SIZE[size]}
        className="object-cover"
        priority={priority}
      />
    );
  }

  const l = FRAME_LAYOUT[size];

  return (
    <div
      aria-hidden
      data-header-invert
      className={`content-card-thumbnail absolute inset-0 flex items-center justify-center text-center ${l.pad}`}
    >
      <span
        className={`font-semibold tracking-tight text-background ${l.title}`}
        style={{
          display: "-webkit-box",
          WebkitLineClamp: 3,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
        }}
      >
        {title}
      </span>
    </div>
  );
}
