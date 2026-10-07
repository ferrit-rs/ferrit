"use client";

import { Maximize, Minimize, Pause, Play, Volume2, VolumeX } from "lucide-react";
import { type PointerEvent, type ReactNode, useEffect, useRef, useState } from "react";

type StickyVideoStatus = "playing" | "paused" | "loading";

type StickyVideoOverlayProps = {
  status: StickyVideoStatus;
  muted: boolean;
  fullscreen: boolean;
  currentTimeSeconds: number;
  durationSeconds: number;
  bufferedPercent: number;
  onTogglePlay: () => void;
  onToggleMute: () => void;
  onSeekStart: () => void;
  onSeek: (seconds: number) => void;
  onSeekEnd: (seconds: number) => void;
  onFullscreen: () => void;
  extraControl?: ReactNode;
};

const HOVER_IDLE_TIMEOUT_MS = 3000;
const SCRUB_SEEK_THROTTLE_MS = 180;
const ORANGE = "#f46c38";

const SOUND_PILL_CSS = `
.sticky-vsl { position: absolute; inset: 0; color: #fff; isolation: isolate; overflow: hidden; border-radius: inherit; font-size: 16px; }
.sticky-vsl__dark { position: absolute; inset: 0; background: #000; opacity: .1; pointer-events: none; transition: opacity .3s linear; }
.sticky-vsl[data-status="playing"] .sticky-vsl__dark { opacity: 0; }
.sticky-vsl[data-status="paused"] .sticky-vsl__dark,
.sticky-vsl[data-status="playing"][data-hover="active"] .sticky-vsl__dark { opacity: .3; }
.sticky-vsl__playpause { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: opacity .3s linear; border: 0; background: transparent; color: inherit; }
.sticky-vsl[data-status="playing"] .sticky-vsl__playpause,
.sticky-vsl[data-status="loading"] .sticky-vsl__playpause { opacity: 0; }
.sticky-vsl[data-status="playing"][data-hover="active"] .sticky-vsl__playpause { opacity: 1; }
.sticky-vsl__big-btn { display: flex; align-items: center; justify-content: center; width: 4em; height: 4em; padding: 1em; border-radius: 50%; background: #0f0f101f; box-shadow: 0 1px 0 rgba(255,255,255,.04) inset, 0 0 70px rgba(255,255,255,.02) inset, 0 0 12px rgba(255,255,255,.04) inset; backdrop-filter: blur(1rem); outline: 1px solid rgba(255,255,255,.08); }
.sticky-vsl__big-btn svg { width: 100%; height: 100%; opacity: .7; }
.sticky-vsl__interface { position: absolute; inset: 0; display: flex; flex-direction: column; justify-content: flex-end; pointer-events: none; transition: opacity .6s cubic-bezier(.625,.05,0,1), transform .6s cubic-bezier(.625,.05,0,1); }
.sticky-vsl[data-status="playing"] .sticky-vsl__interface,
.sticky-vsl[data-status="loading"] .sticky-vsl__interface { opacity: 0; transform: translateY(1em); }
.sticky-vsl[data-status="playing"][data-hover="active"] .sticky-vsl__interface,
.sticky-vsl[data-status="loading"][data-hover="active"] .sticky-vsl__interface { opacity: 1; transform: translateY(0); }
.sticky-vsl__fade { position: absolute; bottom: 0; width: 100%; height: 25%; opacity: .5; background: linear-gradient(rgba(0,0,0,0), #000); }
.sticky-vsl__bottom { position: relative; display: flex; align-items: center; justify-content: space-between; gap: 1em; width: 100%; padding: 1.5em; pointer-events: auto; }
.sticky-vsl__icon-btn { flex: none; width: 1.5em; height: 1.5em; cursor: pointer; border: 0; background: transparent; color: inherit; padding: 0; }
.sticky-vsl__icon-btn svg { width: 100%; height: 100%; }
.sticky-vsl__time { flex: none; display: flex; align-items: center; justify-content: center; gap: .125em; width: 5.75em; font-size: .9375em; line-height: 1; white-space: nowrap; font-variant-numeric: tabular-nums; }
.sticky-vsl__time-dim { opacity: .5; }
.sticky-vsl__timeline { position: relative; flex: 1 1 0%; display: flex; align-items: center; height: 1em; margin: 0 .5em; cursor: pointer; touch-action: none; }
.sticky-vsl__timeline-bar { position: absolute; width: 100%; height: 30%; border-radius: 1em; overflow: hidden; }
.sticky-vsl__timeline-bar > div { position: absolute; inset: 0; border-radius: 1em; pointer-events: none; }
.sticky-vsl__timeline-bg { background: rgba(255,255,255,.15); }
.sticky-vsl__timeline-buffered { background: #fff; opacity: .2; }
.sticky-vsl__timeline-handle { position: absolute; top: 50%; width: 1em; height: 1em; border-radius: 1em; pointer-events: none; transform: translate(-50%, -50%) scale(0); transition: transform .15s ease-in-out; }
.sticky-vsl[data-drag="true"] .sticky-vsl__timeline-handle { transform: translate(-50%, -50%) scale(1); }
.sticky-vsl__loading { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,.33); opacity: 0; visibility: hidden; pointer-events: none; transition: opacity .3s linear, visibility .3s linear; }
.sticky-vsl[data-status="loading"] .sticky-vsl__loading { opacity: 1; visibility: visible; }
.sticky-vsl__loading svg { width: 6em; animation: sticky-vsl-spin 1s linear infinite; }
.sticky-vsl__unmute { position: absolute; top: 0; right: 0; z-index: 4; padding: 1em; cursor: pointer; border: 0; background: transparent; color: inherit; }
.sticky-vsl__unmute-wrap { animation: sticky-vsl-nudge 3.5s infinite; }
.sticky-vsl__unmute-tag { display: flex; align-items: center; gap: .375em; padding: .375em .75em; border-radius: 999px; background: rgba(15,15,16,.24); backdrop-filter: blur(5px); outline: 1px solid rgba(255,255,255,.08); box-shadow: 0 1px 0 rgba(255,255,255,.04) inset, 0 0 70px rgba(255,255,255,.02) inset, 0 0 12px rgba(255,255,255,.04) inset; font-size: .875em; font-weight: 500; line-height: 1.3; white-space: nowrap; animation: sticky-vsl-breathe 3.2s infinite; transition: transform .2s cubic-bezier(.32,.72,0,1); }
.sticky-vsl__unmute:hover .sticky-vsl__unmute-tag { animation: none; transform: scale(1.05); }
.sticky-vsl__unmute-tag svg { width: 1.125em; height: 1.125em; flex: none; }
.sticky-vsl__speaker-wave { animation: sticky-vsl-wave 1.8s infinite; }
.sticky-vsl__speaker-wave + .sticky-vsl__speaker-wave { animation-delay: .3s; }
@keyframes sticky-vsl-spin { to { transform: rotate(360deg); } }
@keyframes sticky-vsl-wave { 0%, 100% { opacity: 0; } 40%, 70% { opacity: 1; } }
@keyframes sticky-vsl-breathe { 0%, 100% { transform: scale(1); } 50% { transform: scale(1.04); } }
@keyframes sticky-vsl-nudge { 0%, 82%, 100% { transform: rotate(0); } 85% { transform: rotate(-3deg); } 88% { transform: rotate(3deg); } 91% { transform: rotate(-2deg); } 94% { transform: rotate(1.5deg); } 97% { transform: rotate(0); } }
@media (max-width: 767px) { .sticky-vsl__bottom { padding: 1em; font-size: .875em; } }
@media (prefers-reduced-motion: reduce) { .sticky-vsl__unmute-wrap, .sticky-vsl__unmute-tag, .sticky-vsl__speaker-wave { animation: none; } .sticky-vsl__speaker-wave { opacity: 1; } }
`;

