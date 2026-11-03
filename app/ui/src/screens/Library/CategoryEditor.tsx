// Category editor (port of ui/library/CategoryDialog.kt): add, rename, reorder, delete.
import * as Dialog from "@radix-ui/react-dialog";
import { ArrowDown, ArrowUp, Check, Pencil, Trash2, X } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/commands";
import { useCategories, useLibraryInvalidate } from "../../api/queries";
import { IconButton, PrimaryButton, SkeletonRows } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import s from "./Library.module.css";

export function CategoryEditor({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const cats = useCategories();
  const invalidate = useLibraryInvalidate();
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<{ id: number; name: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  /** What is being written, as `<op>:<id>`, so that button shows a spinner and the rest wait. */
  const [working, setWorking] = useState<string | null>(null);
  const run = async (key: string, f: () => Promise<unknown>) => {
    if (working) return;
    setError(null);
    setWorking(key);
    try {
      await f();
    } catch (e) {
      setError((e as CmdError).message);
    } finally {
      setWorking(null);
    }
    invalidate();
  };
  const busy = (key: string) => working === key;

  const list = cats.data ?? [];
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className={s.backdrop} />
        <Dialog.Content className={s.dialog} aria-describedby={undefined}>
          <div className={s.dialogHead}>
            <Dialog.Title style={{ font: "inherit", margin: 0 }}>Edit categories</Dialog.Title>
            <Dialog.Close asChild>
              <IconButton icon={X} label="Close" />
            </Dialog.Close>
          </div>
          <form
            className={s.addRow}
            onSubmit={(e) => {
              e.preventDefault();
              if (name.trim() === "") return;
              void run("add", () => api.categoryCreate(name)).then(() => setName(""));
            }}
          >
            <input className={s.field} value={name} placeholder="New category" onChange={(e) => setName(e.target.value)} maxLength={200} />
            <PrimaryButton type="submit" disabled={name.trim() === "" || (working !== null && !busy("add"))} busy={busy("add")}>
              {busy("add") ? "Adding…" : "Add"}
            </PrimaryButton>
          </form>
          {error && <div className={s.error}>{error}</div>}
          <div className={s.catList}>
            {cats.isLoading && <SkeletonRows count={3} cover={false} />}
            {!cats.isLoading && list.length === 0 && <div className={s.meta}>No categories yet. Add one above.</div>}
            {list.map((c, i) => (
              <div key={c.id} className={s.catRow}>
                {editing?.id === c.id ? (
                  <form
                    className={s.addRow}
                    style={{ flex: 1 }}
                    onSubmit={(e) => {
                      e.preventDefault();
                      const n = editing.name;
                      void run(`rename:${c.id}`, () => api.categoryRename(c.id, n)).then(() => setEditing(null));
                    }}
                  >
                    <input
                      className={s.field}
                      autoFocus
                      value={editing.name}
                      maxLength={200}
                      onChange={(e) => setEditing({ id: c.id, name: e.target.value })}
                      onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), setEditing(null))}
                    />
                    <IconButton icon={Check} label="Save name" type="submit" busy={busy(`rename:${c.id}`)} />
                  </form>
                ) : (
                  <>
                    <span className={s.catName}>{c.name}</span>
                    <IconButton icon={ArrowUp} label="Move up" size={18} disabled={i === 0 || working !== null} busy={busy(`up:${c.id}`)} onClick={() => void run(`up:${c.id}`, () => api.categoryMove(c.id, true))} />
                    <IconButton icon={ArrowDown} label="Move down" size={18} disabled={i === list.length - 1 || working !== null} busy={busy(`down:${c.id}`)} onClick={() => void run(`down:${c.id}`, () => api.categoryMove(c.id, false))} />
                    <IconButton icon={Pencil} label="Rename" size={18} onClick={() => setEditing({ id: c.id, name: c.name })} />
                    <IconButton icon={Trash2} label={`Delete ${c.name}`} size={18} disabled={working !== null} busy={busy(`del:${c.id}`)} onClick={() => void run(`del:${c.id}`, () => api.categoryDelete(c.id))} />
                  </>
                )}
              </div>
            ))}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
