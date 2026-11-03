// One reader page: a placeholder with a spinner while it loads (online pages can take seconds),
// and "Couldn't load page N · Retry" if it fails, instead of a blank space or a broken image.
import { ImageOff } from "lucide-react";
import { useEffect, useState, type SyntheticEvent } from "react";
import { Spinner, TextButton } from "../../components/ui";
import s from "./Reader.module.css";

export function PageImage({
  src,
  page,
  width,
  height,
  fill,
  onLoad,
}: {
  src: string;
  /** 0-based. */
  page: number;
  width?: number;
  height?: number;
  /** Fill the parent (the strip sizes each item itself). */
  fill?: boolean;
  onLoad?: (e: SyntheticEvent<HTMLImageElement>) => void;
}) {
  const [state, setState] = useState<"loading" | "ok" | "error">("loading");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    setState("loading");
    setAttempt(0);
  }, [src]);
  // The page route ignores the query, so this only defeats the webview's cached failure.
  const url = attempt === 0 ? src : `${src}?retry=${attempt}`;
  return (
    <span className={s.pageWrap} data-fill={fill} style={fill ? undefined : { width, height }}>
      <img
        src={url}
        alt={`Page ${page + 1}`}
        width={width}
        height={height}
        decoding="async"
        draggable={false}
        style={state === "ok" ? undefined : { opacity: 0 }}
        onLoad={(e) => {
          setState("ok");
          onLoad?.(e);
        }}
        onError={() => setState("error")}
      />
      {state === "loading" && (
        <span className={s.pageState} role="status">
          <Spinner size={28} />
          <span>Page {page + 1}</span>
        </span>
      )}
      {state === "error" && (
        <span className={s.pageState} role="alert">
          <ImageOff size={28} />
          <span>Couldn't load page {page + 1}</span>
          <TextButton
            onClick={(e) => {
              e.stopPropagation();
              setState("loading");
              setAttempt((a) => a + 1);
            }}
          >
            Retry
          </TextButton>
        </span>
      )}
    </span>
  );
}
