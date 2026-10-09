import { useRef, useState } from "react";
import { AlertTriangle } from "lucide-react";
import { errorMessage } from "../api";
import { Button, Feedback, Overlay } from "./ui";
interface Props {
  title: string;
  body?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}
export default function Confirm({
  title,
  body,
  confirmLabel = "Delete",
  cancelLabel = "Cancel",
  onConfirm,
  onCancel,
}: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(false);
  async function confirm() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return (
    <Overlay
      title={title}
      alert
      onDismiss={() => {
        if (!busy) onCancel();
      }}
    >
      <AlertTriangle className="warning-icon" size={25} aria-hidden="true" />
      {body && <p className="muted">{body}</p>}
      {error && <Feedback>{error}</Feedback>}
      <div className="row end">
        <Button disabled={busy} onClick={onCancel}>
          {cancelLabel}
        </Button>
        <Button className="danger" busy={busy} onClick={confirm}>
          {busy ? "Working…" : confirmLabel}
        </Button>
      </div>
    </Overlay>
  );
}
