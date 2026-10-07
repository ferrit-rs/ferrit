import type { Metadata } from "next";
import Image from "next/image";
import Link from "next/link";
import { ArrowRight, Check } from "lucide-react";

import { ContactCtaButton } from "@/components/contact-cta-button";
import { BracketHeading } from "@/components/my-components/bracket-heading";
import { ClosingCta } from "@/components/my-components/closing-cta";
import { ProjectStoryCarousel } from "@/components/my-components/project-story-carousel";
import { Button } from "@/components/ui/button";
import { projectConfig } from "@/lib/project";
import { pageMetadata } from "@/lib/seo";

const title = "Ferrit project overview";
const description =
  "The idea, architecture, and delivery behind Ferrit, a Git manager for the terminal.";
export const metadata: Metadata = pageMetadata(title, description, "/project");

export default function ProjectPage() {
  return (
    <div className="flex flex-1 flex-col">
      <section className="mx-auto grid w-full max-w-5xl gap-12 px-4 py-16 sm:py-24 lg:grid-cols-[1.1fr_0.9fr] lg:items-center lg:gap-20">
        <div className="flex flex-col gap-7">
          <BracketHeading as="h1" kicker="The project">
            A Git manager that respects the work.
          </BracketHeading>
          <p className="max-w-xl text-lg leading-8 text-muted-foreground">
            Ferrit is a terminal-first Git manager built in Rust. It keeps everyday Git
            operations visible, reversible, and close to the commands developers already
            trust, while giving the workflow a calmer interface.
          </p>
          <div className="flex flex-wrap gap-3">
            <ContactCtaButton
              href="/contact"
              className="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
            >
              Contact me
              <ArrowRight />
            </ContactCtaButton>
            <Button
              variant="outline"
              size="lg"
              className="h-12 rounded-xl px-6 text-base font-semibold"
              nativeButton={false}
              render={<Link href="/work" />}
            >
              See the work
            </Button>
          </div>
        </div>
        <div className="relative overflow-hidden rounded-3xl border border-border bg-card shadow-sm ring-1 ring-foreground/5">
          <Image
            src="/images/ferrit/social-preview.webp"
            alt="Ferrit Git manager project preview"
            width={1200}
            height={630}
            className="aspect-[1200/630] w-full object-cover"
            priority
          />
          <div className="flex items-center justify-between gap-4 border-t border-border px-5 py-4 text-sm">
            <span className="font-medium">
              The everyday Git manager for your terminal.
            </span>
            <span className="font-mono text-xs text-muted-foreground">v0.10.0</span>
          </div>
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <div className="grid grid-cols-2 gap-px overflow-hidden rounded-3xl border border-border bg-border ring-1 ring-foreground/5 sm:grid-cols-4">
          {projectConfig.stats.map(({ value, label }) => (
            <div key={label} className="flex flex-col gap-1 bg-card p-6 sm:p-7">
              <span className="text-[clamp(24px,3vw,32px)] font-semibold tracking-tight">
                {value}
              </span>
              <span className="text-sm text-muted-foreground">{label}</span>
            </div>
          ))}
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <BracketHeading kicker="At a glance" muted>
          The project in four lines.
        </BracketHeading>
        <div className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {projectConfig.details.map(({ label, value }) => (
            <div
              key={label}
              className="rounded-2xl border border-border bg-card p-5 ring-1 ring-foreground/5"
            >
              <p className="font-mono text-[11px] uppercase tracking-wide text-muted-foreground">
                {label}
              </p>
              <p className="mt-8 text-lg font-semibold tracking-tight">{value}</p>
            </div>
          ))}
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <div className="grid gap-10 lg:grid-cols-[0.72fr_1.28fr] lg:gap-20">
          <div>
            <BracketHeading kicker="The result" muted>
              Built around the useful part.
            </BracketHeading>
            <p className="mt-6 max-w-sm text-[15px] leading-7 text-muted-foreground">
              Ferrit turns a long Git command sequence into a readable workflow without
              hiding the state changes underneath. Each decision protects fidelity first,
              then adds convenience where it earns its place.
            </p>
          </div>
          <div className="overflow-hidden rounded-3xl border border-border bg-card ring-1 ring-foreground/5">
            {projectConfig.pillars.map((pillar) => (
              <article
                key={pillar.number}
                className="grid gap-4 border-b border-border p-6 last:border-b-0 md:grid-cols-[56px_170px_1fr] md:items-start md:gap-6"
              >
                <span className="font-mono text-sm text-muted-foreground">
                  {pillar.number}
                </span>
                <h2 className="text-lg font-semibold tracking-tight">{pillar.title}</h2>
                <p className="text-sm leading-6 text-muted-foreground">{pillar.body}</p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <ProjectStoryCarousel steps={projectConfig.steps} />

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <div className="flex flex-col gap-6">
          <BracketHeading kicker="System map" muted>
            From core logic to shipped result.
          </BracketHeading>
          <p className="max-w-2xl text-[15px] leading-7 text-muted-foreground">
            Ferrit separates the rules of Git work from the terminal surface and from the
            release path. That separation keeps the core testable, the UI replaceable, and
            the user-facing behavior explainable.
          </p>
        </div>
        <div className="relative mt-10 overflow-hidden rounded-3xl border border-border bg-card p-2 ring-1 ring-foreground/5">
          <div className="grid md:grid-cols-3 md:divide-x md:divide-border">
            {projectConfig.layers.map(([label, body], index) => (
              <article key={label} className="relative p-6 sm:p-8">
                <div className="flex items-center justify-between">
                  <span className="font-mono text-sm text-muted-foreground">
                    0{index + 1}
                  </span>
                  {index < projectConfig.layers.length - 1 ? (
                    <ArrowRight className="hidden size-4 text-muted-foreground md:block" />
                  ) : null}
                </div>
                <h2 className="mt-16 text-xl font-semibold tracking-tight">{label}</h2>
                <p className="mt-3 text-sm leading-6 text-muted-foreground">{body}</p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <section className="mx-auto grid w-full max-w-5xl gap-8 px-4 py-16 lg:grid-cols-[0.8fr_1.2fr] lg:gap-20">
        <BracketHeading kicker="Outcome" muted>
          What the project delivers.
        </BracketHeading>
        <div className="rounded-3xl border border-border bg-card p-7 ring-1 ring-foreground/5 sm:p-9">
          <p className="text-lg leading-8 text-muted-foreground">
            A public Rust application with a clear domain boundary, a deliberate Git
            integration seam, a usable terminal interface, and enough tests and release
            history for another engineer to inspect the work instead of taking it on
            faith.
          </p>
          <ul className="mt-8 grid gap-4 sm:grid-cols-2">
            {[
              "Readable Git state",
              "Focused terminal workflow",
              "Explicit Rust boundaries",
              "Public release trail",
            ].map((item) => (
              <li key={item} className="flex items-center gap-3 text-sm font-medium">
                <span className="flex size-7 items-center justify-center rounded-full bg-muted text-primary">
                  <Check className="size-4" />
                </span>
                {item}
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 pb-24">
        <ClosingCta
          heading="Building a Rust project with real constraints?"
          body="Tell me what the system needs to do, where the hard edges are, and what a finished result should make easier."
          ctaLabel="Contact me"
          ctaHref="/contact"
          ctaClassName="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
        />
      </section>
    </div>
  );
}
