import type { Metadata } from "next";
import Image from "next/image";
import Link from "next/link";
import { ArrowRight } from "lucide-react";

import { BracketHeading } from "@/components/my-components/bracket-heading";
import { ClosingCta } from "@/components/my-components/closing-cta";
import { Button } from "@/components/ui/button";
import { pageMetadata } from "@/lib/seo";

const title = "Richard's work";
const description = "The engineering context behind Ferrit.";
export const metadata: Metadata = pageMetadata(title, description, "/work");

const WORK_ITEMS = [
  {
    label: "Ferrit",
    title: "A Git manager for the terminal",
    type: "Rust · Ratatui · Open source",
    scope: "Architecture, implementation, testing, and delivery",
    image: "/images/ferrit/demo-ferrit.webp",
    href: "/project",
    body: "Ferrit brings everyday Git work into one focused terminal workflow while preserving the behavior and control developers expect from Git.",
  },
  {
    label: "Systems direction",
    title: "A Rust CLI for Microsoft Fabric",
    type: "Rust · CLI · Data platform",
    scope: "Authentication, workspace discovery, and partition refresh",
    image: "/images/richard/linkedin-banner.webp",
    href: "/contact",
    body: "A systems-oriented CLI direction connecting Richard's data engineering background with Rust tooling. Production status remains to be confirmed.",
  },
  {
    label: "Data engineering",
    title: "Eight years of data and analytics engineering",
    type: "Data · Analytics · Architecture",
    scope: "Reusable data products, automation, and technical leadership",
    image: "/images/richard/portrait.webp",
    href: "/about",
    body: "Before Ferrit, Richard worked across data and analytics engineering. That context shapes how the project treats flow, reliability, maintainability, and user trust.",
  },
] as const;

const CASE_STUDY_LENSES = [
  ["Context", "Why the work existed and which user or system constraint mattered."],
  ["Contribution", "The role, decisions, and parts of the system Richard owned."],
  ["Evidence", "What was delivered and which artifacts make the result inspectable."],
] as const;

export default function WorkPage() {
  return (
    <div className="flex flex-1 flex-col">
      <section className="mx-auto grid w-full max-w-5xl gap-12 px-4 py-16 sm:py-24 lg:grid-cols-[1fr_0.8fr] lg:items-end lg:gap-20">
        <div className="flex flex-col gap-7">
          <BracketHeading as="h1" kicker="About the builder">
            The context behind Ferrit.
          </BracketHeading>
          <p className="max-w-xl text-lg leading-8 text-muted-foreground">
            Ferrit is the main project on this site. This page adds the engineering
            context around it: the systems direction, the data background, and the habits
            carried into the finished Rust tool.
          </p>
        </div>
        <div className="rounded-3xl border border-border bg-card p-6 ring-1 ring-foreground/5">
          <p className="font-mono text-[12px] uppercase tracking-wide text-muted-foreground">
            Working principle
          </p>
          <p className="mt-4 text-lg leading-8 text-foreground">
            Understand the system first. Make the useful path obvious.
          </p>
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <BracketHeading kicker="How to read the work" muted>
          Every case has the same useful lens.
        </BracketHeading>
        <div className="mt-10 overflow-hidden rounded-3xl border border-border bg-card ring-1 ring-foreground/5">
          <div className="grid md:grid-cols-3 md:divide-x md:divide-border">
            {CASE_STUDY_LENSES.map(([heading, body], index) => (
              <article
                key={heading}
                className="border-b border-border p-6 last:border-b-0 md:border-b-0 sm:p-8"
              >
                <span className="font-mono text-sm text-muted-foreground">
                  0{index + 1}
                </span>
                <h2 className="mt-10 text-xl font-semibold tracking-tight">{heading}</h2>
                <p className="mt-3 text-sm leading-6 text-muted-foreground">{body}</p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <div className="grid gap-5">
          {WORK_ITEMS.map((item, index) => (
            <article
              key={item.label}
              className="group grid overflow-hidden rounded-3xl border border-border bg-card ring-1 ring-foreground/5 md:grid-cols-[0.95fr_1.05fr]"
            >
              <div className="relative min-h-64 overflow-hidden bg-muted md:min-h-80">
                <Image
                  src={item.image}
                  alt={item.title}
                  fill
                  sizes="(min-width: 768px) 45vw, 100vw"
                  className="object-cover transition-transform duration-500 group-hover:scale-[1.03]"
                />
                <span className="absolute left-5 top-5 rounded-full bg-background/90 px-3 py-1 font-mono text-[11px] uppercase tracking-wide text-muted-foreground">
                  0{index + 1}
                </span>
              </div>
              <div className="flex flex-col justify-between gap-8 p-6 sm:p-8">
                <div>
                  <p className="font-mono text-[12px] uppercase tracking-wide text-muted-foreground">
                    {item.label}
                  </p>
                  <h2 className="mt-4 max-w-md text-2xl font-semibold tracking-tight sm:text-3xl">
                    {item.title}
                  </h2>
                  <p className="mt-4 max-w-lg text-sm leading-7 text-muted-foreground">
                    {item.body}
                  </p>
                </div>
                <div className="flex flex-wrap items-center justify-between gap-4 border-t border-border pt-5">
                  <span className="text-xs font-medium text-muted-foreground">
                    {item.type}
                  </span>
                  <span className="max-w-[18rem] text-right text-xs text-muted-foreground">
                    {item.scope}
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    nativeButton={false}
                    render={<Link href={item.href} />}
                  >
                    Read more
                    <ArrowRight />
                  </Button>
                </div>
              </div>
            </article>
          ))}
        </div>
      </section>

      <section className="mx-auto grid w-full max-w-5xl gap-8 px-4 py-16 lg:grid-cols-[0.8fr_1.2fr] lg:gap-20">
        <BracketHeading kicker="Proof in practice" muted>
          The work is more than a screenshot.
        </BracketHeading>
        <div className="rounded-3xl border border-border bg-card p-7 ring-1 ring-foreground/5 sm:p-9">
          <p className="text-lg leading-8 text-muted-foreground">
            Ferrit makes the proof concrete: a public repository, a usable terminal
            application, release history, tests, replay fixtures, documentation, and code
            boundaries that another engineer can inspect.
          </p>
          <div className="mt-8 grid gap-3 sm:grid-cols-2">
            {["Context", "Constraints", "Decisions", "Outcome"].map((item) => (
              <div
                key={item}
                className="rounded-xl bg-muted/60 px-4 py-3 text-sm font-medium"
              >
                {item}
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 pb-24">
        <ClosingCta
          heading="Have a Rust system worth making clearer?"
          body="Share the context, the constraint, and the result you need. Start from the problem, then choose the tool."
          ctaLabel="Contact me"
          ctaHref="/contact"
          ctaClassName="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
        />
      </section>
    </div>
  );
}
