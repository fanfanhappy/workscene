import { useEffect, useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  AlertTriangle,
  CheckCircle2,
  Info,
  Search,
  X,
  XCircle,
} from "lucide-react";
import { useUiStore, type ToastTone } from "@/stores/ui.store";

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "tinted";

const buttonStyles: Record<ButtonVariant, string> = {
  primary: "apple-btn-primary border border-transparent",
  tinted:
    "bg-accent/14 text-accent border border-accent/25 hover:bg-accent/22 active:scale-[0.985]",
  secondary:
    "bg-surface text-ink border border-line-strong/80 hover:bg-raised/70 active:scale-[0.985]",
  ghost:
    "bg-transparent text-ink-2 border border-transparent hover:bg-raised/70 hover:text-ink",
  danger:
    "bg-danger/12 text-danger border border-danger/30 hover:bg-danger/20 active:scale-[0.985]",
};

export function Button({
  variant = "secondary",
  size = "md",
  disabled,
  onClick,
  children,
  title,
  ariaLabel,
  className = "",
  type = "button",
  autoFocus = false,
}: {
  variant?: ButtonVariant;
  size?: "xs" | "sm" | "md";
  disabled?: boolean;
  onClick?: (e: React.MouseEvent<HTMLButtonElement>) => void;
  children: ReactNode;
  title?: string;
  ariaLabel?: string;
  className?: string;
  type?: "button" | "submit";
  /** 弹窗打开时优先获得焦点（由 Dialog 读取 data-autofocus，而不是浏览器原生 autoFocus） */
  autoFocus?: boolean;
}) {
  const height =
    size === "xs"
      ? "h-[26px] px-2.5 text-[12px] rounded-[7px]"
      : size === "sm"
        ? "h-[30px] px-3 text-[12px] rounded-[8px]"
        : "h-[34px] px-3.5 text-body rounded-[9px]";
  return (
    <button
      type={type}
      title={title}
      aria-label={ariaLabel ?? (typeof children === "string" ? undefined : title)}
      disabled={disabled}
      onClick={onClick}
      data-autofocus={autoFocus ? "true" : undefined}
      className={`inline-flex shrink-0 cursor-pointer items-center justify-center gap-1.5 font-medium transition-all duration-150 disabled:cursor-not-allowed disabled:opacity-40 ${height} ${buttonStyles[variant]} ${className}`}
    >
      {children}
    </button>
  );
}

export function Card({
  children,
  className = "",
  padded = true,
  interactive = false,
  onClick,
}: {
  children: ReactNode;
  className?: string;
  padded?: boolean;
  interactive?: boolean;
  onClick?: () => void;
}) {
  return (
    <section
      onClick={onClick}
      className={`apple-card rounded-[12px] border border-line bg-surface ${
        padded ? "p-3.5" : ""
      } ${
        interactive
          ? "apple-card-interactive cursor-pointer transition-[border-color,box-shadow,transform] duration-150 hover:border-line-strong active:scale-[0.995]"
          : ""
      } ${className}`}
    >
      {children}
    </section>
  );
}

export type Tone = "neutral" | "accent" | "success" | "warning" | "danger";

const toneStyles: Record<Tone, string> = {
  neutral: "text-ink-2 bg-raised/75 border-transparent",
  accent: "text-accent bg-accent/14 border-transparent",
  success: "text-success bg-success/14 border-transparent",
  warning: "text-warning bg-warning/14 border-transparent",
  danger: "text-danger bg-danger/14 border-transparent",
};

