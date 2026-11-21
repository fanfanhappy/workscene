import type { ReactNode } from "react";

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

const buttonStyles: Record<ButtonVariant, string> = {
  // 主按钮：深色文字 on accent（对比度 6.10:1，见评审 U3）
  primary:
    "bg-accent text-accent-ink hover:brightness-110 active:brightness-95 border border-transparent",
  secondary:
    "bg-surface text-ink border border-line-strong hover:bg-raised",
  ghost: "bg-transparent text-ink-2 border border-transparent hover:bg-raised hover:text-ink",
  danger: "bg-transparent text-danger border border-line-strong hover:bg-raised",
};

export function Button({
  variant = "secondary",
  size = "md",
  disabled,
  onClick,
  children,
  title,
  className = "",
  type = "button",
}: {
  variant?: ButtonVariant;
  size?: "sm" | "md";
  disabled?: boolean;
  onClick?: () => void;
  children: ReactNode;
  title?: string;
  className?: string;
  type?: "button" | "submit";
}) {
  const height = size === "sm" ? "h-8 px-3 text-aux" : "h-9 px-4 text-body";
  return (
    <button
      type={type}
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={`inline-flex items-center justify-center gap-2 rounded-md font-medium transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-40 ${height} ${buttonStyles[variant]} ${className}`}
    >
      {children}
    </button>
  );
}

export function Card({
  children,
  className = "",
  padded = true,
}: {
  children: ReactNode;
  className?: string;
  padded?: boolean;
}) {
  return (
    <section
      className={`rounded-lg border border-line bg-surface ${padded ? "p-5" : ""} ${className}`}
    >
      {children}
    </section>
  );
}

type Tone = "neutral" | "accent" | "success" | "warning" | "danger";

const toneStyles: Record<Tone, string> = {
  neutral: "text-ink-2 border-line",
  accent: "text-accent border-accent/40",
  success: "text-success border-success/40",
  warning: "text-warning border-warning/40",
  danger: "text-danger border-danger/40",
};

export function Badge({
  tone = "neutral",
  children,
}: {
  tone?: Tone;
  children: ReactNode;
}) {
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-sm border px-1.5 py-0.5 text-aux ${toneStyles[tone]}`}
    >
      {children}
    </span>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-line px-6 py-12 text-center">
      {icon ? <div className="text-ink-3">{icon}</div> : null}
      <p className="text-card font-medium text-ink">{title}</p>
      {description ? (
        <p className="max-w-md text-aux text-ink-3">{description}</p>
      ) : null}
      {action ? <div className="pt-1">{action}</div> : null}
    </div>
  );
}

export function Spinner({ className = "" }: { className?: string }) {
  return (
    <span
      className={`inline-block size-3.5 animate-spin rounded-full border-2 border-current border-t-transparent ${className}`}
      aria-hidden
    />
  );
}

export function SectionTitle({
  title,
  hint,
  action,
}: {
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="mb-3 flex items-end justify-between gap-4">
      <div>
        <h2 className="text-module font-semibold tracking-tight text-ink">{title}</h2>
        {hint ? <p className="mt-1 text-aux text-ink-3">{hint}</p> : null}
      </div>
      {action}
    </div>
  );
}

export function Metric({
  label,
  value,
  unit,
}: {
  label: string;
  value: string | number;
  unit?: string;
}) {
  return (
    <div className="min-w-24">
      <div className="flex items-baseline gap-1">
        <span className="tabnum text-[26px] font-semibold leading-none text-ink">{value}</span>
        {unit ? <span className="text-aux text-ink-3">{unit}</span> : null}
      </div>
      <p className="mt-1.5 text-aux text-ink-3">{label}</p>
    </div>
  );
}

export function ErrorNote({ message }: { message: string }) {
  return (
    <div
      role="alert"
      className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-aux text-danger"
    >
      {message}
    </div>
  );
}
