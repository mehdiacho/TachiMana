// Shared components from spec §2 (buttons §2.7, badges §2.8, chips §2.9, toggle §2.14).
import { AlertCircle, Download, Loader2, type LucideIcon } from "lucide-react";
import { useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import { coverUrl } from "../lib/tmUrl";
import s from "./ui.module.css";

type BtnProps = ButtonHTMLAttributes<HTMLButtonElement>;

export function IconButton({
  icon: Icon,
  label,
  active,
  size = 20,
  ...rest
}: BtnProps & { icon: LucideIcon; label: string; active?: boolean; size?: number }) {
  return (
    <button type="button" className={s.iconBtn} aria-label={label} title={label} data-active={active} {...rest}>
      <Icon size={size} strokeWidth={1.8} />
    </button>
  );
}

export function PrimaryButton({ children, ...rest }: BtnProps) {
  return (
    <button type="button" className={s.primaryBtn} {...rest}>
      {children}
    </button>
  );
}

export function OutlinedButton({ children, ...rest }: BtnProps) {
  return (
    <button type="button" className={s.outlinedBtn} {...rest}>
      {children}
    </button>
  );
}

export function TextButton({ children, ...rest }: BtnProps) {
  return (
    <button type="button" className={s.textBtn} {...rest}>
      {children}
    </button>
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
}: {
  mangaId: number;
  version: number;
  title: string;
  width?: number;
  height?: number;
  className?: string;
}) {
  const [state, setState] = useState<"loading" | "ok" | "error">("loading");
  return (
    <div className={`${s.cover} ${className ?? ""}`} style={width ? { width, height } : undefined}>
      {state !== "ok" && <span className={s.coverInitial}>{title.trim().charAt(0).toUpperCase()}</span>}
      {state !== "error" && (
        <img
          src={coverUrl(mangaId, version)}
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

export function Loading() {
  return (
    <div className={s.state} aria-busy="true">
      <Loader2 size={28} className={s.spinner} />
    </div>
  );
}
