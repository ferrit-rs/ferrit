import type { ComponentPropsWithoutRef } from "react";

import { cn } from "@/lib/utils";

/**
 * Rich-text wrapper for a rendered MDX article body. Every rule is a
 * descendant selector so the MDX components stay untouched — drop the body
 * inside `<Typeset>` and the prose picks up spacing, headings, tables, code,
 * blockquotes and links. Theme-token colours only, so it flips light / dark.
 */
export function Typeset({ className, ...props }: ComponentPropsWithoutRef<"div">) {
  return (
    <div
      className={cn(
        "max-w-none text-[0.95rem] leading-7 text-foreground",
        "[&_a]:font-medium [&_a]:text-primary [&_a]:underline-offset-4 hover:[&_a]:underline",
        "[&_blockquote]:border-l-2 [&_blockquote]:border-primary/30 [&_blockquote]:bg-muted/40 [&_blockquote]:pl-4 [&_blockquote]:pr-4 [&_blockquote]:text-muted-foreground",
        "[&_code]:rounded-md [&_code]:bg-muted [&_code]:px-1.5 [&_code]:py-0.5 [&_code]:text-[0.9em] [&_code]:text-foreground",
        "[&_h1]:mt-10 [&_h1]:mb-4 [&_h1]:text-4xl [&_h1]:font-bold [&_h1]:tracking-tight [&_h1]:text-foreground",
        "[&_h2]:mt-8 [&_h2]:mb-4 [&_h2]:text-xl [&_h2]:font-semibold [&_h2]:text-foreground [&_h2]:scroll-m-24",
        "[&_h3]:mt-6 [&_h3]:mb-2 [&_h3]:text-lg [&_h3]:font-medium [&_h3]:text-foreground [&_h3]:scroll-m-24",
        "[&_h4]:mt-6 [&_h4]:mb-2 [&_h4]:text-base [&_h4]:font-medium [&_h4]:text-foreground",
        "[&_hr]:my-8 [&_hr]:border-border",
        "[&_img]:rounded-2xl [&_img]:border [&_img]:border-border [&_img]:shadow-sm",
        "[&_li]:my-2 [&_li]:leading-relaxed",
        "[&_ol]:my-4 [&_ol]:list-decimal [&_ol]:pl-6",
        "[&_p]:my-4 [&_p]:leading-relaxed",
        "[&_strong]:font-semibold [&_strong]:text-foreground",
        "[&_table]:w-full [&_table]:border-collapse [&_table]:text-sm",
        "[&_thead_th]:border-b [&_thead_th]:border-border [&_thead_th]:bg-muted-foreground/[0.08] [&_thead_th]:px-4 [&_thead_th]:py-2.5 [&_thead_th]:text-left [&_thead_th]:font-semibold [&_thead_th]:text-foreground",
        "[&_tbody_tr]:border-b [&_tbody_tr]:border-border [&_tbody_tr:last-child]:border-0 [&_tbody_tr:hover]:bg-muted-foreground/[0.05]",
        "[&_td]:px-4 [&_td]:py-2.5 [&_td]:align-top",
        "[&_tbody_td:first-child]:font-medium [&_tbody_td:first-child]:text-foreground",
        "[&_ul]:my-4 [&_ul]:list-disc [&_ul]:pl-6",
        className,
      )}
      {...props}
    />
  );
}
