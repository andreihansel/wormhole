import * as Dialog from "@radix-ui/react-dialog";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Tooltip from "@radix-ui/react-tooltip";
import { Eye, EyeOff, LoaderCircle, X, type LucideIcon } from "lucide-react";
import {
  useId,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";

export function BrandMark({ className = "" }: { className?: string }) {
  return (
    <svg
      className={`brand-mark ${className}`}
      viewBox="0 0 40 40"
      fill="none"
      aria-hidden="true"
    >
      <ellipse
        cx="20"
        cy="20"
        rx="17"
        ry="9"
        transform="rotate(-35 20 20)"
        stroke="currentColor"
        strokeWidth="1.5"
      />
      <circle cx="20" cy="20" r="10" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="20" cy="20" r="3" fill="currentColor" />
    </svg>
  );
}
export function Button({
  icon: Icon,
  busy,
  children,
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  icon?: LucideIcon;
  busy?: boolean;
}) {
  return (
    <button
      {...props}
      disabled={props.disabled || busy}
      aria-busy={busy || undefined}
      className={className}
    >
      {busy ? (
        <LoaderCircle className="spin" size={16} aria-hidden="true" />
      ) : (
        Icon && <Icon size={16} aria-hidden="true" />
      )}
      {children}
    </button>
  );
}
export function Tip({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <Tooltip.Provider delayDuration={350}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>{children}</Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content className="tooltip" sideOffset={8}>
            {label}
            <Tooltip.Arrow />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}
export function IconButton({
  label,
  icon: Icon,
  ...props
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> & {
  label: string;
  icon: LucideIcon;
}) {
  return (
    <Tip label={label}>
      <button
        {...props}
        type={props.type ?? "button"}
        aria-label={label}
        className={`icon-button ${props.className ?? ""}`}
      >
        <Icon size={17} aria-hidden="true" />
      </button>
    </Tip>
  );
}
export function TextField({
  label,
  description,
  error,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & {
  label: string;
  description?: string;
  error?: string;
}) {
  const id = useId();
  return (
    <label htmlFor={id}>
      {label}
      <input
        {...props}
        id={id}
        aria-invalid={!!error || undefined}
        aria-describedby={description || error ? `${id}-help` : undefined}
      />
      {(error || description) && (
        <span
          id={`${id}-help`}
          className={error ? "error small" : "small muted"}
        >
          {error || description}
        </span>
      )}
    </label>
  );
}
export function PasswordField({
  label,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string }) {
  const [visible, setVisible] = useState(false);
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <div className="password-input">
        <input {...props} id={id} type={visible ? "text" : "password"} />
        <Tip label={`${visible ? "Hide" : "Show"} ${label}`}>
          <button
            type="button"
            className="password-toggle"
            disabled={props.disabled}
            aria-label={`${visible ? "Hide" : "Show"} ${label}`}
            aria-pressed={visible}
            onClick={() => setVisible(!visible)}
          >
            {visible ? (
              <EyeOff size={17} aria-hidden="true" />
            ) : (
              <Eye size={17} aria-hidden="true" />
            )}
          </button>
        </Tip>
      </div>
    </div>
  );
}
export function PageHeader({
  title,
  description,
  actions,
  count,
}: {
  title: string;
  description: string;
  actions?: ReactNode;
  count?: number;
}) {
  return (
    <header className="page-header">
      <div className="page-heading">
        <h2>
          {title}
          {count !== undefined && (
            <span className="heading-count">{count}</span>
          )}
        </h2>
        <p className="muted">{description}</p>
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </header>
  );
}
export function EmptyState({
  icon: Icon,
  title,
  children,
  action,
}: {
  icon: LucideIcon;
  title: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <Icon size={30} strokeWidth={1.25} aria-hidden="true" />
      <h3>{title}</h3>
      <p>{children}</p>
      {action}
    </div>
  );
}
export function Feedback({
  kind = "error",
  children,
}: {
  kind?: "error" | "success" | "info";
  children: ReactNode;
}) {
  return (
    <p
      className={`feedback ${kind}`}
      role={kind === "error" ? "alert" : "status"}
    >
      {children}
    </p>
  );
}
export function StatusBadge({
  state,
  children,
}: {
  state: "live" | "idle" | "warning";
  children: ReactNode;
}) {
  return (
    <span className={`status-badge ${state}`}>
      <span className="status-light" aria-hidden="true" />
      {children}
    </span>
  );
}

export function Overlay({
  title,
  children,
  onDismiss,
  sheet = false,
  wide = false,
  alert = false,
  dismissOutside = false,
}: {
  title: string;
  children: ReactNode;
  onDismiss: () => void;
  sheet?: boolean;
  wide?: boolean;
  alert?: boolean;
  dismissOutside?: boolean;
}) {
  const content = useRef<HTMLDivElement>(null);
  const dismiss = () => {
    if (!content.current?.querySelector('[aria-busy="true"]')) onDismiss();
  };
  const preventPendingEscape = (event: KeyboardEvent) => {
    if (content.current?.querySelector('[aria-busy="true"]'))
      event.preventDefault();
  };
  const returnFocus = useRef(document.activeElement as HTMLElement | null);
  const restoreFocus = (event: Event) => {
    event.preventDefault();
    if (returnFocus.current?.isConnected) returnFocus.current.focus();
  };
  if (alert)
    return (
      <AlertDialog.Root open onOpenChange={(open) => !open && dismiss()}>
        <AlertDialog.Portal>
          <AlertDialog.Overlay className="modal-backdrop" />
          <AlertDialog.Content
            ref={content}
            onEscapeKeyDown={preventPendingEscape}
            onOpenAutoFocus={(event) => {
              event.preventDefault();
              content.current
                ?.querySelector<HTMLElement>("[data-cancel], button")
                ?.focus();
            }}
            className={`modal ${wide ? "wide" : ""}`}
            aria-describedby={undefined}
            onCloseAutoFocus={restoreFocus}
          >
            <AlertDialog.Title>{title}</AlertDialog.Title>
            {children}
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    );
  return (
    <Dialog.Root open onOpenChange={(open) => !open && dismiss()}>
      <Dialog.Portal>
        <Dialog.Overlay className="modal-backdrop" />
        <Dialog.Content
          ref={content}
          onEscapeKeyDown={preventPendingEscape}
          className={sheet ? "drawer" : `modal ${wide ? "wide" : ""}`}
          aria-describedby={undefined}
          onCloseAutoFocus={restoreFocus}
          onPointerDownOutside={(event) => {
            if (!dismissOutside) event.preventDefault();
          }}
          onInteractOutside={(event) => {
            if (!dismissOutside) event.preventDefault();
          }}
        >
          <div className="overlay-header">
            <Dialog.Title>{title}</Dialog.Title>
            {sheet && (
              <IconButton
                label="Close host editor"
                icon={X}
                onClick={dismiss}
              />
            )}
          </div>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