function formatTimestamp(totalSeconds: number) {
  const wholeSeconds = Number.isFinite(totalSeconds)
    ? Math.max(0, Math.floor(totalSeconds))
    : 0;
  return `${String(Math.floor(wholeSeconds / 60)).padStart(2, "0")}:${String(wholeSeconds % 60).padStart(2, "0")}`;
}

function clampPercent(percent: number) {
  return Math.min(100, Math.max(0, percent));
}

export function StickyVideoOverlay({
  status,
  muted,
  fullscreen,
  currentTimeSeconds,
  durationSeconds,
  bufferedPercent,
  onTogglePlay,
  onToggleMute,
  onSeekStart,
  onSeek,
  onSeekEnd,
  onFullscreen,
  extraControl,
}: StickyVideoOverlayProps) {
  const [hover, setHover] = useState<"active" | "idle">("idle");
  const [dragPercent, setDragPercent] = useState<number | null>(null);
  const hoverTimerRef = useRef<number | null>(null);
  const lastScrubSeekAtRef = useRef(0);

  useEffect(
    () => () => {
      if (hoverTimerRef.current !== null) window.clearTimeout(hoverTimerRef.current);
    },
    [],
  );

  function activateHover() {
    setHover("active");
    if (hoverTimerRef.current !== null) window.clearTimeout(hoverTimerRef.current);
    hoverTimerRef.current = window.setTimeout(
      () => setHover("idle"),
      HOVER_IDLE_TIMEOUT_MS,
    );
  }

  function pointerLeave(event: PointerEvent<HTMLDivElement>) {
    if (event.pointerType === "mouse") setHover("idle");
  }

  function percentFromPointer(event: PointerEvent<HTMLDivElement>) {
    const rect = event.currentTarget.getBoundingClientRect();
    return rect.width === 0
      ? 0
      : clampPercent(((event.clientX - rect.left) / rect.width) * 100);
  }

  function secondsFromPercent(percent: number) {
    return (percent / 100) * durationSeconds;
  }

  function handlePointerDown(event: PointerEvent<HTMLDivElement>) {
    if (durationSeconds <= 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    const percent = percentFromPointer(event);
    setDragPercent(percent);
    onSeekStart();
    lastScrubSeekAtRef.current = Date.now();
    onSeek(secondsFromPercent(percent));
  }

  function handlePointerMove(event: PointerEvent<HTMLDivElement>) {
    if (dragPercent === null) return;
    const percent = percentFromPointer(event);
    setDragPercent(percent);
    if (Date.now() - lastScrubSeekAtRef.current >= SCRUB_SEEK_THROTTLE_MS) {
      lastScrubSeekAtRef.current = Date.now();
      onSeek(secondsFromPercent(percent));
    }
  }

  function handlePointerUp(event: PointerEvent<HTMLDivElement>) {
    if (dragPercent === null) return;
    setDragPercent(null);
    onSeekEnd(secondsFromPercent(percentFromPointer(event)));
  }

  const playedPercent =
    dragPercent ??
    (durationSeconds > 0
      ? clampPercent((currentTimeSeconds / durationSeconds) * 100)
      : 0);
  const displayedTime =
    dragPercent === null ? currentTimeSeconds : secondsFromPercent(dragPercent);
  const isPlayingLike = status !== "paused";

  return (
    <div
      className="sticky-vsl select-none"
      data-status={status}
      data-hover={hover}
      data-drag={dragPercent === null ? "false" : "true"}
      onPointerEnter={activateHover}
      onPointerMove={activateHover}
      onPointerDown={activateHover}
      onPointerLeave={pointerLeave}
    >
      <style>{SOUND_PILL_CSS}</style>
      <div className="sticky-vsl__dark" />
      {extraControl}
      <button
        type="button"
        className="sticky-vsl__playpause"
        aria-label={isPlayingLike ? "Pause video" : "Play video"}
        onClick={onTogglePlay}
      >
        <span className="sticky-vsl__big-btn">
          {isPlayingLike ? <Pause /> : <Play />}
        </span>
      </button>
      <div className="sticky-vsl__interface">
        <div className="sticky-vsl__fade" />
        <div className="sticky-vsl__bottom">
          <button
            type="button"
            className="sticky-vsl__icon-btn"
            aria-label={isPlayingLike ? "Pause video" : "Play video"}
            onClick={onTogglePlay}
          >
            {isPlayingLike ? <Pause /> : <Play />}
          </button>
          <div className="sticky-vsl__time">
            <span>{formatTimestamp(displayedTime)}</span>
            <span className="sticky-vsl__time-dim">/</span>
            <span className="sticky-vsl__time-dim">
              {formatTimestamp(durationSeconds)}
            </span>
          </div>
          <div
            className="sticky-vsl__timeline"
            role="slider"
            tabIndex={-1}
            aria-label="Seek"
            aria-valuemin={0}
            aria-valuemax={Math.round(durationSeconds)}
            aria-valuenow={Math.round(displayedTime)}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerUp}
          >
            <div className="sticky-vsl__timeline-bar">
              <div className="sticky-vsl__timeline-bg" />
              <div
                className="sticky-vsl__timeline-buffered"
                style={{
                  transform: `translateX(${clampPercent(bufferedPercent) - 100}%)`,
                }}
              />
              <div
                style={{
                  background: ORANGE,
                  transform: `translateX(${playedPercent - 100}%)`,
                }}
              />
            </div>
            <div
              className="sticky-vsl__timeline-handle"
              style={{ left: `${playedPercent}%`, background: ORANGE }}
            />
          </div>
          <button
            type="button"
            className="sticky-vsl__icon-btn"
            aria-label={muted ? "Unmute video" : "Mute video"}
            onClick={onToggleMute}
          >
            {muted ? <VolumeX /> : <Volume2 />}
          </button>
          <button
            type="button"
            className="sticky-vsl__icon-btn"
            aria-label={fullscreen ? "Exit fullscreen" : "Fullscreen"}
            onClick={onFullscreen}
          >
            {fullscreen ? <Minimize /> : <Maximize />}
          </button>
        </div>
      </div>
      <div className="sticky-vsl__loading">
        <svg viewBox="0 0 100 100" fill="none" aria-hidden="true">
          <path
            fill="currentColor"
            d="M73,50c0-12.7-10.3-23-23-23S27,37.3,27,50 M30.9,50c0-10.5,8.5-19.1,19.1-19.1S69.1,39.5,69.1,50"
          />
        </svg>
      </div>
      {muted ? (
        <button type="button" className="sticky-vsl__unmute" onClick={onToggleMute}>
          <span className="sticky-vsl__unmute-wrap block">
            <span className="sticky-vsl__unmute-tag">
              <Volume2 />
              Turn on sound
            </span>
          </span>
        </button>
      ) : null}
    </div>
  );
}
