// What a restore can still do once the library is back: the extension repos it added, the
// extensions to install from them, chapters to download again, and an AniList sign-in.
import { listen } from "@tauri-apps/api/event";
import { useState } from "react";
import { api } from "../../api/commands";
import { keys, queryClient, useAniList } from "../../api/queries";
import { useAction } from "../../hooks/useAction";
import { OutlinedButton, PrimaryButton, TextButton } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { RestoreExtensionsDto } from "../../gen/RestoreExtensionsDto";
import type { RestoreOfferDto } from "../../gen/RestoreOfferDto";
import { extensionsLine, redownloadLine } from "../../lib/sync";
import l from "../Library/Library.module.css";
import st from "../Settings/Settings.module.css";
import s from "./Sync.module.css";

/** The NAS password the phone sealed with a passphrase: type the passphrase to unlock it. */
function NasPassword() {
  const [phrase, setPhrase] = useState("");
  const unlock = useAction(
    async () => {
      await api.restoreNasPassword(phrase);
      setPhrase("");
      await queryClient.invalidateQueries({ queryKey: keys.nas });
      await queryClient.invalidateQueries({ queryKey: keys.sync });
    },
    { ok: () => "The NAS password is set. Your NAS is ready to use." },
  );
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        void unlock.run();
      }}
    >
      <div className={st.rowDesc}>The backup has your NAS password, locked with the passphrase you typed on the phone. Type it here to unlock the password. The passphrase isn't saved.</div>
      <div className={s.addRow}>
        <input className={l.field} type="password" value={phrase} onChange={(e) => (setPhrase(e.target.value), unlock.setError(null))} placeholder="Passphrase" aria-label="Passphrase for the NAS password" autoComplete="off" />
        <OutlinedButton type="submit" busy={unlock.pending} disabled={phrase.length === 0}>
          {unlock.pending ? "Unlocking…" : "Unlock"}
        </OutlinedButton>
      </div>
      {unlock.error && (
        <div className={l.error} role="alert">
          {unlock.error}
        </div>
      )}
    </form>
  );
}

