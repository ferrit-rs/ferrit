"use client";

/**
 * site-header.tsx. The trendtrack.io / Rustify header design, refitted for this
 * template.
 *
 * Mechanics:
 * - desktop mega menu driven by radix <NavigationMenu> (viewport={false}) so the
 *   open state is persistent across the trigger -> panel gap
 * - navlink text-swap, pill ::after, scroll backdrop (`header_bg`), slide-down
 *   mobile panel, reduced-motion guard
 * - every colour token maps onto the shadcn theme vars (--foreground, --popover,
 *   --muted-foreground, --border, --accent, --primary...) so it tracks
 *   light/dark. The <ModeToggle> lives in the right-hand button row.
 *
 * Ferrit project navigation. Keep project proof first, then Richard's context.
 *
 * PostHog: one delegated `header_link_clicked` fires from a single handler on
 * `.rfy-root` uses `{ label, href, location, group?, section?, source }` where
 * location ∈ logo | mega | plain | cta and source ∈ desktop | mobile.
 *
 * Everything is namespaced under `.rfy-root`.
 */

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { BookOpen, Briefcase, Compass, Globe, Menu, UserRound, X } from "lucide-react";

import {
  NavigationMenu,
  NavigationMenuContent,
  NavigationMenuItem,
  NavigationMenuLink,
  NavigationMenuList,
  NavigationMenuTrigger,
} from "@/components/ui/navigation-menu";
import { cn } from "@/lib/utils";
import { capture, EVENTS } from "@/lib/posthog";
import { siteConfig } from "@/lib/site";
import { ModeToggle } from "@/components/mode-toggle";
import { HeaderDarkOverlay } from "@/components/my-components/header-dark-overlay";

const GITHUB_HREF = siteConfig.links.github;
const PERSONAL_WEBSITE_HREF = siteConfig.links.personalWebsite;
const START_HREF = "/contact";

type MegaLink = {
  title: string;
  description: string;
  href: string;
  icon: typeof BookOpen;
  badge?: string;
};

type MegaSection = {
  heading: string;
  links: MegaLink[];
};

type MegaGroup = {
  label: string;
  sections: MegaSection[];
};

const PROJECT_LINKS: MegaSection[] = [
  {
    heading: "Project",
    links: [
      {
        title: "Project overview",
        description: "The idea, mission, and promise behind this project.",
        href: "/project",
        icon: Compass,
      },
      {
        title: "Design choices",
        description: "The thinking, choices, and details behind the experience.",
        href: "/design",
        icon: Briefcase,
      },
      {
        title: "Articles",
        description: "Notes, ideas, and lessons connected to this project.",
        href: "/articles",
        icon: BookOpen,
      },
    ],
  },
];

const ABOUT_LINKS: MegaSection[] = [
  {
    heading: "About me",
    links: [
      {
        title: "My story",
        description: "Background, journey, and the person behind the work.",
        href: "/about",
        icon: UserRound,
      },
      {
        title: "My work",
        description: "Projects, experience, and things I have built.",
        href: "/work",
        icon: Briefcase,
      },
      {
        title: "Personal website",
        description: "Visit my main website and discover more of my work.",
        href: PERSONAL_WEBSITE_HREF,
        icon: Globe,
      },
    ],
  },
];

const MEGA_GROUPS: MegaGroup[] = [
  { label: "Ferrit", sections: PROJECT_LINKS },
  { label: "About me", sections: ABOUT_LINKS },
];

const PLAIN_LINKS = [
  { label: "My story", href: "/about" },
  { label: "Articles", href: "/articles" },
];

function MegaRow({
  link,
  group,
  section,
}: {
  link: MegaLink;
  group: string;
  section: string;
}) {
  const Icon = link.icon;
  return (
    <NavigationMenuLink asChild>
      <Link
        href={link.href}
        className="nav_mega__link"
        data-rfy-loc="mega"
        data-rfy-label={link.title}
        data-rfy-group={group}
        data-rfy-section={section}
      >
        <span className="nav_menu__icon">
          <Icon size={16} strokeWidth={1.75} />
        </span>
        <span className="nav_mega__copy">
          <span className="nav_mega__title">
            {link.title}
            {link.badge ? <span className="nav_mega__badge">{link.badge}</span> : null}
          </span>
          <span className="nav_mega__desc">{link.description}</span>
        </span>
      </Link>
    </NavigationMenuLink>
  );
}

