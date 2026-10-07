"use client";

import { BookOpen, Compass, GitCompare, type LucideIcon } from "lucide-react";

import {
  HubCarousel,
  type HubCarouselItem,
} from "@/components/my-components/hub-carousel";
import type { ArticleCategory } from "@/lib/article-categories";

/**
 * "Browse Categories" rail for the `/articles` hub. Thin wrapper over the
 * shared <HubCarousel>: resolves the closed `ArticleCategory` enum to a lucide
 * icon here (client side) and links each tile to `/articles/category/[slug]`.
 */

const ICONS: Record<ArticleCategory, LucideIcon> = {
  guides: BookOpen,
  comparisons: GitCompare,
  career: Compass,
};

export type CategoryCard = {
  key: ArticleCategory;
  label: string;
  blurb: string;
  count: number;
};

export function CategoryCarousel({ categories }: { categories: CategoryCard[] }) {
  const items: HubCarouselItem[] = categories.map((c) => ({
    key: c.key,
    label: c.label,
    blurb: c.blurb,
    count: c.count,
    href: `/articles/category/${c.key}`,
    Icon: ICONS[c.key],
  }));

  return <HubCarousel kicker="All categories" title="Browse Categories" items={items} />;
}
