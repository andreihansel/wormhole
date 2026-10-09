import { useEffect, useRef, useState } from "react";

const VIDEO = "/media/black-hole.mp4";
const POSTER = "/media/black-hole-poster.webp";

export default function VaultBackdrop({ still = false }: { still?: boolean }) {
  const media = useRef<HTMLVideoElement>(null);
  const unavailable = useRef(false);
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const [reduced, setReduced] = useState(
    () =>
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false,
  );

  useEffect(() => {
    const video = media.current!;
    const preference = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    let active = true;
    let playbackAttempt = 0;
    unavailable.current = false;
    function synchronize() {
      const attempt = ++playbackAttempt;
      if (
        still ||
        preference?.matches ||
        document.hidden ||
        unavailable.current
      ) {
        video.pause();
        return;
      }
      // Delay even the local media request until motion is permitted.
      if (!video.hasAttribute("src")) video.src = VIDEO;
      video.play().catch((error: unknown) => {
        // A pause aborts pending play() in Chromium/WebKit. It is routine
        // visibility/motion handling, not a decoder failure. Older attempts
        // must also be unable to hide a subsequent successful resume.
        if (!active || attempt !== playbackAttempt) return;
        if (error instanceof DOMException && error.name === "AbortError")
          return;
        unavailable.current = true;
        video.pause();
        setFailed(true);
      });
    }
    function motionChanged() {
      setReduced(preference?.matches ?? false);
      synchronize();
    }
    preference?.addEventListener("change", motionChanged);
    document.addEventListener("visibilitychange", synchronize);
    synchronize();
    return () => {
      active = false;
      preference?.removeEventListener("change", motionChanged);
      document.removeEventListener("visibilitychange", synchronize);
      video.pause();
      video.removeAttribute("src");
      video.load();
    };
  }, [still]);

  return (
    <div className="vault-backdrop" aria-hidden="true">
      <img src={POSTER} alt="" className="vault-poster" />
      <video
        ref={media}
        poster={POSTER}
        muted
        loop
        playsInline
        controls={false}
        disablePictureInPicture
        disableRemotePlayback
        tabIndex={-1}
        aria-hidden="true"
        hidden={still || reduced || failed}
        preload="none"
        className={ready ? "is-ready" : ""}
        onCanPlay={() => setReady(true)}
        onError={() => {
          unavailable.current = true;
          media.current?.pause();
          setFailed(true);
        }}
      />
      <div className="vault-scrim" />
    </div>
  );
}
