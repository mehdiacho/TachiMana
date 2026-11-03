// Tracking dialog (spec §2.22, port of ui/track/TrackDialog.kt): connect AniList, find the manga
// on AniList and link it, then edit status, chapters and score. Finished chapters push on their own.
import * as Dialog from "@radix-ui/react-dialog";
import { ExternalLink, Minus, Plus, Search, X } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/commands";
import { keys, queryClient, useAniList, useTrack } from "../../api/queries";
import { IconButton, OutlinedButton, PrimaryButton, SkeletonRows, Spinner, TextButton } from "../../components/ui";
import { useAction } from "../../hooks/useAction";
import type { CmdError } from "../../gen/CmdError";
import type { MediaDto } from "../../gen/MediaDto";
import type { TrackDto } from "../../gen/TrackDto";
import { accountLine, chaptersLine, mediaLine, SCORES, scoreLabel, STATUSES, stepChapters } from "../../lib/track";
import s from "./Library.module.css";

const msg = (e: unknown) => (e as CmdError).message ?? String(e);

function Connect({ onNote }: { onNote: (n: string | null) => void }) {
  const account = useAniList();
  const [pasted, setPasted] = useState("");
  const [busy, setBusy] = useState(false);
  const waiting = busy || account.data?.signingIn === true;
  const connect = async () => {
    setBusy(true);
    onNote(null);
    try {
      const a = await api.anilistConnect();
      queryClient.setQueryData(keys.anilist, a);
    } catch (e) {
      onNote(`Couldn't connect: ${msg(e)}`);
    } finally {
      setBusy(false);
    }
  };
  const [checking, setChecking] = useState(false);
  const submitToken = async () => {
    onNote(null);
    setChecking(true);
    try {
      const a = await api.anilistConnectToken(pasted);
      queryClient.setQueryData(keys.anilist, a);
      setPasted("");
    } catch (e) {
      onNote(`Token not accepted: ${msg(e)}`);
    } finally {
      setChecking(false);
    }
  };
  return (
    <div className={s.trackConnect}>
      <div className={s.trackNote}>
        Connect your AniList account to keep your reading progress there. Your browser opens; approve TachiMana and come back here.
      </div>
      <PrimaryButton onClick={() => void connect()} busy={waiting}>
        {waiting ? "Waiting for the browser…" : "Connect AniList"}
      </PrimaryButton>
      {waiting && account.data && (
        <div className={s.trackNote}>
          No browser opened? Go to <span className={s.trackUrl}>{account.data.authorizeUrl}</span>
        </div>
      )}
      <div className={s.trackNote}>If the browser sign-in doesn't work, paste a token instead:</div>
      <form
        className={s.addRow}
        onSubmit={(e) => {
          e.preventDefault();
          void submitToken();
        }}
      >
        <input className={s.field} value={pasted} onChange={(e) => setPasted(e.target.value)} placeholder="Access token" aria-label="AniList access token" autoComplete="off" spellCheck={false} />
        <OutlinedButton type="submit" disabled={pasted.trim() === ""} busy={checking}>
          {checking ? "Checking…" : "Use"}
        </OutlinedButton>
      </form>
    </div>
  );
}

