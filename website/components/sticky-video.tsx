"use client";

import { X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { StickyVideoOverlay } from "@/components/sticky-video-overlay";
import { useStickyVideo } from "@/hooks/use-sticky-video";

const VIDEO_SRC = "/videos/ferrit-film.mp4";

type VideoStatus = "playing" | "paused" | "loading";

export function StickyVideo() {
  const videoRef = useRef<HTMLVideoElement>(null);
  const frameRef = useRef<HTMLDivElement>(null);
  const shellRef = useRef<HTMLDivElement>(null);
  const previousShellRectRef = useRef<DOMRect | null>(null);
  const wasPlayingBeforeSeekRef = useRef(false);
  const { isFloating, spacerRef, spacerStyle, videoStyle } = useStickyVideo();
  const previousFloatingRef = useRef(isFloating);
  const [status, setStatus] = useState<VideoStatus>("loading");
  const [muted, setMuted] = useState(true);
  const [fullscreen, setFullscreen] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [bufferedPercent, setBufferedPercent] = useState(0);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = true;
    video.defaultMuted = true;
    void video.play().catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!isFloating) setDismissed(false);
  }, [isFloating]);

  useEffect(() => {
    const onFullscreenChange = () => {
      setFullscreen(document.fullscreenElement === frameRef.current);
    };
    document.addEventListener("fullscreenchange", onFullscreenChange);
    return () => document.removeEventListener("fullscreenchange", onFullscreenChange);
  }, []);

  useLayoutEffect(() => {
    const shell = shellRef.current;
    if (!shell) return;

    const nextRect = shell.getBoundingClientRect();
    const previousRect = previousShellRectRef.current;
    if (previousRect && previousFloatingRef.current !== isFloating) {
      const scaleX = previousRect.width / nextRect.width;
      const scaleY = previousRect.height / nextRect.height;
      const offsetX = previousRect.left - nextRect.left;
      const offsetY = previousRect.top - nextRect.top;
      shell.animate(
        [
          {
            transform: `translate3d(${offsetX}px, ${offsetY}px, 0) scale(${scaleX}, ${scaleY})`,
          },
          { transform: "translate3d(0, 0, 0) scale(1, 1)" },
        ],
        {
          duration: 620,
          easing: "cubic-bezier(.16, 1, .3, 1)",
        },
      );
    }
    previousShellRectRef.current = nextRect;
    previousFloatingRef.current = isFloating;
  }, [isFloating]);

  function updateProgress(video: HTMLVideoElement) {
    setCurrentTime(video.currentTime);
    setDuration(video.duration || 0);
    const bufferedEnd =
      video.buffered.length > 0 ? video.buffered.end(video.buffered.length - 1) : 0;
    setBufferedPercent(video.duration > 0 ? (bufferedEnd / video.duration) * 100 : 0);
  }

  function togglePlay() {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) {
      void video.play().catch(() => undefined);
    } else {
      video.pause();
    }
  }

  function toggleMute() {
    const video = videoRef.current;
    if (!video) return;
    const nextMuted = !muted;
    video.muted = nextMuted;
    setMuted(nextMuted);
  }

  function seekStart() {
    const video = videoRef.current;
    if (!video) return;
    wasPlayingBeforeSeekRef.current = !video.paused;
    if (wasPlayingBeforeSeekRef.current) video.pause();
  }

  function seek(seconds: number) {
    if (videoRef.current) videoRef.current.currentTime = seconds;
  }

  function seekEnd(seconds: number) {
    const video = videoRef.current;
    if (!video) return;
    video.currentTime = seconds;
    if (wasPlayingBeforeSeekRef.current) void video.play().catch(() => undefined);
    wasPlayingBeforeSeekRef.current = false;
  }

  function toggleFullscreen() {
    if (document.fullscreenElement) {
      void document.exitFullscreen().catch(() => undefined);
      return;
    }
    void frameRef.current?.requestFullscreen().catch(() => undefined);
  }

  function closeSticky() {
    setDismissed(true);
    videoRef.current?.pause();
  }

  const closeButton = isFloating ? (
    <button
      type="button"
      aria-label="Close video"
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => {
        event.stopPropagation();
        closeSticky();
      }}
      className="sticky-video-close absolute top-2 left-2 z-20 flex size-7 items-center justify-center rounded-full text-white"
    >
      <X className="size-3.5" strokeWidth={2.5} />
    </button>
  ) : null;

  return (
    <>
      <style>{`
        .sticky-video-reveal { animation: sticky-video-reveal .7s cubic-bezier(.16,1,.3,1) both; }
        @keyframes sticky-video-reveal { from { opacity: 0; transform: translateY(20px); filter: blur(6px); } to { opacity: 1; transform: translateY(0); filter: blur(0); } }
        .sticky-video-shell { transition: opacity .45s cubic-bezier(.16,1,.3,1), transform .45s cubic-bezier(.16,1,.3,1); }
        .sticky-video-shell[data-dismissed="true"] { opacity: 0; transform: translateY(24px) scale(.9); pointer-events: none; }
        .sticky-video-close { z-index: 30; pointer-events: auto; background: rgba(15,15,16,.24); backdrop-filter: blur(5px); outline: 1px solid rgba(255,255,255,.08); outline-offset: -1px; box-shadow: 0 2px 8px rgba(0,0,0,.12); opacity: 0; transform: scale(.85); transition: opacity .2s ease, transform .2s cubic-bezier(.32,.72,0,1), background-color .2s ease; }
        .sticky-video-shell:hover .sticky-video-close, .sticky-video-close:focus-visible { opacity: 1; transform: scale(1); }
        .sticky-video-shell:hover .sticky-video-close:hover { background: rgba(15,15,16,.32); transform: scale(1.06); }
        @media (hover: none) { .sticky-video-close { opacity: 1; transform: scale(1); } }
        [data-sticky-video-frame]:fullscreen { border-radius: 0; }
      `}</style>
      <div ref={spacerRef} style={spacerStyle} className="relative w-full">
        <div
          ref={shellRef}
          data-sticky-video
          data-sticky={isFloating ? "true" : "false"}
          data-dismissed={isFloating && dismissed ? "true" : "false"}
          className="sticky-video-shell sticky-video-reveal overflow-hidden rounded-2xl border border-border bg-black shadow-2xl shadow-black/10"
          style={videoStyle}
        >
          <div
            ref={frameRef}
            data-sticky-video-frame
            className="relative aspect-video overflow-hidden bg-black"
          >
            <video
              ref={videoRef}
              src={VIDEO_SRC}
              title="Video preview"
              className="absolute inset-0 h-full w-full object-cover"
              autoPlay
              muted
              loop
              playsInline
              preload="metadata"
              onLoadedMetadata={(event) => setDuration(event.currentTarget.duration)}
              onCanPlay={() =>
                setStatus((current) => (current === "loading" ? "playing" : current))
              }
              onPlaying={() => setStatus("playing")}
              onPause={() => setStatus("paused")}
              onWaiting={() => setStatus("loading")}
              onTimeUpdate={(event) => updateProgress(event.currentTarget)}
              onProgress={(event) => updateProgress(event.currentTarget)}
            />
            <StickyVideoOverlay
              status={status}
              muted={muted}
              fullscreen={fullscreen}
              currentTimeSeconds={currentTime}
              durationSeconds={duration}
              bufferedPercent={bufferedPercent}
              onTogglePlay={togglePlay}
              onToggleMute={toggleMute}
              onSeekStart={seekStart}
              onSeek={seek}
              onSeekEnd={seekEnd}
              onFullscreen={toggleFullscreen}
            />
            {closeButton}
          </div>
        </div>
      </div>
    </>
  );
}