function NavlinkLabel({ text }: { text: string }) {
  return (
    <span className="navlink_inner">
      <span className="navlink_text">{text}</span>
    </span>
  );
}

function GithubIcon() {
  return (
    <svg
      aria-hidden="true"
      className="size-4 shrink-0"
      fill="none"
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.65 7.65 0 0 1 2-.27c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"
        fill="currentColor"
      />
    </svg>
  );
}

export function SiteHeader() {
  const [scrolled, setScrolled] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 4);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  // One delegated listener for every nav anchor. Desktop mega + plain links,
  // the logo, the CTA buttons, and their mobile-panel twins. Anchors opt in
  // with `data-rfy-loc`; `.rfy-mobile` ancestry decides the `source`. Bound on
  // the root node (not a JSX handler) so the wrapper stays a plain element.
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const onNavClick = (e: MouseEvent) => {
      const a = (e.target as HTMLElement).closest<HTMLAnchorElement>("a[data-rfy-loc]");
      if (!a) return;
      capture(EVENTS.HEADER_LINK_CLICKED, {
        label: a.dataset.rfyLabel ?? a.textContent?.trim() ?? "",
        href: a.getAttribute("href") ?? "",
        location: (a.dataset.rfyLoc ?? "plain") as "logo" | "mega" | "plain" | "cta",
        group: a.dataset.rfyGroup || undefined,
        section: a.dataset.rfySection || undefined,
        source: a.closest(".rfy-mobile") ? "mobile" : "desktop",
      });
    };
    root.addEventListener("click", onNavClick);
    return () => root.removeEventListener("click", onNavClick);
  }, []);

  return (
    <div className="rfy-root" ref={rootRef}>
      <style>{CSS}</style>

      <div
        className="header"
        data-navbar="rfy"
        data-scrolled={scrolled}
        data-nav-status={mobileOpen ? "open" : "closed"}
      >
        <HeaderDarkOverlay />
        <nav className="nav_wrap">
          <div className="nav_inner">
            <div className="nav_left_row">
              <Link
                href="/"
                className="nav_logo"
                aria-label="Home"
                data-rfy-loc="logo"
                data-rfy-label="logo"
              >
                <span className="nav_logo__text">{siteConfig.name}</span>
              </Link>

              {/* Desktop. Radix NavigationMenu drives the persistent mega dropdown. */}
              <NavigationMenu
                viewport={false}
                className="rfy-nav hidden max-w-none md:flex"
              >
                <NavigationMenuList className="nav_menu_links gap-[0.5em]">
                  {MEGA_GROUPS.map((group) => (
                    <NavigationMenuItem key={group.label} className="nav_menu_item">
                      <NavigationMenuTrigger className={cn("navlink", "rfy-trigger")}>
                        <NavlinkLabel text={group.label} />
                      </NavigationMenuTrigger>
                      <NavigationMenuContent className={cn("nav_mega", "rfy-content")}>
                        <div className="nav_mega__wrap">
                          <div className="nav_mega__layout">
                            {group.sections.map((section) => (
                              <div key={section.heading} className="nav_mega__section">
                                <p className="nav_mega__heading">{section.heading}</p>
                                <ul className="nav_mega__list">
                                  {section.links.map((link) => (
                                    <li key={link.title}>
                                      <MegaRow
                                        link={link}
                                        group={group.label}
                                        section={section.heading}
                                      />
                                    </li>
                                  ))}
                                </ul>
                              </div>
                            ))}
                          </div>
                        </div>
                      </NavigationMenuContent>
                    </NavigationMenuItem>
                  ))}

                  {PLAIN_LINKS.map((link) => (
                    <NavigationMenuItem key={link.label} className="nav_menu_item">
                      <NavigationMenuLink
                        asChild
                        className={cn("navlink", "rfy-trigger")}
                      >
                        <Link
                          href={link.href}
                          data-rfy-loc="plain"
                          data-rfy-label={link.label}
                        >
                          <NavlinkLabel text={link.label} />
                        </Link>
                      </NavigationMenuLink>
                    </NavigationMenuItem>
                  ))}
                </NavigationMenuList>
              </NavigationMenu>
            </div>

            <div className="nav_button_row">
              <div className="nav_button_wrap">
                <a
                  href={GITHUB_HREF}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="btn_wrap"
                  data-variant="outline"
                  data-rfy-loc="cta"
                  data-rfy-label="GitHub"
                >
                  <span className="btn_inner">
                    <GithubIcon />
                    <span className="btn_text">
                      <span className="btn_text_span">GitHub</span>
                    </span>
                  </span>
                  <span className="btn_bg" />
                </a>
                <Link
                  href={START_HREF}
                  className="btn_wrap"
                  data-variant="main"
                  data-rfy-loc="cta"
                  data-rfy-label="Contact"
                >
                  <span className="btn_inner">
                    <span className="btn_text">
                      <span className="btn_text_span">Contact</span>
                    </span>
                  </span>
                  <span className="btn_bg" />
                </Link>
              </div>
              <button
                type="button"
                className="menu-button"
                aria-label="toggle menu"
                aria-expanded={mobileOpen}
                onClick={() => setMobileOpen((v) => !v)}
              >
                {mobileOpen ? <X size={18} /> : <Menu size={18} />}
              </button>
              <ModeToggle />
            </div>
          </div>
        </nav>

        {/* Mobile panel. Slide-down. */}
        <div className="rfy-mobile" data-state={mobileOpen ? "open" : "closed"}>
          <div className="rfy-mobile__panel">
            {MEGA_GROUPS.map((group) => (
              <div key={group.label} className="rfy-mobile__group">
                <p className="rfy-mobile__label">{group.label}</p>
                {group.sections.map((section) => (
                  <div key={section.heading} className="rfy-mobile__section">
                    <p className="rfy-mobile__subhead">{section.heading}</p>
                    <div className="rfy-mobile__links">
                      {section.links.map((link) => {
                        const Icon = link.icon;
                        return (
                          <Link
                            key={link.title}
                            href={link.href}
                            className="nav_mega__link"
                            data-rfy-loc="mega"
                            data-rfy-label={link.title}
                            data-rfy-group={group.label}
                            data-rfy-section={section.heading}
                            onClick={() => setMobileOpen(false)}
                          >
                            <span className="nav_menu__icon">
                              <Icon size={16} strokeWidth={1.75} />
                            </span>
                            <span className="nav_mega__copy">
                              <span className="nav_mega__title">
                                {link.title}
                                {link.badge ? (
                                  <span className="nav_mega__badge">{link.badge}</span>
                                ) : null}
                              </span>
                            </span>
                          </Link>
                        );
                      })}
                    </div>
                  </div>
                ))}
              </div>
            ))}
            <div className="rfy-mobile__plain">
              {PLAIN_LINKS.map((link) => (
                <Link
                  key={link.label}
                  href={link.href}
                  data-rfy-loc="plain"
                  data-rfy-label={link.label}
                  onClick={() => setMobileOpen(false)}
                >
                  {link.label}
                </Link>
              ))}
            </div>
            <div className="nav_button_wrap cc-mobile">
              <a
                href={GITHUB_HREF}
                target="_blank"
                rel="noopener noreferrer"
                className="btn_wrap"
                data-variant="outline"
                data-rfy-loc="cta"
                data-rfy-label="GitHub"
              >
                <span className="btn_inner">
                  <GithubIcon />
                  <span className="btn_text">
                    <span className="btn_text_span">GitHub</span>
                  </span>
                </span>
                <span className="btn_bg" />
              </a>
              <Link
                href={START_HREF}
                className="btn_wrap"
                data-variant="main"
                data-rfy-loc="cta"
                data-rfy-label="Contact"
              >
                <span className="btn_inner">
                  <span className="btn_text">
                    <span className="btn_text_span">Contact</span>
                  </span>
                </span>
                <span className="btn_bg" />
              </Link>
            </div>
          </div>
        </div>

        <div className="header_bg" aria-hidden>
          <div />
          <div />
          <div />
          <div />
        </div>
      </div>
    </div>
  );
}

