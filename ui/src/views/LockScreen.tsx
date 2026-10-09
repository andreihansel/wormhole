import { useEffect, useRef, useState, type FormEvent } from "react";
import { ArrowRight, ShieldCheck } from "lucide-react";
import { api, errorMessage } from "../api";
import { BrandMark, Button, Feedback, PasswordField } from "../components/ui";
import VaultBackdrop from "../components/VaultBackdrop";

const QUOTES = [
  "Beam me up, Scotty!",
  "Live long and prosper.",
  "To boldly go where no man has gone before.",
  "In space no one can hear you scream.",
  "Traveling through hyperspace ain't like dusting crops, boy!",
];
const QUOTE_CURSOR = "wormhole.lockQuote.next";
let fallbackQuote = 0;
function nextQuoteIndex() {
  try {
    const index = Number(localStorage.getItem(QUOTE_CURSOR));
    return Number.isInteger(index) && index >= 0 && index < QUOTES.length
      ? index
      : 0;
  } catch {
    return fallbackQuote;
  }
}
interface Props {
  initialized: boolean;
  onUnlocked: () => void;
}
export default function LockScreen({ initialized, onUnlocked }: Props) {
  const [quoteIndex] = useState(nextQuoteIndex);
  const quoteClaimed = useRef(false);
  useEffect(() => {
    // Count committed openings, not renders or StrictMode's effect replay.
    if (quoteClaimed.current) return;
    quoteClaimed.current = true;
    fallbackQuote = (quoteIndex + 1) % QUOTES.length;
    try {
      localStorage.setItem(QUOTE_CURSOR, String(fallbackQuote));
    } catch {
      // A restricted storage environment still cycles during this app session.
    }
  }, [quoteIndex]);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (pending.current) return;
    setError(null);
    if (!initialized && password !== confirm) {
      setError("Passwords do not match");
      return;
    }
    pending.current = true;
    setBusy(true);
    try {
      await (initialized
        ? api.vaultUnlock(password)
        : api.vaultCreate(password));
      setPassword("");
      setConfirm("");
      onUnlocked();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return (
    <div className="lock">
      <div className="lock-brand">
        <BrandMark />
        <span>Wormhole</span>
      </div>
      <VaultBackdrop
        still={Boolean(
          import.meta.env.VITE_E2E &&
          new URLSearchParams(location.search).has("still"),
        )}
      />
      <div className="lock-form-region">
        <form
          className="lock-card vault-glass"
          onSubmit={submit}
          aria-label={initialized ? "Unlock vault" : "Create vault"}
          aria-busy={busy}
        >
          <h1>
            {initialized ? "Welcome back." : "Your universe,\nwithin reach."}
          </h1>
          <p className="muted">
            {initialized
              ? "Unlock your vault. Your connections are right where you left them."
              : "A private home for your servers, keys, and connections. Start with a master password."}
          </p>
          <div className="lock-fields">
            <PasswordField
              label="Master password"
              autoFocus
              autoComplete={initialized ? "current-password" : "new-password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              disabled={busy}
            />
            {!initialized && (
              <PasswordField
                label="Confirm password"
                autoComplete="new-password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                disabled={busy}
              />
            )}
          </div>
          {error && <Feedback>{error}</Feedback>}
          <Button
            type="submit"
            className="primary unlock-button"
            icon={ArrowRight}
            busy={busy}
            disabled={!password}
          >
            {busy
              ? initialized
                ? "Unlocking…"
                : "Creating vault…"
              : initialized
                ? "Unlock"
                : "Create vault"}
          </Button>
          {!initialized && (
            <p className="recovery-note">
              Your master password encrypts everything in your vault and cannot
              be recovered. Keep it somewhere safe.
            </p>
          )}
          <div className="lock-assurance">
            <ShieldCheck size={15} aria-hidden="true" />
            Encrypted on this device. Always yours.
          </div>
        </form>
      </div>
      <footer className="lock-footer">
        <span className="lock-quote">“{QUOTES[quoteIndex]}”</span>
        <span>
          Wormhole <span className="mono">0.1.0</span>
        </span>
      </footer>
    </div>
  );
}
