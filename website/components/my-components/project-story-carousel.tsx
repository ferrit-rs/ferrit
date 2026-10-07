"use client";

import useEmblaCarousel from "embla-carousel-react";
import { motion, useReducedMotion } from "motion/react";
import * as React from "react";

type ProjectStoryStep = {
  label: string;
  detail: string;
  title?: React.ReactNode;
  expandedContent?: React.ReactNode;
};

type ProjectStoryCarouselProps = {
  steps: readonly ProjectStoryStep[];
};

export function ProjectStoryCarousel({ steps }: ProjectStoryCarouselProps) {
  const prefersReducedMotion = useReducedMotion();
  const [viewportRef, emblaApi] = useEmblaCarousel({
    align: "start",
    containScroll: false,
    skipSnaps: false,
  });
  const [activeIndex, setActiveIndex] = React.useState(0);
  const [isHovering, setIsHovering] = React.useState(false);
  const activeIndexRef = React.useRef(0);
  const isDraggingRef = React.useRef(false);

  React.useEffect(() => {
    if (!emblaApi) return;

    const handleSelect = () => {
      const nextIndex = emblaApi.selectedScrollSnap();
      activeIndexRef.current = nextIndex;
      setActiveIndex(nextIndex);
    };

    handleSelect();
    emblaApi.on("select", handleSelect);
    emblaApi.on("reInit", handleSelect);

    return () => {
      emblaApi.off("select", handleSelect);
      emblaApi.off("reInit", handleSelect);
    };
  }, [emblaApi]);

  React.useEffect(() => {
    if (!emblaApi || steps.length < 2 || isHovering || prefersReducedMotion) return;

    const timer = window.setInterval(() => {
      if (!isDraggingRef.current) {
        emblaApi.scrollTo((activeIndexRef.current + 1) % steps.length);
      }
    }, 5600);

    return () => window.clearInterval(timer);
  }, [emblaApi, isHovering, prefersReducedMotion, steps.length]);

  const selectStep = (index: number) => {
    emblaApi?.scrollTo(index);
  };

  if (steps.length === 0) return null;

  return (
    <section
      aria-label="Project steps"
      className="mx-auto mt-24 max-w-[1080px] overflow-hidden px-5 sm:px-6"
      onMouseEnter={() => setIsHovering(true)}
      onMouseLeave={() => setIsHovering(false)}
    >
      <div className="mx-auto max-w-3xl text-center">
        <p className="text-[10px] font-semibold tracking-[0.22em] text-primary uppercase">
          The project journey
        </p>
        <h2 className="mt-3 text-3xl font-extrabold tracking-[-1.5px] text-foreground sm:text-4xl">
          From first constraint to shipped result.
        </h2>
      </div>

      <div className="relative mx-auto mt-10 max-w-[760px] px-2 sm:mt-12">
        <div
          aria-hidden="true"
          className="pointer-events-none absolute top-5 right-5 left-5 h-px bg-border/80"
        />
        <div
          aria-hidden="true"
          className="pointer-events-none absolute top-5 left-5 h-px bg-gradient-to-r from-primary/35 via-primary to-primary"
          style={{
            width: `calc(${(activeIndex / Math.max(steps.length - 1, 1)) * 100}% - 10px)`,
          }}
        />

        <ol
          className="relative grid"
          style={{ gridTemplateColumns: `repeat(${steps.length}, minmax(0, 1fr))` }}
          aria-label="Project stages"
        >
          {steps.map((step, index) => {
            const active = index === activeIndex;
            const isLast = index === steps.length - 1;

            return (
              <li key={step.label} className="flex justify-center">
                <button
                  type="button"
                  onClick={() => selectStep(index)}
                  aria-label={`${step.label}, step ${index + 1} of ${steps.length}`}
                  aria-current={active ? "step" : undefined}
                  className="group flex min-w-0 cursor-pointer flex-col items-center"
                >
                  <span
                    className={[
                      "flex size-10 items-center justify-center rounded-full border-2 text-xs font-bold transition-[transform,box-shadow,border-color,background-color,color] duration-300 sm:size-11 sm:text-sm",
                      active
                        ? "border-primary bg-primary text-primary-foreground shadow-xl shadow-primary/20 ring-4 ring-primary/10"
                        : "border-border bg-card text-foreground shadow-sm group-hover:border-primary/50",
                    ].join(" ")}
                  >
                    {isLast ? "∞" : `S${index + 1}`}
                  </span>
                  <span
                    className={[
                      "mt-2 max-w-24 truncate text-center text-[9px] font-semibold tracking-[0.14em] uppercase transition-colors duration-300 sm:text-[10px]",
                      active ? "text-primary" : "text-muted-foreground/65",
                    ].join(" ")}
                  >
                    {step.label}
                  </span>
                </button>
              </li>
            );
          })}
        </ol>
      </div>

      <div
        ref={viewportRef}
        className="-mx-5 mt-9 overflow-hidden px-5 sm:-mx-6 sm:px-6"
        onPointerDown={() => {
          isDraggingRef.current = true;
        }}
        onPointerUp={() => {
          isDraggingRef.current = false;
        }}
        onPointerCancel={() => {
          isDraggingRef.current = false;
        }}
      >
        <div className="flex touch-pan-y gap-4 py-2">
          {steps.map((step, index) => {
            const active = index === activeIndex;
            const title = step.title ?? step.label;

            return (
              <div
                key={step.label}
                className="min-w-0 flex-[0_0_86%] sm:flex-[0_0_62%] md:flex-[0_0_45%]"
              >
                <motion.button
                  type="button"
                  onClick={() => selectStep(index)}
                  animate={
                    prefersReducedMotion
                      ? { opacity: active ? 1 : 0.42 }
                      : {
                          opacity: active ? 1 : 0.42,
                          scale: active ? 1 : 0.92,
                          y: active ? 0 : 8,
                        }
                  }
                  transition={{ duration: 0.35, ease: "easeOut" }}
                  className="h-full w-full cursor-pointer text-left"
                >
                  <div className="min-h-[178px] rounded-2xl border border-border/60 bg-card p-5 shadow-sm sm:p-6">
                    <div className="mb-4 flex items-center justify-between gap-3">
                      <span className="rounded-full bg-primary/10 px-2.5 py-1 text-[10px] font-semibold tracking-[0.16em] text-primary uppercase">
                        Step {String(index + 1).padStart(2, "0")}
                      </span>
                      <span className="text-xs font-medium text-muted-foreground">
                        {index + 1} / {steps.length}
                      </span>
                    </div>
                    <h3 className="text-xl font-semibold sm:text-2xl">{title}</h3>
                    <p className="mt-3 line-clamp-3 text-sm leading-relaxed text-muted-foreground">
                      {step.detail}
                    </p>
                  </div>
                </motion.button>
              </div>
            );
          })}
        </div>
      </div>

      <p className="mt-3 text-center text-xs text-muted-foreground/70">
        Drag to explore · Select a stage to read the full story
      </p>
    </section>
  );
}