function Find({ mangaId, title, onNote }: { mangaId: number; title: string; onNote: (n: string | null) => void }) {
  const [query, setQuery] = useState(title);
  const [results, setResults] = useState<MediaDto[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [binding, setBinding] = useState<number | null>(null);
  const search = async () => {
    setSearching(true);
    onNote(null);
    try {
      setResults(await api.trackSearch(query));
    } catch (e) {
      onNote(`Search failed: ${msg(e)}`);
    } finally {
      setSearching(false);
    }
  };
  const bind = async (m: MediaDto) => {
    setBinding(m.id);
    onNote(null);
    try {
      queryClient.setQueryData(keys.track(mangaId), await api.trackBind(mangaId, m));
    } catch (e) {
      onNote(`Couldn't link it: ${msg(e)}`);
    } finally {
      setBinding(null);
    }
  };
  return (
    <>
      <form
        className={s.addRow}
        onSubmit={(e) => {
          e.preventDefault();
          void search();
        }}
      >
        <input className={s.field} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search AniList…" aria-label="Search AniList" maxLength={200} />
        <PrimaryButton type="submit" disabled={query.trim() === ""} busy={searching}>
          {!searching && <Search size={16} />} {searching ? "Searching…" : "Search"}
        </PrimaryButton>
      </form>
      {searching && results === null && <SkeletonRows count={4} cover={false} />}
      {results !== null && results.length === 0 && <div className={s.trackNote}>No matches on AniList. Try another title.</div>}
      {results !== null && results.length > 0 && (
        <div className={s.trackResults} role="list">
          {results.map((m) => (
            <button key={m.id} type="button" role="listitem" className={s.trackResult} disabled={binding !== null} onClick={() => void bind(m)}>
              <span className={s.trackResultTitle}>{m.title}</span>
              <span className={s.trackNote}>
                {binding === m.id ? (
                  <>
                    <Spinner size={12} /> Linking…
                  </>
                ) : (
                  mediaLine(m)
                )}
              </span>
            </button>
          ))}
        </div>
      )}
    </>
  );
}

function Linked({ mangaId, track, onNote }: { mangaId: number; track: TrackDto; onNote: (n: string | null) => void }) {
  const [busy, setBusy] = useState(false);
  const run = async (f: () => Promise<TrackDto>, what: string) => {
    setBusy(true);
    onNote(null);
    try {
      queryClient.setQueryData(keys.track(mangaId), await f());
    } catch (e) {
      onNote(`Couldn't update ${what}: ${msg(e)}`);
    } finally {
      setBusy(false);
    }
  };
  const step = (d: number) => {
    const next = stepChapters(track.progress, d, track.total);
    if (next !== track.progress) void run(() => api.trackSetProgress(mangaId, next), "chapters");
  };
  return (
    <div className={s.trackCard}>
      <div className={s.trackStrip}>
        <span className={s.trackChip} aria-hidden>
          AL
        </span>
        <div className={s.trackStripText}>
          <span className={s.trackResultTitle}>{track.title}</span>
          <span className={s.trackLinked}>
            {busy ? (
              <>
                <Spinner size={11} /> Saving to AniList…
              </>
            ) : (
              "Linked · updates on read"
            )}
          </span>
        </div>
        <IconButton icon={ExternalLink} label="Open on AniList" size={18} onClick={() => void api.trackOpen(mangaId).catch((e) => onNote(msg(e)))} />
      </div>
      <div className={s.trackGrid}>
        <label className={s.trackCell}>
          <span className={s.trackLabel}>Status</span>
          <select className={s.trackSelect} value={track.status} disabled={busy} onChange={(e) => void run(() => api.trackSetStatus(mangaId, Number(e.target.value)), "the status")}>
            {STATUSES.map((st) => (
              <option key={st.value} value={st.value}>
                {st.label}
              </option>
            ))}
          </select>
        </label>
        <div className={s.trackCell}>
          <span className={s.trackLabel}>Chapters</span>
          <div className={s.trackStepper}>
            <IconButton icon={Minus} label="One chapter less" size={16} disabled={busy || track.progress <= 0} onClick={() => step(-1)} />
            <span className={s.trackValue}>{chaptersLine(track.progress, track.total)}</span>
            <IconButton icon={Plus} label="One chapter more" size={16} disabled={busy || (track.total > 0 && track.progress >= track.total)} onClick={() => step(1)} />
          </div>
        </div>
        <label className={s.trackCell}>
          <span className={s.trackLabel}>Score</span>
          <select className={s.trackSelect} value={Math.round(track.score)} disabled={busy} onChange={(e) => void run(() => api.trackSetScore(mangaId, Number(e.target.value)), "the score")}>
            {SCORES.map((n) => (
              <option key={n} value={n}>
                {scoreLabel(n)}
              </option>
            ))}
          </select>
        </label>
      </div>
    </div>
  );
}

export function TrackDialog({ mangaId, title, open, onOpenChange }: { mangaId: number; title: string; open: boolean; onOpenChange: (o: boolean) => void }) {
  const account = useAniList();
  const track = useTrack(mangaId);
  const [note, setNote] = useState<string | null>(null);
  const connected = account.data?.connected === true;
  const t = track.data ?? null;
  const unbind = useAction(async () => {
    setNote(null);
    await api.trackUnbind(mangaId);
    queryClient.setQueryData(keys.track(mangaId), null);
  });
  const signOut = useAction(async () => {
    setNote(null);
    queryClient.setQueryData(keys.anilist, await api.anilistDisconnect());
  });
  const shownNote = note ?? unbind.error ?? signOut.error;
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className={s.backdrop} />
        <Dialog.Content className={s.dialog} style={{ width: 390 }} aria-describedby={undefined}>
          <div className={s.dialogHead}>
            <Dialog.Title className={s.trackTitle}>Tracking · {title}</Dialog.Title>
            <Dialog.Close asChild>
              <IconButton icon={X} label="Close" />
            </Dialog.Close>
          </div>
          <div className={s.trackAccount}>
            <span className={s.trackNote}>{accountLine(account.data?.userName ?? null, account.data?.expiresAt ?? null)}</span>
            {connected && (
              <TextButton onClick={() => void signOut.run()} busy={signOut.pending}>
                {signOut.pending ? "Signing out…" : "Sign out"}
              </TextButton>
            )}
          </div>
          {account.isLoading || (connected && track.isLoading) ? (
            <SkeletonRows count={3} cover={false} />
          ) : !connected ? (
            <Connect onNote={setNote} />
          ) : t ? (
            <>
              <Linked mangaId={mangaId} track={t} onNote={setNote} />
              <div className={s.addRow}>
                <span className={s.grow} />
                <TextButton onClick={() => void unbind.run()} busy={unbind.pending}>
                  {unbind.pending ? "Stopping…" : "Stop tracking"}
                </TextButton>
              </div>
            </>
          ) : (
            <Find mangaId={mangaId} title={title} onNote={setNote} />
          )}
          {t && !connected && (
            <div className={s.trackNote}>
              Linked to {t.title} ({chaptersLine(t.progress, t.total)}). Connect to update it.
            </div>
          )}
          {shownNote && (
            <div className={s.error} role="alert">
              {shownNote}
            </div>
          )}
          <div className={s.trackFoot}>Finished chapters update AniList while you read. Progress there never goes down.</div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
