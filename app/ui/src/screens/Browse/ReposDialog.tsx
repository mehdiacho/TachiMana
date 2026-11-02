// Extension repositories (spec §2.23 "Repos"): add by URL, remove. Nothing is added for the user;
// the Keiyoushi repo is offered as a one-tap suggestion only.
import * as Dialog from "@radix-ui/react-dialog";
import { Plus, Trash2, X } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/commands";
import { useBrowseInvalidate, useRepos } from "../../api/queries";
import { Chip, IconButton, PrimaryButton } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import { KEIYOUSHI_REPO } from "../../lib/browse";
import l from "../Library/Library.module.css";
import s from "./Browse.module.css";

export function ReposDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const repos = useRepos();
  const invalidate = useBrowseInvalidate();
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const add = async (u: string) => {
    const trimmed = u.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      await api.extRepoAdd(trimmed);
      setUrl("");
    } catch (e) {
      setError((e as CmdError).message);
    } finally {
      setBusy(false);
      invalidate();
    }
  };
  const remove = async (baseUrl: string) => {
    setError(null);
    try {
      await api.extRepoRemove(baseUrl);
    } catch (e) {
      setError((e as CmdError).message);
    } finally {
      invalidate();
    }
  };

  const list = repos.data ?? [];
  const hasKeiyoushi = list.some((r) => r.baseUrl.startsWith(KEIYOUSHI_REPO));
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className={l.backdrop} />
        <Dialog.Content className={l.dialog} style={{ width: 520 }} aria-describedby={undefined}>
          <div className={l.dialogHead}>
            <Dialog.Title style={{ font: "inherit", margin: 0 }}>Extension repositories</Dialog.Title>
            <Dialog.Close asChild>
              <IconButton icon={X} label="Close" />
            </Dialog.Close>
          </div>
          <div className={s.hint}>
            A repository lists extensions you can install. Only add repositories you trust: extensions run code on this computer.
          </div>
          <form
            className={l.addRow}
            onSubmit={(e) => {
              e.preventDefault();
              void add(url);
            }}
          >
            <input
              className={l.field}
              value={url}
              placeholder="https://…/index.min.json"
              aria-label="Repository URL"
              onChange={(e) => setUrl(e.target.value)}
              maxLength={2000}
              disabled={busy}
            />
            <PrimaryButton type="submit" disabled={busy || url.trim() === ""}>
              {busy ? "Adding…" : "Add"}
            </PrimaryButton>
          </form>
          {!hasKeiyoushi && (
            <div>
              <Chip onClick={() => void add(KEIYOUSHI_REPO)} disabled={busy}>
                <Plus size={15} /> Keiyoushi
              </Chip>
            </div>
          )}
          {error && <div className={l.error}>{error}</div>}
          <div className={l.catList}>
            {list.length === 0 && <div className={l.meta}>No repositories yet.</div>}
            {list.map((r) => (
              <div key={r.baseUrl} className={s.repo}>
                <div className={s.itemText}>
                  <span className={s.itemTitle}>{r.name}</span>
                  <span className={s.repoUrl}>{r.baseUrl}</span>
                </div>
                <IconButton icon={Trash2} label={`Remove ${r.name}`} size={18} onClick={() => void remove(r.baseUrl)} />
              </div>
            ))}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