export function RestoreOffer({ offer }: { offer: RestoreOfferDto }) {
  const anilist = useAniList();
  const [plan, setPlan] = useState<RestoreExtensionsDto | null>(null);
  const [busy, setBusy] = useState<"look" | "install" | "download" | null>(null);
  const [which, setWhich] = useState<"all" | "unread">("all");
  const dismiss = useAction(api.restoreDismiss, { toastError: true });
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);
  const [helperNote, setHelperNote] = useState<string | null>(null);

  const signIn = offer.anilistSignIn && anilist.data?.connected !== true;
  const nothing = offer.reposAdded.length === 0 && !offer.hasExtensions && offer.downloaded === 0 && !signIn && !offer.nasPasswordSealed;
  if (nothing) return null;

  const look = async () => {
    setBusy("look");
    setNote(null);
    try {
      // Online sources need the extension helper; fetch it first when this install has none.
      const helper = await api.helperStatus();
      if (helper.source === "missing") {
        if (!helper.canDownload) throw { message: "This computer has no extension helper and none can be downloaded for it. Install one from Settings, Extension helper." };
        setHelperNote("Downloading the extension helper…");
        const un = await listen<{ done: number; total: number }>("helper-progress", (e) => {
          const { done, total } = e.payload;
          setHelperNote(total > 0 ? `Downloading the extension helper… ${Math.round((done / total) * 100)}% of ${Math.round(total / 1048576)} MB` : "Downloading the extension helper…");
        });
        try {
          await api.helperDownload();
        } finally {
          un();
          setHelperNote(null);
        }
        await queryClient.invalidateQueries({ queryKey: keys.helper });
        await queryClient.invalidateQueries({ queryKey: keys.browseStatus });
      }
      setPlan(await api.restoreExtensions());
    } catch (e) {
      setNote({ ok: false, text: `Couldn't look up the extensions: ${(e as CmdError).message}` });
    } finally {
      setBusy(null);
    }
  };

  const exts = plan?.extensions ?? [];
  const toInstall = exts.filter((e) => !e.installed && e.repoUrl !== null);
  const missing = exts.filter((e) => !e.installed && e.repoUrl === null);

  const installAll = async () => {
    setBusy("install");
    setNote(null);
    const failed: string[] = [];
    let done = 0;
    for (const e of toInstall) {
      try {
        await api.extInstall(e.repoUrl ?? "", e.pkgName);
        done += 1;
      } catch {
        failed.push(e.name);
      }
    }
    await queryClient.invalidateQueries({ queryKey: ["browse"] });
    setPlan(await api.restoreExtensions().catch(() => plan));
    setNote(
      failed.length === 0
        ? { ok: true, text: `Installed ${done === 1 ? "1 extension" : `${done} extensions`}.` }
        : { ok: false, text: `Installed ${done}. These didn't install: ${failed.join(", ")}.` },
    );
    setBusy(null);
  };

  const redownload = async (unreadOnly: boolean) => {
    setBusy("download");
    setNote(null);
    try {
      const n = await api.restoreRedownload(unreadOnly);
      setNote({ ok: true, text: `Queued ${n === 1 ? "1 chapter" : `${n} chapters`} for download.` });
      await queryClient.invalidateQueries({ queryKey: keys.sync });
    } catch (e) {
      setNote({ ok: false, text: (e as CmdError).message });
    } finally {
      setBusy(null);
    }
  };

  const signInAniList = async () => {
    setNote(null);
    try {
      await api.anilistConnect();
    } catch (e) {
      setNote({ ok: false, text: (e as CmdError).message });
    } finally {
      await queryClient.invalidateQueries({ queryKey: keys.anilist });
    }
  };

  return (
    <div className={s.card} aria-label="After the restore">
      <div className={st.rowTitle}>Finish setting up</div>

      {offer.reposAdded.length > 0 && (
        <>
          <div className={st.rowDesc}>
            {offer.reposAdded.length === 1
              ? "Added this extension repo from the backup. You can remove it under Browse, Extensions."
              : `Added these ${offer.reposAdded.length} extension repos from the backup. You can remove any of them under Browse, Extensions.`}
          </div>
          <div role="list">
            {offer.reposAdded.map((p) => (
              <div key={p.indexUrl} className={s.repo} role="listitem">
                <span className={st.rowTitle}>{p.name || p.indexUrl}</span>
                <span className={st.rowDesc}>{p.indexUrl}</span>
              </div>
            ))}
          </div>
        </>
      )}

      {offer.hasExtensions && plan === null && (
        <div className={st.buttons}>
          <PrimaryButton onClick={() => void look()} disabled={busy !== null} busy={busy === "look"}>
            {busy === "look" ? "Looking…" : "Find the extensions it used"}
          </PrimaryButton>
        </div>
      )}
      {helperNote && (
        <div className={st.rowDesc} role="status">
          {helperNote}
        </div>
      )}
      {busy === "look" && !helperNote && (
        <div className={st.rowDesc} role="status">
          Reading the extension repositories…
        </div>
      )}
      {plan !== null && (
        <>
          <div className={st.rowDesc}>{extensionsLine(toInstall, missing, plan.failures.length > 0)}</div>
          {plan.failures.length > 0 && (
            <div className={l.error} role="alert">
              Couldn't read {plan.failures.length === 1 ? "this repo" : "these repos"}, so some extensions could not be looked up. Try again when it can be reached: {plan.failures.join("; ")}
            </div>
          )}
          {toInstall.length > 0 && (
            <div className={st.buttons}>
              <PrimaryButton onClick={() => void installAll()} disabled={busy !== null} busy={busy === "install"}>
                {busy === "install" ? "Installing…" : "Install all"}
              </PrimaryButton>
            </div>
          )}
        </>
      )}

      {offer.downloaded > 0 && (
        <>
          <div className={st.rowDesc}>{redownloadLine(offer.downloaded, offer.downloadedUnread)}</div>
          <div className={st.buttons}>
            <PrimaryButton
              onClick={() => {
                setWhich("all");
                void redownload(false);
              }}
              disabled={busy !== null}
              busy={busy === "download" && which === "all"}
            >
              Download all
            </PrimaryButton>
            {offer.downloadedUnread > 0 && offer.downloadedUnread < offer.downloaded && (
              <OutlinedButton
                onClick={() => {
                  setWhich("unread");
                  void redownload(true);
                }}
                disabled={busy !== null}
                busy={busy === "download" && which === "unread"}
              >
                Unread only
              </OutlinedButton>
            )}
          </div>
        </>
      )}

      {offer.nasPasswordSealed && <NasPassword />}

      {signIn && (
        <>
          <div className={st.rowDesc}>Your AniList tracking came back. Sign in to keep it in sync from this computer. Next time, tick “Include sensitive settings” when you create the backup in Mihon, and TachiMana signs you in for you.</div>
          <div className={st.buttons}>
            <OutlinedButton onClick={() => void signInAniList()} disabled={busy !== null} busy={anilist.data?.signingIn === true}>
              {anilist.data?.signingIn === true ? "Waiting for AniList…" : "Sign in to AniList"}
            </OutlinedButton>
          </div>
        </>
      )}

      {note && (
        <div className={note.ok ? st.ok : l.error} role={note.ok ? "status" : "alert"}>
          {note.text}
        </div>
      )}
      <div className={st.buttons}>
        <TextButton onClick={() => void dismiss.run()} busy={dismiss.pending}>
          Done
        </TextButton>
      </div>
    </div>
  );
}
