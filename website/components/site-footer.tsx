"use client";

import Link from "next/link";
import { ArrowUpRight, Globe } from "lucide-react";

import { capture, EVENTS } from "@/lib/posthog";
import { siteConfig } from "@/lib/site";

type FooterLink = { label: string; href: string };

const SOCIAL_ICON_PATHS = {
  GitHub:
    "M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.65 7.65 0 0 1 2-.27c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z",
  LinkedIn:
    "M14.9646 2H3.03152C2.75743 2 2.49456 2.10888 2.30074 2.3027C2.10693 2.49651 1.99805 2.75938 1.99805 3.03347V14.9665C1.99805 15.2406 2.10693 15.5035 2.30074 15.6973C2.49456 15.8911 2.75743 16 3.03152 16H14.9646C15.2387 16 15.5015 15.8911 15.6973 15.6973C15.8892 15.5035 15.998 15.2406 15.998 14.9665V3.03347C15.998 2.75938 15.8892 2.49651 15.6954 2.3027C15.5015 2.10888 15.2387 2 14.9646 2ZM6.17082 13.9263H4.06597V7.24028H6.17082V13.9263ZM5.11694 6.31375C4.87818 6.31241 4.64517 6.24036 4.44731 6.10672C4.24945 5.97307 4.09562 5.78381 4.00523 5.56282C3.91483 5.34183 3.89192 5.09901 3.9394 4.86502C3.98687 4.63102 4.1026 4.41633 4.27197 4.24804C4.44134 4.07975 4.65677 3.9654 4.89107 3.91943C5.12537 3.87347 5.36803 3.89793 5.58844 3.98974C5.80884 4.08155 5.99711 4.2366 6.12948 4.43531C6.26186 4.63402 6.3324 4.86749 6.33222 5.10625C6.33446 5.2661 6.30451 5.42477 6.24413 5.5728C6.18376 5.72083 6.0942 5.85519 5.98079 5.96787C5.86739 6.08055 5.73246 6.16925 5.58404 6.22867C5.43563 6.28809 5.27677 6.31703 5.11694 6.31375ZM13.9291 13.9321H11.8253V10.2795C11.8253 9.20223 11.3673 8.86972 10.7763 8.86972C10.1521 8.86972 9.53957 9.34027 9.53957 10.3067V13.9321H7.43472V7.24514H9.45886V8.17167H9.48609C9.6893 7.76042 10.401 7.0575 11.487 7.0575C12.6614 7.0575 13.9301 7.75458 13.9301 9.79625L13.9291 13.9321Z",
  X: "M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z",
} as const;

function CreatorLinkIcon({ label }: { label: string }) {
  if (label === "Website") return <Globe className="size-4" strokeWidth={1.8} />;

  const path = SOCIAL_ICON_PATHS[label as keyof typeof SOCIAL_ICON_PATHS];
  if (!path) return <ArrowUpRight className="size-4" />;

  return (
    <svg
      aria-hidden="true"
      className="size-4"
      viewBox={
        label === "GitHub" ? "0 0 16 16" : label === "X" ? "0 0 24 24" : "0 0 18 18"
      }
    >
      <path d={path} fill="currentColor" />
    </svg>
  );
}

function trackFooterLink(link: FooterLink, group: string) {
  capture(EVENTS.FOOTER_LINK_CLICKED, {
    label: link.label,
    href: link.href,
    group,
  });
}

function FooterLinkItem({ link, group }: { link: FooterLink; group: string }) {
  const content = (
    <>
      <span>{link.label}</span>
      <ArrowUpRight className="size-3.5 opacity-0 transition-opacity group-hover:opacity-100" />
    </>
  );
  const className =
    "group inline-flex w-fit items-center gap-1 text-sm text-muted-foreground transition-colors hover:text-foreground";
  const onClick = () => trackFooterLink(link, group);

  if (link.href.startsWith("/")) {
    return (
      <Link href={link.href} className={className} onClick={onClick}>
        {content}
      </Link>
    );
  }

  return (
    <a href={link.href} className={className} onClick={onClick}>
      {content}
    </a>
  );
}

export function SiteFooter() {
  const year = new Date().getFullYear();

  return (
    <footer className="border-t border-border/60 bg-background">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-12 px-6 py-16 md:px-8 lg:gap-16">
        <div className="flex flex-col justify-between gap-12 lg:flex-row lg:gap-16">
          <div className="flex max-w-sm flex-col items-start gap-5">
            <Link href="/" className="inline-flex items-center gap-3 text-foreground">
              <span className="flex size-9 items-center justify-center rounded-xl bg-primary text-sm font-semibold text-primary-foreground">
                {siteConfig.name.charAt(0)}
              </span>
              <span className="font-semibold tracking-tight">{siteConfig.name}</span>
            </Link>

            <p className="max-w-xs text-sm leading-6 text-muted-foreground">
              {siteConfig.footer.blurb.replace("{creator}", siteConfig.creator.name)}
            </p>

            <div className="flex items-center gap-1.5">
              {siteConfig.creator.links.map((link) => (
                <a
                  key={link.label}
                  href={link.href}
                  aria-label={link.label}
                  className="flex size-9 items-center justify-center rounded-[0.625rem] bg-foreground/5 p-2 text-foreground transition-colors hover:bg-foreground/10"
                  onClick={() => trackFooterLink(link, "creator")}
                >
                  <CreatorLinkIcon label={link.label} />
                </a>
              ))}
            </div>
          </div>

          <nav
            aria-label="Footer navigation"
            className="grid grid-cols-2 gap-x-12 gap-y-10 sm:grid-cols-3"
          >
            {siteConfig.footer.groups.map((group) => (
              <div key={group.heading} className="flex flex-col gap-4">
                <h2 className="text-sm font-semibold text-foreground">{group.heading}</h2>
                <div className="flex flex-col items-start gap-3">
                  {group.links.map((link) => (
                    <FooterLinkItem key={link.label} link={link} group={group.heading} />
                  ))}
                </div>
              </div>
            ))}
          </nav>
        </div>

        <div className="h-px w-full bg-[repeating-linear-gradient(to_right,transparent_0,transparent_0.35rem,color-mix(in_oklab,currentColor_12%,transparent)_0.35rem,color-mix(in_oklab,currentColor_12%,transparent)_0.7rem)]" />

        <div className="text-sm text-muted-foreground">
          <p>
            <strong className="font-semibold text-foreground">©</strong> {year}{" "}
            {siteConfig.name}.
            <span className="hidden sm:inline"> All rights reserved.</span>
          </p>
        </div>
      </div>
    </footer>
  );
}
