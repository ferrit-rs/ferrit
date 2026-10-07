import type { Metadata } from "next";
import Image from "next/image";
import Link from "next/link";
import { ArrowRight, Check } from "lucide-react";

import { ContactCtaButton } from "@/components/contact-cta-button";
import { BracketHeading } from "@/components/my-components/bracket-heading";
import { ClosingCta } from "@/components/my-components/closing-cta";
import { Button } from "@/components/ui/button";
import { pageMetadata } from "@/lib/seo";
import { siteConfig } from "@/lib/site";

const title = "My Story";
const description = "Richard Lavoura's path from data engineering to systems Rust.";
export const metadata: Metadata = pageMetadata(title, description, "/about");

const PRINCIPLES = [
  {
    title: "Understand before building",
    body: "Start with the problem, the data flow, the people using the system, and the constraint that actually matters.",
  },
  {
    title: "Make intent explicit",
    body: "Readable boundaries, useful names, typed decisions, and documentation beat clever code that only its author can explain.",
  },
  {
    title: "Leave a stronger system",
    body: "A good result includes tests, operating context, a release path, and enough reasoning for the next engineer to continue.",
  },
];

const TIMELINE = [
  {
    year: "2017",
    title: "Started in analytics",
    body: "Data analysis and business context became the foundation: understand the signal before proposing the solution.",
  },
  {
    year: "2019",
    title: "Built reusable data products",
    body: "Analytics engineering expanded into ingestion, modeling, dashboards, automation, and delivery across large client environments.",
  },
  {
    year: "2024",
    title: "Moved closer to architecture",
    body: "Technical validation, standards, data quality, and acting Tech Lead work made system boundaries and trade-offs central.",
  },
  {
    year: "2026",
    title: "Building in systems Rust",
    body: "Ferrit applies those habits to a public terminal tool: reliable boundaries, visible process behavior, and deliberate delivery.",
  },
];

export default function AboutPage() {
  return (
    <div className="flex flex-1 flex-col">
      <section className="mx-auto grid w-full max-w-5xl gap-12 px-4 py-16 sm:py-24 lg:grid-cols-[1.1fr_0.9fr] lg:items-center lg:gap-20">
        <div className="flex flex-col gap-7">
          <BracketHeading as="h1" kicker="About me">
            A data engineer learning toward systems Rust.
          </BracketHeading>
          <p className="max-w-xl text-lg leading-8 text-muted-foreground">
            I am Richard Lavoura. I have spent eight years turning messy data, business
            questions, and technical constraints into systems people can use. Now I am
            applying that experience to backend Rust, developer tooling, and systems
            programming.
          </p>
          <div className="flex flex-wrap gap-3">
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
              render={<Link href="/articles" />}
            >
              Read the articles
            </Button>
          </div>
        </div>
        <div className="relative mt-8 lg:mt-0">
          <div className="relative aspect-square overflow-hidden rounded-3xl border border-border bg-card p-2 shadow-sm ring-1 ring-foreground/5 sm:p-3">
            <Image
              src={siteConfig.author.image}
              alt="Richard Lavoura"
              fill
              sizes="(min-width: 1024px) 36vw, 100vw"
              className="object-cover"
              priority
            />
          </div>
          <div className="absolute -bottom-6 -left-4 grid w-[calc(100%+2rem)] grid-cols-3 divide-x divide-border rounded-2xl border border-border bg-card/95 p-4 shadow-xl shadow-black/5 backdrop-blur sm:-left-8 sm:w-[calc(100%+4rem)] sm:p-5">
            {siteConfig.about.stats.map(({ value, label }) => (
              <div key={label} className="flex min-w-0 flex-col gap-1 px-2 text-center">
                <span className="text-lg font-semibold tracking-tight sm:text-xl">
                  {value}
                </span>
                <span className="text-[10px] leading-4 text-muted-foreground sm:text-xs">
                  {label}
                </span>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="mx-auto grid w-full max-w-5xl gap-8 px-4 py-16 lg:grid-cols-[0.8fr_1.2fr] lg:gap-20">
        <BracketHeading kicker="My story" muted>
          Curiosity became a way of working.
        </BracketHeading>
        <div className="flex flex-col gap-6 text-[16px] leading-8 text-muted-foreground">
          <p>
            My background is in data and analytics engineering. I have worked with teams
            at CHANEL, Kering, Carrefour, Stellantis, Française des Jeux, and other
            enterprise environments where the hard part is rarely writing one query. The
            hard part is understanding the ecosystem well enough to build something
            reliable.
          </p>
          <p>
            That work shaped how I approach software: ask why before choosing how, make
            business rules and data flows visible, reduce repeated work, and leave clear
            standards behind. It also made me comfortable moving between implementation,
            architecture, delivery, and the people who depend on the result.
          </p>
          <p>
            Rust is the next expression of that mindset. Ownership, explicit error paths,
            and strong boundaries force important decisions into the design instead of
            leaving them for production to discover.
          </p>
          <div className="rounded-2xl border border-border bg-card p-6 text-foreground ring-1 ring-foreground/5">
            <p className="font-semibold tracking-tight">The short version.</p>
            <ul className="mt-4 flex flex-col gap-3 text-sm leading-6 text-muted-foreground">
              {[
                "Understand the real constraint.",
                "Make the system easier to reason about.",
                "Ship proof, not only promises.",
              ].map((point) => (
                <li key={point} className="flex items-start gap-3">
                  <Check className="mt-1 size-4 shrink-0 text-primary" />
                  <span>{point}</span>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <BracketHeading kicker="Principles" muted>
          How I approach the work.
        </BracketHeading>
        <div className="mt-10 grid gap-4 md:grid-cols-3">
          {PRINCIPLES.map((principle, index) => (
            <article
              key={principle.title}
              className="rounded-2xl border border-border bg-card p-6 ring-1 ring-foreground/5"
            >
              <span className="font-mono text-sm text-muted-foreground">
                0{index + 1}
              </span>
              <h2 className="mt-8 text-xl font-semibold tracking-tight">
                {principle.title}
              </h2>
              <p className="mt-3 text-sm leading-6 text-muted-foreground">
                {principle.body}
              </p>
            </article>
          ))}
        </div>
      </section>

      <section className="mx-auto grid w-full max-w-5xl gap-8 px-4 py-16 lg:grid-cols-[0.8fr_1.2fr] lg:gap-20">
        <BracketHeading kicker="Journey" muted>
          The direction is clear.
        </BracketHeading>
        <div className="flex flex-col divide-y divide-border rounded-3xl border border-border bg-card px-6 ring-1 ring-foreground/5">
          {TIMELINE.map((item) => (
            <div
              key={item.year + item.title}
              className="grid gap-3 py-6 sm:grid-cols-[96px_1fr] sm:gap-8"
            >
              <span className="font-mono text-sm text-muted-foreground">{item.year}</span>
              <div>
                <h2 className="font-semibold tracking-tight">{item.title}</h2>
                <p className="mt-2 text-sm leading-6 text-muted-foreground">
                  {item.body}
                </p>
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 pb-24">
        <ClosingCta
          heading="Want to understand the work?"
          body="Start with Ferrit, then follow the decisions, constraints, and lessons behind it."
          ctaLabel="Explore Ferrit"
          ctaHref="/project"
          ctaClassName="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
        />
      </section>
    </div>
  );
}