export function Badge({
  tone = "neutral",
  children,
  className = "",
}: {
  tone?: Tone;
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={`inline-flex shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium leading-tight ${toneStyles[tone]} ${className}`}
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
    <div className="flex flex-col items-center justify-center gap-2.5 rounded-[12px] border border-dashed border-line-strong/60 bg-surface/50 px-6 py-10 text-center">
      {icon ? (
        <div className="grid size-10 place-items-center rounded-[10px] bg-raised/80 text-ink-3">
          {icon}
        </div>
      ) : null}
      <p className="text-card font-semibold text-ink">{title}</p>
      {description ? (
        <p className="max-w-sm text-aux leading-relaxed text-ink-3">{description}</p>
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

/** 轻量 Apple 风格页面加载骨架屏，避免切页时布局闪烁 */
export function PageSkeleton() {
  return (
    <div className="mx-auto flex max-w-[1040px] flex-col gap-3.5 animate-pulse">
      <div className="h-8 w-52 rounded-[9px] bg-raised/70" />
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {[0, 1, 2].map((i) => (
          <div
            key={i}
            className="h-36 rounded-[12px] border border-line bg-surface p-4"
          >
            <div className="h-4 w-2/3 rounded-[6px] bg-raised/80" />
            <div className="mt-3 h-3 w-1/2 rounded-[6px] bg-raised/60" />
            <div className="mt-7 h-7 w-24 rounded-[8px] bg-raised/70" />
          </div>
        ))}
      </div>
    </div>
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
    <div className="mb-2 flex min-h-[26px] items-center justify-between gap-3">
      <div className="min-w-0">
        <h2 className="text-card font-semibold tracking-tight text-ink">{title}</h2>
        {hint ? <p className="mt-0.5 text-[11px] text-ink-3">{hint}</p> : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </div>
  );
}

export function Metric({
  label,
  value,
  unit,
  tone,
}: {
  label: string;
  value: string | number;
  unit?: string;
  tone?: Tone;
}) {
  const valueTone =
    tone === "success"
      ? "text-success"
      : tone === "warning"
        ? "text-warning"
        : tone === "danger"
          ? "text-danger"
          : tone === "accent"
            ? "text-accent"
            : "text-ink";
  return (
    <div className="min-w-16">
      <div className="flex items-baseline gap-1">
        <span className={`tabnum text-[22px] font-semibold leading-none tracking-tight ${valueTone}`}>
          {value}
        </span>
        {unit ? <span className="text-[11px] text-ink-3">{unit}</span> : null}
      </div>
      <p className="mt-1 text-[11px] text-ink-3">{label}</p>
    </div>
  );
}

export function ErrorNote({
  message,
  onDismiss,
}: {
  message: string;
  onDismiss?: () => void;
}) {
  return (
    <div
      role="alert"
      className="flex items-start justify-between gap-3 rounded-[10px] border border-danger/30 bg-danger/10 px-3.5 py-2.5 text-aux text-danger"
    >
      <div className="flex items-start gap-2">
        <XCircle size={15} className="mt-0.5 shrink-0" />
        <span className="selectable leading-relaxed">{message}</span>
      </div>
      {onDismiss ? (
        <button
          type="button"
          onClick={onDismiss}
          aria-label="关闭错误提示"
          className="rounded-md p-0.5 text-danger/80 hover:bg-danger/15 hover:text-danger"
        >
          <X size={14} />
        </button>
      ) : null}
    </div>
  );
}

/** 苹果风格系统开关（Apple System Green） */
export function Switch({
  checked,
  onChange,
  disabled,
  ariaLabel,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  ariaLabel?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative shrink-0 cursor-pointer rounded-full transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-40 ${
        checked ? "bg-[#30d158]" : "bg-raised"
      }`}
      style={{ height: 22, width: 38 }}
    >
      <span
        className={`absolute top-[2px] rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.3)] transition-[left] duration-200 ${
          checked ? "left-[18px]" : "left-[2px]"
        }`}
        style={{ width: 18, height: 18 }}
      />
    </button>
  );
}

export function SearchInput({
  value,
  onChange,
  placeholder = "搜索…",
  className = "",
  ariaLabel,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  className?: string;
  ariaLabel?: string;
}) {
  return (
    <div className={`relative flex items-center ${className}`}>
      <Search size={13} className="pointer-events-none absolute left-2.5 text-ink-3" />
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={ariaLabel ?? placeholder}
        className="h-[30px] w-full rounded-[8px] border border-line bg-raised/55 pl-8 pr-7 text-aux text-ink placeholder:text-ink-3 outline-none transition-colors focus:border-accent focus:bg-surface"
      />
      {value ? (
        <button
          type="button"
          onClick={() => onChange("")}
          aria-label="清除搜索"
          className="absolute right-2 rounded-full p-0.5 text-ink-3 hover:bg-raised hover:text-ink"
        >
          <X size={12} />
        </button>
      ) : null}
    </div>
  );
}

/** Apple macOS Segmented Control 风格分段标签栏 */
export function Tabs<T extends string>({
  items,
  value,
  onChange,
}: {
  items: { id: T; label: string; count?: number; icon?: ReactNode }[];
  value: T;
  onChange: (next: T) => void;
}) {
  return (
    <div
      role="tablist"
      className="inline-flex w-fit max-w-full flex-wrap items-center gap-0.5 rounded-[9px] bg-raised/70 p-0.5"
    >
      {items.map((item) => {
        const active = item.id === value;
        return (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(item.id)}
            className={`inline-flex cursor-pointer items-center gap-1.5 rounded-[7px] px-3 py-1 text-aux font-medium transition-all duration-150 ${
              active
                ? "apple-segment-active"
                : "text-ink-2 hover:text-ink"
            }`}
          >
            {item.icon}
            <span>{item.label}</span>
            {typeof item.count === "number" ? (
              <span
                className={`tabnum text-[11px] ${
                  active ? "text-accent" : "text-ink-3"
                }`}
              >
                {item.count}
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}

/** 全局模态框 Esc 栈：确保嵌套弹窗（如编辑框上弹确认框）按 Esc 总是先关最顶层 */
const dialogEscStack: Array<() => void> = [];
let escListenerBound = false;

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** 取弹窗内当前可见、可聚焦的元素（用于初始焦点与 Tab 循环） */
function getFocusable(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (el) => !el.hasAttribute("aria-hidden") && el.getClientRects().length > 0,
  );
}

function ensureEscListener() {
  if (escListenerBound || typeof window === "undefined") return;
  escListenerBound = true;
  window.addEventListener(
    "keydown",
    (e: KeyboardEvent) => {
      if (e.key === "Escape" && dialogEscStack.length > 0) {
        e.preventDefault();
        e.stopImmediatePropagation();
        const top = dialogEscStack[dialogEscStack.length - 1];
        top?.();
      }
    },
    true,
  );
}

export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  maxWidth = "max-w-lg",
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children?: ReactNode;
  footer?: ReactNode;
  maxWidth?: string;
}) {
  const titleId = useId();
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    ensureEscListener();
    const handler = () => onCloseRef.current();
    dialogEscStack.push(handler);
    return () => {
      const idx = dialogEscStack.lastIndexOf(handler);
      if (idx >= 0) dialogEscStack.splice(idx, 1);
    };
  }, [open]);

  // 打开时把焦点移进弹窗（优先 data-autofocus 的按钮），关闭/卸载时还回原来的元素。
  // 不做这一步时焦点会留在遮罩后的页面上，Tab / Enter 会继续操作看不见的控件。
  useEffect(() => {
    if (!open) return;
    const previouslyFocused =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const panel = panelRef.current;
    const raf = requestAnimationFrame(() => {
      if (!panel) return;
      // 页面里用原生 autoFocus 指定的输入框已经拿到焦点时不要抢走（编辑器弹窗靠它定位输入框）
      const current = document.activeElement;
      if (
        current instanceof HTMLElement &&
        current !== document.body &&
        panel.contains(current)
      ) {
        return;
      }
      const preferred = panel.querySelector<HTMLElement>("[data-autofocus]");
      const body = panel.querySelector<HTMLElement>('[data-dialog-body="true"]');
      const bodyFirst = body ? getFocusable(body)[0] : undefined;
      const all = getFocusable(panel);
      // 没有明确目标时优先正文里的输入框，其次页脚主按钮（最后一个），最后才是标题栏关闭键
      const fallback = bodyFirst ?? all[all.length - 1] ?? panel;
      (preferred ?? fallback).focus();
    });
    return () => {
      cancelAnimationFrame(raf);
      if (previouslyFocused && document.contains(previouslyFocused)) {
        previouslyFocused.focus();
      }
    };
  }, [open]);

  if (!open) return null;

  /** Tab / Shift+Tab 只在弹窗内循环 */
  const trapTab = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Tab") return;
    const panel = panelRef.current;
    if (!panel) return;
    const items = getFocusable(panel);
    if (items.length === 0) {
      e.preventDefault();
      panel.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) return;
    const active = document.activeElement;
    if (e.shiftKey) {
      if (active === first || active === panel) {
        e.preventDefault();
        last.focus();
      }
    } else if (active === last) {
      e.preventDefault();
      first.focus();
    }
  };

  const content = (
    <div
      data-ws-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={trapTab}
        className={`animate-dialog flex max-h-[85vh] w-full flex-col overflow-hidden rounded-[14px] border border-line-strong/60 bg-surface shadow-overlay outline-none ${maxWidth}`}
      >
        <div className="flex items-start justify-between gap-4 border-b border-line px-5 py-3.5">
          <div className="min-w-0">
            <h3 id={titleId} className="text-[15px] font-semibold text-ink">
              {title}
            </h3>
            {description ? (
              <p className="mt-0.5 whitespace-pre-line text-aux text-ink-2">{description}</p>
            ) : null}
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭弹窗"
            className="grid size-6 cursor-pointer place-items-center rounded-full bg-raised/70 text-ink-3 hover:bg-raised hover:text-ink"
          >
            <X size={13} />
          </button>
        </div>

        {children ? (
          <div data-dialog-body="true" className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
            {children}
          </div>
        ) : null}

        {footer ? (
          <div className="flex items-center justify-end gap-2 border-t border-line bg-canvas/55 px-5 py-3">
            {footer}
          </div>
        ) : null}
      </div>
    </div>
  );

  return typeof document !== "undefined" ? createPortal(content, document.body) : content;
}

/** 苹果风格二次确认弹窗 */
export function ConfirmDialogHost() {
  const req = useUiStore((s) => s.confirmRequest);
  const resolveConfirm = useUiStore((s) => s.resolveConfirm);

  return (
    <Dialog
      open={Boolean(req)}
      onClose={() => resolveConfirm(false)}
      title={req?.title ?? ""}
      description={req?.description}
      maxWidth="max-w-sm"
      footer={
        <>
          <Button variant="secondary" onClick={() => resolveConfirm(false)}>
            {req?.cancelLabel ?? "取消"}
          </Button>
          <Button
            variant={req?.variant === "danger" ? "danger" : "primary"}
            onClick={() => resolveConfirm(true)}
            autoFocus
          >
            {req?.confirmLabel ?? "确认"}
          </Button>
        </>
      }
    >
      {req?.detail ? (
        <div
          className={`rounded-[9px] border px-3 py-2 text-aux leading-relaxed ${
            req.variant === "danger"
              ? "border-danger/25 bg-danger/8 text-ink-2"
              : "border-line bg-canvas/60 text-ink-2"
          }`}
        >
          {req.detail}
        </div>
      ) : null}
    </Dialog>
  );
}

const toastToneMeta: Record<
  ToastTone,
  { icon: typeof Info; iconCls: string }
> = {
  info: { icon: Info, iconCls: "text-accent" },
  success: { icon: CheckCircle2, iconCls: "text-success" },
  warning: { icon: AlertTriangle, iconCls: "text-warning" },
  danger: { icon: XCircle, iconCls: "text-danger" },
};

/** Apple macOS 右下角通知横幅 */
export function ToastHost() {
  const toasts = useUiStore((s) => s.toasts);
  const dismiss = useUiStore((s) => s.dismissToast);

  if (toasts.length === 0) return null;

  const toastLayer = (
    <div
      aria-live="polite"
      className="pointer-events-none fixed bottom-4 right-4 z-[70] flex w-[320px] max-w-[calc(100vw-2rem)] flex-col gap-2"
    >
      {toasts.map((t) => {
        const meta = toastToneMeta[t.tone];
        const Icon = meta.icon;
        return (
          <div
            key={t.id}
            className="animate-dialog pointer-events-auto flex items-start gap-2.5 rounded-[12px] border border-line-strong/60 bg-surface/95 px-3.5 py-2.5 shadow-overlay backdrop-blur-md"
          >
            <Icon size={16} className={`mt-0.5 shrink-0 ${meta.iconCls}`} />
            <div className="min-w-0 flex-1">
              <p className="text-body font-medium text-ink">{t.title}</p>
              {t.detail ? (
                <p className="mt-0.5 text-[11px] text-ink-3">{t.detail}</p>
              ) : null}
            </div>
            <button
              type="button"
              onClick={() => dismiss(t.id)}
              aria-label="关闭通知"
              className="rounded-full p-0.5 text-ink-3 hover:bg-raised hover:text-ink"
            >
              <X size={12} />
            </button>
          </div>
        );
      })}
    </div>
  );

  return typeof document !== "undefined" ? createPortal(toastLayer, document.body) : toastLayer;
}
