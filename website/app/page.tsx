import Image from "next/image";
import Link from "next/link";
import {
  ArrowRight,
  BookOpen,
  Compass,
  GitCompare,
  Library,
  Newspaper,
  Users,
} from "lucide-react";

import { StickyVideo } from "@/components/sticky-video";
import { Button } from "@/components/ui/button";
import { ArticlePosterCard } from "@/components/my-components/article-poster-card";
import { BracketHeading } from "@/components/my-components/bracket-heading";
import { ClosingCta } from "@/components/my-components/closing-cta";
import { ContactCtaButton } from "@/components/contact-cta-button";
import { CreatorVideoPlaceholder } from "@/components/creator-video-placeholder";
import { FaqAccordion } from "@/components/my-components/faq-accordion";
import { getArticleSummaries } from "@/lib/articles";
import { projectConfig } from "@/lib/project";
import { siteConfig } from "@/lib/site";

const FEATURES = [
  {
    icon: GitCompare,
    title: "Git fidelity",
    body: "Use libgit2 for stable reads and the real Git binary where config, hooks, diff drivers, and command behavior must remain exact.",
  },
  {
    icon: Compass,
    title: "Headless core",
    body: "Keep domain logic independent from Ratatui so repository behavior can be tested without rendering a terminal frame.",
  },
  {
    icon: BookOpen,
    title: "Explainable decisions",
    body: "Document why each boundary exists, from process logging to SSH askpass and the choice to avoid unstable snapshots.",
  },
  {
    icon: Newspaper,
    title: "Public delivery",
    body: "Release notes, a README, demo media, an MIT license, and a clear install path turn source code into a usable project.",
  },
  {
    icon: Users,
    title: "Real engineering context",
    body: "Eight years of data engineering inform the way Ferrit treats quality, data flow, constraints, and the person using the tool.",
  },
  {
    icon: Library,
    title: "Tests that tell a story",
    body: "Integration tests and replay scripts cover complete workflows: stage, commit, rebase, conflict, remote, dashboard, and settings.",
  },
];

const SHOWCASES = [
  {
    kicker: "The project",
    heading: "A daily Git workflow, rebuilt in Rust.",
    body: "Ferrit gives one terminal surface for status, files, branches, commits, stash, diffs, configuration, and repository history. It also starts the repository when the folder is still empty. The workflow covers precise staging at file, hunk, and line level, explicit history changes, and GitHub remote creation without forcing the user to leave the terminal.",
    detail:
      "The point is not to hide Git behind a polished dashboard. The point is to make the current state legible before an action happens, then keep the result visible after it happens. That makes a daily workflow faster without making it less understandable.",
    image: "/images/ferrit/demo-ferrit.webp",
    alt: "Ferrit terminal demo",
  },
  {
    kicker: "The boundary",
    heading: "Trust is designed at every process edge.",
    body: "A Git TUI can lose trust through one incorrect diff, one hidden prompt, or one command nobody can audit. Ferrit makes those seams visible and testable. The application treats process execution as part of the product: users need to know what ran, which state came back, and how an authentication or configuration problem returns to the workflow.",
    detail:
      "Structured reads use libgit2 where they help the interface. The real Git process remains available where hooks, configuration, diff drivers, credentials, and command semantics matter. One process seam handles logging, redaction, askpass, and the result returned to the running TUI.",
    image: null,
    alt: "",
  },
  {
    kicker: "The proof",
    heading: "A project built to be read, tested, and continued.",
    body: "The finished result includes strict lints, deterministic replay, integration coverage, release tags, and documentation that makes the next contribution easier. The proof is not one impressive screen. It is the collection of artifacts showing that the project can be read, exercised, released, and continued by someone who did not write every line.",
    detail:
      "The current snapshot contains more than 29k Rust lines in src, 411 commits on main, 59 integration test files, replay scripts, flow specifications, and a public v0.10.0 release. Those numbers matter because they give the project a trail: a recruiter, contributor, or maintainer can inspect how the result was built instead of judging a screenshot alone.",
    image: null,
    alt: "",
  },
];

const FAQ_ITEMS = [
  {
    q: "What problem does Ferrit solve?",
    a: "Ferrit gives everyday Git work one focused terminal surface. Status, files, branches, commits, reflog, stash, diffs, staging, remotes, settings, and repository setup stay visible in the same workflow, so the user spends less time reconstructing state from separate commands while Git remains the source of truth.",
  },
  {
    q: "Why build another Git TUI instead of using Git commands directly?",
    a: "Git commands are powerful, but a long sequence of commands can make repository state difficult to scan and easy to misread. Ferrit compresses the repetitive parts into a readable interface while keeping confirmations, command outcomes, configuration, hooks, and external Git behavior explicit instead of hiding them behind a private model.",
  },
  {
    q: "How is Ferrit structured under the terminal interface?",
    a: "The project keeps repository behavior in a headless Git domain and lets Ratatui render the current state and collect intent. libgit2 handles structured reads where it is useful, while the real Git process handles configuration, hooks, diff drivers, and behavior where compatibility matters. That boundary keeps the core testable and makes process behavior easier to inspect.",
  },
  {
    q: "How do you know the workflow is reliable?",
    a: "Ferrit tests behavior at more than one level. The repository includes integration tests, deterministic replay scripts, flow specifications, strict linting, and redacted command logs covering workflows such as staging, commits, rebases, conflicts, remotes, dashboard behavior, and settings. The goal is evidence from the user path, not only isolated functions passing.",
  },
  {
    q: "Who built Ferrit and where can I inspect it?",
    a: "Richard Lavoura built Ferrit as a public Rust project, applying eight years of data and analytics engineering experience to systems programming and developer tooling. The source, release history, documentation, MIT license, and current implementation are available in the public GitHub repository, with the project pages here explaining the important trade-offs before you open the code.",
  },
];

