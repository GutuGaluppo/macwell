import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useQuery } from "@tanstack/react-query";
import { ImageOff } from "lucide-react";
import { api } from "../lib/ipc";

const PREVIEW = 320;

/**
 * Small preview of an image file that grows on hover. The preview comes from the Rust side
 * (`image_thumbnail`, a cached 320 px JPEG), and is only requested once the row is on screen,
 * so a long list does not read every image at once.
 */
export function ImageThumb({ path, size = 40 }: { path: string; size?: number }) {
  const box = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const [rect, setRect] = useState<DOMRect | null>(null);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const io = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setVisible(true);
          io.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  const { data, isLoading } = useQuery({
    queryKey: ["thumb", path],
    queryFn: () => api.imageThumbnail(path),
    enabled: visible,
    staleTime: Infinity,
    retry: false,
  });

  // Prefer the right of the thumbnail; flip left or clamp when it would leave the window.
  const popover = () => {
    if (!rect || !data) return null;
    const margin = 12;
    let left = rect.right + margin;
    if (left + PREVIEW > window.innerWidth - margin) left = Math.max(margin, rect.left - margin - PREVIEW);
    const top = Math.min(Math.max(margin, rect.top + rect.height / 2 - PREVIEW / 2), window.innerHeight - PREVIEW - margin);
    return createPortal(
      <img
        src={data}
        alt=""
        draggable={false}
        className="pointer-events-none fixed z-[60] rounded-xl border border-glass-edge bg-surface-3 object-contain shadow-2xl"
        style={{ left, top, width: PREVIEW, maxHeight: PREVIEW }}
      />,
      document.body,
    );
  };

  return (
    <div
      ref={box}
      className="grid shrink-0 place-items-center overflow-hidden rounded-md bg-surface-3"
      style={{ width: size, height: size }}
      onMouseEnter={() => box.current && setRect(box.current.getBoundingClientRect())}
      onMouseLeave={() => setRect(null)}
    >
      {data ? <img src={data} alt="" className="h-full w-full object-cover" draggable={false} /> : isLoading || !visible ? null : <ImageOff className="size-4 text-ink-3" />}
      {popover()}
    </div>
  );
}