const CSS = `
.rfy-root {
  --smooth-ease: cubic-bezier(0.32, 0.72, 0, 1);
  --duration-s: 0.45s;
  --animation: 0.45s var(--smooth-ease);
  --animation-xs: 0.2s var(--smooth-ease);
  --max-width: 72rem;
  --gr-btn: linear-gradient(135deg, var(--primary) 0%, color-mix(in oklab, var(--primary) 78%, #000) 100%);
  --sh-primary:
    inset 0 1px 0 0 rgba(255, 255, 255, 0.18),
    0 8px 24px -6px color-mix(in oklab, var(--primary) 45%, transparent),
    0 2px 8px -2px color-mix(in oklab, var(--primary) 35%, transparent);
  --sh-glass: inset 0 1px 0 0 rgba(255, 255, 255, 0.06);
  --sh-base:
    0 0 0 1px var(--border),
    0 24px 48px -16px rgba(13, 10, 13, 0.18),
    0 8px 16px -8px rgba(13, 10, 13, 0.1);
}

.rfy-root .header,
.rfy-root .header * { box-sizing: border-box; }

/* ---- header shell -------------------------------------------------------------- */
.rfy-root .header {
  position: fixed;
  inset: 0 0 auto 0;
  z-index: 100;
  width: 100vw;
  padding: 1.5em 2em;
  font-family: inherit;
  font-size: 0.875rem;
  font-weight: 400;
  line-height: 1.4;
  color: var(--foreground);
  transition: padding 0.35s var(--smooth-ease);
}
.rfy-root .header[data-scrolled="true"][data-nav-status="closed"] { padding: 1em 2em; }

.rfy-root .nav_wrap {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  max-width: var(--max-width);
  margin-inline: auto;
}
.rfy-root .nav_inner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}
.rfy-root .nav_left_row { display: flex; align-items: center; gap: 1.5em; }

/* ---- logo -------------------------------------------------------------------- */
.rfy-root .nav_logo {
  display: flex;
  align-items: center;
  gap: 0.5em;
  color: var(--foreground);
  text-decoration: none;
}
.rfy-root .nav_logo__text {
  font-size: 1.2rem;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--foreground);
}

/* ---- NavigationMenu container (radix) ------------------------------------------ */
/* rfy-nav is the shared anchor: every mega panel is positioned against it, so the
   dropdown lands in the exact same spot no matter which trigger is open. Items
   must stay position:static for that to resolve here. */
.rfy-root .rfy-nav { position: relative; }
.rfy-root .nav_menu_links {
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0;
  padding: 0;
  list-style: none;
}
.rfy-root .nav_menu_item { position: static; display: block; margin: 0; }

/* ---- navlink trigger. Override radix/shadcn defaults with higher specificity. -- */
.rfy-root .header .navlink {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5em;
  height: auto;
  padding: 0.428571em 0.857143em;
  font-family: inherit;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--muted-foreground);
  background: transparent;
  border: 0;
  border-radius: 0.857143em;
  cursor: pointer;
  transition: color var(--animation-xs);
}
.rfy-root .header .navlink:hover,
.rfy-root .header .navlink:focus,
.rfy-root .header .navlink[data-state="open"],
.rfy-root .header .navlink[data-popup-open],
.rfy-root .header .navlink[data-open] {
  color: var(--foreground);
  background: transparent;
}

.rfy-root .header .navlink::after {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: color-mix(in oklab, var(--foreground) 7%, transparent);
  opacity: 0;
  transform: scale(0.9);
  transition: transform var(--duration-s) var(--smooth-ease), opacity var(--duration-s) var(--smooth-ease);
  isolation: isolate;
}
.rfy-root .header .navlink:hover::after,
.rfy-root .header .navlink[data-state="open"]::after,
.rfy-root .header .navlink[data-popup-open]::after,
.rfy-root .header .navlink[data-open]::after { opacity: 1; transform: scale(1); }

.rfy-root .navlink_inner {
  position: relative;
  z-index: 1;
  display: block;
  height: 1.5em;
  overflow: hidden;
}
.rfy-root .navlink_text {
  display: block;
  line-height: 1.5;
  --text-distance: 1.5em;
  text-shadow: 0 var(--text-distance) currentColor;
  transition: transform var(--duration-s) var(--smooth-ease);
}
.rfy-root .header .navlink:hover .navlink_text,
.rfy-root .header .navlink[data-state="open"] .navlink_text,
.rfy-root .header .navlink[data-popup-open] .navlink_text,
.rfy-root .header .navlink[data-open] .navlink_text {
  transform: translateY(calc(-1 * var(--text-distance)));
}

/* radix injects its own chevron <svg> after the children */
.rfy-root .header .navlink > svg {
  flex-shrink: 0;
  width: 0.9em;
  height: 0.9em;
  margin: 0;
  opacity: 0.75;
  transition: transform var(--animation-xs);
}
.rfy-root .header .navlink[data-state="open"] > svg,
.rfy-root .header .navlink[data-popup-open] > svg,
.rfy-root .header .navlink[data-open] > svg { transform: rotate(180deg); }

/* ---- mega menu content (radix NavigationMenuContent, viewport=false) ---------- */
.rfy-root .nav_mega.rfy-content {
  position: absolute;
  top: calc(100% + 0.625em);
  left: 0;
  right: auto;
  width: 40rem;
  max-width: calc(100vw - 5em);
  margin: 0;
  padding: 0;
  background: transparent;
  /* neutralize NavigationMenuContent's own border/shadow/radius.
     the visible frame is .nav_mega__wrap */
  border: 0;
  box-shadow: none;
  overflow: visible;
}
.rfy-root .nav_mega__wrap {
  width: 100%;
  color: var(--popover-foreground);
  background-color: var(--popover);
  border-radius: 1.25em;
  box-shadow: var(--sh-base);
  overflow: hidden;
}
.rfy-root .nav_mega__layout {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: start;
  gap: 0.25em 0.75em;
  width: 100%;
  padding: 0.875em;
}
.rfy-root .nav_mega__section { min-width: 0; }
.rfy-root .nav_mega__section:only-child { grid-column: 1 / -1; }
.rfy-root .nav_mega__section:only-child .nav_mega__list {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.1em 0.75em;
}
.rfy-root .nav_mega__heading {
  margin: 0;
  padding: 0.5em 0.75em 0.35em;
  font-size: 0.6875em;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--muted-foreground);
}
.rfy-root .nav_mega__list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.1em;
}
.rfy-root .nav_mega__link {
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.875em;
  width: 100%;
  padding: 0.625em 1.25em 0.625em 0.625em;
  border-radius: 0.875em;
  text-decoration: none;
  color: inherit;
  transition: background-color var(--animation-xs);
}
.rfy-root .nav_mega__link:hover,
.rfy-root .nav_mega__link:focus-visible {
  background-color: var(--accent);
  outline: none;
}
.rfy-root .nav_menu__icon {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 2.5em;
  height: 2.5em;
  padding: 0.5em;
  border-radius: 0.75em;
  color: var(--primary);
  background-color: var(--muted);
  box-shadow: var(--sh-glass), inset 0 0 0 1px var(--border);
  overflow: hidden;
}
.rfy-root .nav_mega__copy { display: flex; flex-direction: column; gap: 0.15em; min-width: 0; }
.rfy-root .nav_mega__title {
  display: flex;
  align-items: center;
  gap: 0.5em;
  font-size: 0.875em;
  font-weight: 500;
  color: var(--foreground);
}
.rfy-root .nav_mega__badge {
  display: inline-flex;
  align-items: center;
  padding: 0.05em 0.5em;
  font-size: 0.6875em;
  font-weight: 600;
  line-height: 1.5;
  color: var(--primary-foreground);
  background: var(--primary);
  border-radius: 999px;
}
.rfy-root .nav_mega__desc {
  font-size: 0.8125em;
  color: var(--muted-foreground);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

/* radix open/close animation */
.rfy-root .nav_mega.rfy-content[data-state="open"],
.rfy-root .nav_mega.rfy-content[data-open] { animation: rfy-in 0.2s var(--smooth-ease); }
.rfy-root .nav_mega.rfy-content[data-state="closed"],
.rfy-root .nav_mega.rfy-content[data-closed] { animation: rfy-out 0.15s var(--smooth-ease); }
@keyframes rfy-in { from { opacity: 0; transform: translateY(0.4em); } to { opacity: 1; transform: translateY(0); } }
@keyframes rfy-out { from { opacity: 1; } to { opacity: 0; } }

/* ---- right side buttons ------------------------------------------------------------ */
.rfy-root .nav_button_row { display: flex; align-items: center; gap: 1em; }
.rfy-root .nav_button_wrap { display: flex; align-items: center; gap: 0.75em; }

.rfy-root .btn_wrap {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0.5em 1em;
  font-weight: 600;
  color: var(--foreground);
  text-decoration: none;
  cursor: pointer;
}
.rfy-root .btn_inner {
  position: relative;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75em;
  border-radius: 100vw;
}
.rfy-root .btn_text {
  display: inline-flex;
  height: 1.4em;
  overflow: hidden;
  text-shadow: 0 1.4em currentColor;
}
.rfy-root .btn_text_span { display: inline-block; transition: transform var(--animation); }
.rfy-root .btn_wrap:hover .btn_text_span { transform: translateY(-100%); }

.rfy-root .btn_bg {
  position: absolute;
  inset: 0;
  z-index: 0;
  border-radius: 0.75em;
  box-shadow: var(--sh-glass);
  transition: all var(--duration-s) var(--smooth-ease);
}
.rfy-root .btn_wrap:hover .btn_bg { inset: 0.125em; }
.rfy-root .btn_wrap:active .btn_bg { scale: 0.95; }

.rfy-root .btn_wrap[data-variant="outline"] .btn_bg {
  background: var(--background);
  box-shadow: inset 0 0 0 1px var(--border);
}
.rfy-root .btn_wrap[data-variant="main"] { color: var(--primary-foreground); }
.rfy-root .btn_wrap[data-variant="main"] .btn_bg {
  background: var(--gr-btn);
  box-shadow: var(--sh-primary);
}

/* ---- hamburger --------------------------------------------------------------------- */
.rfy-root .menu-button {
  display: none;
  align-items: center;
  justify-content: center;
  width: 2.375em;
  height: 2.375em;
  color: var(--foreground);
  background: var(--muted);
  border: 0;
  border-radius: 0.75em;
  box-shadow: var(--sh-glass), inset 0 0 0 1px var(--border);
  cursor: pointer;
}

/* ---- animated scroll backdrop ------------------------------------------------------- */
.rfy-root .header_bg {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 280%;
  pointer-events: none;
  transform-origin: center top;
  transform: scaleY(0) translateY(-30%);
  transition: transform var(--duration-s) var(--smooth-ease);
}
.rfy-root .header[data-scrolled="true"] .header_bg { transform: scaleY(1) translateY(-30%); }
.rfy-root .header_bg > div { position: absolute; inset: 0; }
.rfy-root .header_bg > div:nth-child(1) {
  z-index: 2; backdrop-filter: blur(1px);
  -webkit-mask: linear-gradient(to top, rgba(255,255,255,0) 0%, #fff 12.5%, #fff 37.5%, rgba(255,255,255,0) 50%);
  mask: linear-gradient(to top, rgba(255,255,255,0) 0%, #fff 12.5%, #fff 37.5%, rgba(255,255,255,0) 50%);
}
.rfy-root .header_bg > div:nth-child(2) {
  z-index: 3; backdrop-filter: blur(2px);
  -webkit-mask: linear-gradient(to top, rgba(255,255,255,0) 12.5%, #fff 37.5%, #fff 50%, rgba(255,255,255,0) 62.5%);
  mask: linear-gradient(to top, rgba(255,255,255,0) 12.5%, #fff 37.5%, #fff 50%, rgba(255,255,255,0) 62.5%);
}
.rfy-root .header_bg > div:nth-child(3) {
  z-index: 4; backdrop-filter: blur(4px);
  -webkit-mask: linear-gradient(to top, rgba(255,255,255,0) 25%, #fff 50%, #fff 62.5%, rgba(255,255,255,0) 75%);
  mask: linear-gradient(to top, rgba(255,255,255,0) 25%, #fff 50%, #fff 62.5%, rgba(255,255,255,0) 75%);
}
.rfy-root .header_bg > div:nth-child(4) {
  z-index: 5;
  background: linear-gradient(to bottom, color-mix(in oklab, var(--background) 78%, transparent), transparent);
  backdrop-filter: blur(8px);
  -webkit-mask: linear-gradient(to top, rgba(255,255,255,0) 37.5%, #fff 62.5%);
  mask: linear-gradient(to top, rgba(255,255,255,0) 37.5%, #fff 62.5%);
}

/* ---- mobile panel ---------------------------------------------------------------- */
.rfy-root .rfy-mobile {
  position: fixed;
  inset: 0 0 auto 0;
  padding: 5.5em 1.25em 1.5em;
  visibility: hidden;
  pointer-events: none;
}
.rfy-root .rfy-mobile[data-state="open"] { visibility: visible; pointer-events: auto; }
.rfy-root .rfy-mobile__panel {
  max-height: calc(100vh - 7em);
  overflow-y: auto;
  padding: 1em;
  color: var(--popover-foreground);
  background: var(--popover);
  border-radius: 1.25em;
  box-shadow: var(--sh-base);
  transform: translateY(calc(-100% - 2em));
  transition: transform 0.6s var(--smooth-ease);
}
.rfy-root .rfy-mobile[data-state="open"] .rfy-mobile__panel { transform: translateY(0); }
.rfy-root .rfy-mobile__group { padding: 0.5em 0; }
.rfy-root .rfy-mobile__label {
  margin: 0 0 0.25em;
  padding: 0 0.625em;
  font-size: 0.8125em;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--foreground);
}
.rfy-root .rfy-mobile__section { margin-top: 0.35em; }
.rfy-root .rfy-mobile__subhead {
  margin: 0 0 0.15em;
  padding: 0 0.625em;
  font-size: 0.6875em;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--muted-foreground);
}
.rfy-root .rfy-mobile__links { display: flex; flex-direction: column; }
.rfy-root .rfy-mobile__plain {
  display: flex;
  flex-direction: column;
  padding: 0.5em 0.625em;
  border-top: 1px solid var(--border);
  margin-top: 0.5em;
}
.rfy-root .rfy-mobile__plain a {
  padding: 0.5em 0;
  font-size: 0.9375em;
  font-weight: 500;
  text-decoration: none;
  color: inherit;
}
.rfy-root .nav_button_wrap.cc-mobile {
  display: flex;
  flex-direction: column;
  gap: 0.5em;
  margin-top: 0.75em;
}
.rfy-root .nav_button_wrap.cc-mobile .btn_wrap { width: 100%; }

@media screen and (max-width: 767px) {
  .rfy-root .header { padding: 1em 1.25em; }
  .rfy-root .header[data-scrolled="true"][data-nav-status="closed"] { padding: 0.75em 1.25em; }
  .rfy-root .nav_button_row .nav_button_wrap { display: none; }
  .rfy-root .menu-button { display: flex; }
}
@media screen and (min-width: 768px) {
  .rfy-root .rfy-mobile { display: none; }
}

@media (prefers-reduced-motion: reduce) {
  .rfy-root .header,
  .rfy-root .header * { transition-duration: 0.01ms !important; animation-duration: 0.01ms !important; }
}
`;
