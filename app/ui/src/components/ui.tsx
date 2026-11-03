// Shared components from spec §2 (buttons §2.7, badges §2.8, chips §2.9, toggle §2.14).
import { AlertCircle, Download, Loader2, type LucideIcon } from "lucide-react";
import { useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import { coverUrl } from "../lib/tmUrl";
import s from "./ui.module.css";

type BtnProps = ButtonHTMLAttributes<HTMLButtonElement>;

/** `busy` shows a spinner in place of the icon, disables the button and marks it `aria-busy`. */
type Busy = { busy?: boolean };

export function IconButton({
  icon: Icon,
  label,
  active,
  size = 20,
  busy,
  disabled,
  ...rest
}: BtnProps & Busy & { icon: LucideIcon; label: string; active?: boolean; size?: number }) {
  return (
    <button
      type="button"
      className={s.iconBtn}
      aria-label={label}
      title={label}
      data-active={active}
      aria-busy={busy || undefined}
      disabled={disabled || busy}
      {...rest}
    >
      {busy ? <Spinner size={size} /> : <Icon size={size} strokeWidth={1.8} />}
    </button>
  );
}

function BusyButton({ cls, busy, disabled, children, ...rest }: BtnProps & Busy & { cls: string | undefined }) {
  return (
    <button type="button" className={cls} aria-busy={busy || undefined} disabled={disabled || busy} {...rest}>
      {busy && <Spinner size={14} />}
      {children}
    </button>
  );
}

export function PrimaryButton(props: BtnProps & Busy) {
  return <BusyButton cls={s.primaryBtn} {...props} />;
}

export function OutlinedButton(props: BtnProps & Busy) {
  return <BusyButton cls={s.outlinedBtn} {...props} />;
}

export function TextButton(props: BtnProps & Busy) {
  return <BusyButton cls={s.textBtn} {...props} />;
}

/** The app's one spinner. */
export function Spinner({ size = 16, label }: { size?: number; label?: string }) {
  return <Loader2 size={size} className={s.spinner} aria-label={label} aria-hidden={label ? undefined : true} />;
}

/** A shimmering placeholder block in the shape of what is loading. */
export function Skeleton({ width, height = 14, radius, className }: { width?: number | string; height?: number | string; radius?: string; className?: string }) {
  return <span className={`${s.skeleton} ${className ?? ""}`} style={{ width, height, borderRadius: radius }} aria-hidden="true" />;
}

/** Placeholder list rows: a cover or icon, a title line and a detail line. */
export function SkeletonRows({ count = 8, cover = true }: { count?: number; cover?: boolean }) {
  return (
    <div className={s.skeletonList} aria-busy="true" aria-label="Loading">
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className={s.skeletonRow}>
          {cover && <Skeleton width={40} height={56} radius="var(--tm-radius-sm)" />}
          <div className={s.skeletonLines}>
            <Skeleton width={`${55 + ((i * 17) % 35)}%`} />
            <Skeleton width={`${25 + ((i * 11) % 25)}%`} height={11} />
          </div>
        </div>
      ))}
    </div>
  );
}

/** Placeholder cover grid. */
export function SkeletonGrid({ count = 18 }: { count?: number }) {
  return (
    <div className={s.skeletonGrid} aria-busy="true" aria-label="Loading">
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className={s.skeletonTile}>
          <Skeleton width="100%" height="auto" className={s.skeletonCover} radius="var(--tm-radius-sm)" />
          <Skeleton width="80%" height={12} />
        </div>
      ))}
    </div>
  );
}

export function Chip({ active, count, children, ...rest }: BtnProps & { active?: boolean; count?: number }) {
  return (
    <button type="button" className={s.chip} data-active={active} aria-pressed={active} {...rest}>
      {children}
      {count !== undefined && <span className={s.chipCount}>{count}</span>}
    </button>
  );
}

export function UnreadBadge({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <span className={s.badge} aria-label={`${count} unread`}>
      {count}
    </span>
  );
}

export function DownloadBadge({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <span className={`${s.badge} ${s.badgeDownload}`} aria-label={`${count} downloaded`}>
      <Download size={12} />
      {count}
    </span>
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={s.toggle}
      onClick={() => onChange(!checked)}
    />
  );
}

/** Cover with a gradient placeholder and the title's initial until (or unless) the image loads. */
export function Cover({
  mangaId,
  version,
  title,
  width,
  height,
  className,
  src,
}: {
  mangaId?: number;
  version?: number;
  /** An image URL to show instead of the library cover. */
  src?: string | null;
  title: string;
  width?: number;
  height?: number;
  className?: string;
}) {
  const [state, setState] = useState<"loading" | "ok" | "error">(src === null ? "error" : "loading");
  const url = src ?? (mangaId !== undefined ? coverUrl(mangaId, version ?? 0) : null);
  return (
    <div className={`${s.cover} ${className ?? ""}`} style={width ? { width, height } : undefined}>
      {state !== "ok" && <span className={s.coverInitial}>{title.trim().charAt(0).toUpperCase()}</span>}
      {state !== "error" && url && (
        <img
          src={url}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          onLoad={() => setState("ok")}
          onError={() => setState("error")}
          style={state === "ok" ? undefined : { opacity: 0 }}
        />
      )}
    </div>
  );
}

export function EmptyState({ icon: Icon, title, detail, action }: { icon: LucideIcon; title: string; detail?: string; action?: ReactNode }) {
  return (
    <div className={s.state}>
      <Icon size={48} className={s.stateIcon} strokeWidth={1.4} />
      <div className={s.stateTitle}>{title}</div>
      {detail && <div className={s.stateDetail}>{detail}</div>}
      {action}
    </div>
  );
}

export function ErrorState({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <EmptyState
      icon={AlertCircle}
      title="Something went wrong"
      detail={message}
      action={onRetry && <TextButton onClick={onRetry}>Try again</TextButton>}
    />
  );
}

/** A centred spinner, with a line saying what is loading when it may take a while. */
export function Loading({ label }: { label?: string }) {
  return (
    <div className={s.state} aria-busy="true" role="status">
      <Loader2 size={28} className={s.spinner} />
      {label && <div className={s.stateDetail}>{label}</div>}
    </div>
  );
}