export default function Home() {
  const latestArticles = getArticleSummaries().slice(0, 3);

  return (
    <div className="flex flex-1 flex-col">
      {/* biome-ignore lint/correctness/useUniqueElementIds: stable page navigation anchor. */}
      <section
        id="project"
        className="mx-auto grid w-full max-w-5xl items-center gap-10 px-4 pt-20 pb-16 sm:pt-28 lg:grid-cols-2"
      >
        <div className="flex flex-col items-start gap-6">
          <BracketHeading as="h1" kicker="Ferrit · Rust terminal application">
            Git work, made visible.
          </BracketHeading>
          <p className="max-w-xl text-lg leading-relaxed text-muted-foreground">
            Ferrit is the everyday Git manager for your terminal. It brings status, files,
            branches, history, staging, remotes, and repository setup into one focused
            Rust workflow without hiding the Git behavior underneath.
          </p>
          <div className="flex flex-wrap gap-3">
            <Button
              size="lg"
              className="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
              nativeButton={false}
              render={<Link href="/project" />}
            >
              Explore Ferrit
              <ArrowRight />
            </Button>
            <ContactCtaButton
              href="/contact"
              className="h-12 rounded-xl px-6 text-base font-semibold"
            >
              Contact me
              <ArrowRight />
            </ContactCtaButton>
          </div>
          <div className="flex flex-wrap items-center gap-x-6 gap-y-2 pt-4 font-mono text-[12px] uppercase tracking-wide text-muted-foreground">
            <span>Rust · Ratatui</span>
            <span>Git-native workflow</span>
            <span>MIT open source</span>
            <span>v0.10.0 public release</span>
          </div>
        </div>
        <StickyVideo />
      </section>

      {/* biome-ignore lint/correctness/useUniqueElementIds: stable page navigation anchor. */}
      <section id="design" className="mx-auto w-full max-w-5xl px-4 py-16">
        <BracketHeading kicker="The approach" muted>
          Build the useful part. Make the reasoning visible.
        </BracketHeading>
        <div className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {FEATURES.map(({ icon: Icon, title, body }) => (
            <article
              key={title}
              className="flex flex-col gap-3 rounded-2xl border border-border bg-card p-6 ring-1 ring-foreground/5"
            >
              <span className="flex size-10 items-center justify-center rounded-xl bg-muted text-primary ring-1 ring-border">
                <Icon size={18} strokeWidth={1.75} />
              </span>
              <h2 className="text-[16px] font-semibold tracking-tight">{title}</h2>
              <p className="text-[14px] leading-[1.55] text-muted-foreground">{body}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <div className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border bg-border ring-1 ring-foreground/5 sm:grid-cols-4">
          {projectConfig.stats.map(({ value, label }) => (
            <div key={label} className="flex flex-col gap-1 bg-card p-6">
              <span className="text-[clamp(24px,3vw,32px)] font-semibold tracking-tight text-foreground">
                {value}
              </span>
              <span className="text-[13px] text-muted-foreground">{label}</span>
            </div>
          ))}
        </div>
      </section>

      {SHOWCASES.map((showcase, index) => (
        <section
          key={showcase.heading}
          id={index === 0 ? "work" : undefined}
          className={`mx-auto grid w-full max-w-5xl items-center gap-10 px-4 py-16 lg:grid-cols-2 ${showcase.image ? "" : "border-t border-border/70"}`}
        >
          <div
            className={[
              index % 2 === 0 ? "lg:order-1" : "lg:order-2",
              showcase.image ? "" : "mx-auto w-full max-w-4xl lg:col-span-2",
            ].join(" ")}
          >
            <div className="flex flex-col gap-5">
              <BracketHeading
                kicker={showcase.kicker}
                muted
                align={showcase.image ? "left" : "center"}
              >
                {showcase.heading}
              </BracketHeading>
              <p
                className={`text-[16px] leading-[1.7] text-muted-foreground ${showcase.image ? "" : "mx-auto max-w-3xl text-center"}`}
              >
                {showcase.body}
              </p>
              <p
                className={`text-[16px] leading-[1.7] text-muted-foreground ${showcase.image ? "" : "mx-auto max-w-3xl text-center"}`}
              >
                {showcase.detail}
              </p>
            </div>
          </div>
          {showcase.image ? (
            <div
              className={`relative aspect-4/3 w-full overflow-hidden rounded-2xl border border-border bg-card ring-1 ring-foreground/5 ${index % 2 === 0 ? "lg:order-2" : "lg:order-1"}`}
            >
              <Image
                src={showcase.image}
                alt={showcase.alt}
                fill
                sizes="(min-width: 1024px) 50vw, 100vw"
                className="object-cover"
              />
            </div>
          ) : null}
        </section>
      ))}

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <BracketHeading kicker="About" muted align="center">
          Who am I?
        </BracketHeading>
        {/* biome-ignore lint/correctness/useUniqueElementIds: stable page navigation anchor. */}
        <div
          id="story"
          className="relative mt-32 rounded-3xl border border-border bg-card px-6 pb-10 pt-28 ring-1 ring-foreground/5 sm:px-10 sm:pt-32"
        >
          <div className="absolute left-1/2 top-0 size-44 -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-3xl border border-border bg-card shadow-sm sm:size-48">
            <Image
              src={siteConfig.author.image}
              alt="Richard Lavoura"
              fill
              sizes="192px"
              className="object-cover"
            />
          </div>
          <div className="mx-auto grid max-w-3xl grid-cols-3 divide-x divide-border">
            {siteConfig.about.stats.map(({ value, label }) => (
              <div
                key={label}
                className="flex flex-col items-center gap-1 px-3 text-center"
              >
                <span className="text-[clamp(24px,3vw,32px)] font-semibold tracking-tight text-foreground">
                  {value}
                </span>
                <span className="text-xs text-muted-foreground">{label}</span>
              </div>
            ))}
          </div>
          <div className="mx-auto my-12 h-px max-w-3xl bg-border" />
          <div className="mx-auto max-w-2xl">
            <CreatorVideoPlaceholder />
          </div>
          <div className="mx-auto mt-10 flex max-w-3xl flex-col gap-5 text-[15px] leading-7 text-muted-foreground">
            <p>
              I started in data and analytics because understanding the data was often the
              fastest way to understand the business. Over eight years, that meant
              building data products, automating workflows, reviewing architecture, and
              making complex systems useful to the people relying on them.
            </p>
            <p>
              That same instinct now shapes my Rust work. I want to understand the problem
              before choosing a tool, make ownership and boundaries explicit, and leave
              code that the next engineer can read without reverse-engineering every
              decision.
            </p>
            <p className="font-semibold text-foreground">
              Ferrit is where those habits meet systems Rust.
            </p>
            <ul className="flex flex-col gap-2 rounded-2xl bg-muted/60 px-5 py-4">
              <li>→ Understand the constraint before building</li>
              <li>→ Prefer readable boundaries over clever abstractions</li>
              <li>→ Test behavior in the context users actually experience</li>
              <li>→ Ship a useful result with a clear trail behind it</li>
            </ul>
            <p>
              Technology is a means. The goal is a reliable system, an honest explanation
              of its trade-offs, and a result that can keep improving after the first
              release.
            </p>
          </div>
          <div className="mt-10 flex flex-wrap items-center justify-center gap-3">
            <ContactCtaButton
              href="/contact"
              className="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
            >
              Contact me <ArrowRight />
            </ContactCtaButton>
            <Button
              variant="outline"
              size="lg"
              className="h-12 rounded-xl px-6 text-base font-semibold"
              nativeButton={false}
              render={<Link href="/work" />}
            >
              Explore my work <ArrowRight />
            </Button>
          </div>
        </div>
      </section>

      {latestArticles.length > 0 && (
        <section className="mx-auto w-full max-w-5xl px-4 py-16">
          <div className="flex flex-wrap items-end justify-between gap-4">
            <BracketHeading kicker="Articles" muted>
              Latest articles
            </BracketHeading>
            <Button
              variant="outline"
              nativeButton={false}
              render={<Link href="/articles" />}
            >
              Browse all articles <ArrowRight />
            </Button>
          </div>
          <div className="mt-10 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {latestArticles.map((article, index) => (
              <ArticlePosterCard
                key={article.slug}
                article={article}
                eventCategory="home"
                position={index + 1}
              />
            ))}
          </div>
        </section>
      )}

      {/* biome-ignore lint/correctness/useUniqueElementIds: stable page navigation anchor. */}
      <section id="faq" className="mx-auto w-full max-w-3xl px-4 py-16">
        <BracketHeading kicker="FAQ" muted>
          Questions worth answering.
        </BracketHeading>
        <div className="mt-8">
          <FaqAccordion items={FAQ_ITEMS} />
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 pb-24">
        <ClosingCta
          heading="🦀 Working on a Rust project?"
          body="Tell me what you are building, where you are stuck, and what you want to make reliable next."
          ctaLabel="Contact me"
          ctaHref="/contact"
          ctaClassName="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
        />
      </section>
    </div>
  );
}
