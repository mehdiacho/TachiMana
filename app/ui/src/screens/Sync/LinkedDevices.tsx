// Linked devices (docs/rust-port/profiles-and-remote-downloads.md §2): this profile's link code as
// a QR code and as text, linking to another device, and the list with Unlink. A linked device
// talks to this one without the six-digit code.
import { Laptop, Link2, Smartphone, Unlink } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/commands";
import { keys, queryClient, useLinks } from "../../api/queries";
import { OutlinedButton, Skeleton, TextButton } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { LinkDto } from "../../gen/LinkDto";
import type { LinksDto } from "../../gen/LinksDto";
import { useAction } from "../../hooks/useAction";
import { cleanCode, spacedCode } from "../../lib/sync";
import l from "../Library/Library.module.css";
import st from "../Settings/Settings.module.css";
import s from "./Sync.module.css";

function since(ms: number): string {
  const d = new Date(ms);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

function LinkRow({ link }: { link: LinkDto }) {
  const [confirm, setConfirm] = useState(false);
  const unlink = useAction(
    async () => {
      const links = await api.unlinkDevice(link.peerDeviceId);
      queryClient.setQueryData<LinksDto>(keys.links, (old) => (old ? { ...old, links } : old));
      return links;
    },
    { ok: () => `Unlinked ${link.peerName}.`, toastError: true },
  );
  const Icon = link.platform === "android" ? Smartphone : Laptop;
  return (
    <div className={st.row} role="listitem">
      <Icon size={20} className={st.rowIcon} />
      <div className={st.rowText}>
        <span className={st.rowTitle}>{link.peerName}</span>
        <span className={st.rowDesc}>
          {confirm ? "It will need the pairing code again to link." : `Linked ${since(Number(link.linkedAt))}${link.lastAddress ? ` · last seen at ${link.lastAddress}` : ""}`}
        </span>
      </div>
      <div className={st.buttons}>
        {confirm ? (
          <>
            <OutlinedButton busy={unlink.pending} onClick={() => void unlink.run()}>
              {unlink.pending ? "Unlinking…" : "Unlink"}
            </OutlinedButton>
            <TextButton onClick={() => setConfirm(false)}>Cancel</TextButton>
          </>
        ) : (
          <TextButton onClick={() => setConfirm(true)} aria-label={`Unlink ${link.peerName}`}>
            <Unlink size={16} /> Unlink
          </TextButton>
        )}
      </div>
    </div>
  );
}

/** How the other device's library comes into this PC. */
export type Keep = "merge" | "new";

/** What each choice does, in a sentence. */
function keepText(keep: Keep, empty: boolean, other: string): string {
  const text = keepSentence(keep, empty, other);
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function keepSentence(keep: Keep, empty: boolean, other: string): string {
  if (keep === "merge") {
    return empty
      ? `This profile is empty, so it becomes ${other}'s profile and copies its library. TachiMana restarts to do it.`
      : `${other}'s library is merged into this profile: what's missing here is added and what both have is brought up to date. Nothing here is deleted, and your NAS and extensions stay set up. TachiMana restarts to do it.`;
  }
  return `This profile stays as it is, and ${other}'s library goes into a profile of its own. TachiMana restarts into it.`;
}

/** The choice when the two devices are on different profiles. */
function KeepChoice({ message, other, empty, busy, onKeep, onCancel }: { message: string; other: string; empty: boolean; busy: Keep | null; onKeep: (k: Keep) => void; onCancel: () => void }) {
  return (
    <div className={s.choice} role="alertdialog" aria-label={`Choose how to bring in ${other}'s library`}>
      <div className={st.rowDesc}>{message}</div>
      <div className={s.option}>
        <div className={st.rowTitle}>{empty ? `Use ${other}'s library here` : "Merge into this profile"}</div>
        <div className={st.rowDesc}>{keepText("merge", empty, other)}</div>
        <OutlinedButton busy={busy === "merge"} disabled={busy !== null} onClick={() => onKeep("merge")}>
          {busy === "merge" ? "Linking…" : empty ? "Use it here" : "Merge"}
        </OutlinedButton>
      </div>
      {!empty && (
        <div className={s.option}>
          <div className={st.rowTitle}>Put it in a new profile</div>
          <div className={st.rowDesc}>{keepText("new", empty, other)}</div>
          <OutlinedButton busy={busy === "new"} disabled={busy !== null} onClick={() => onKeep("new")}>
            {busy === "new" ? "Linking…" : "New profile"}
          </OutlinedButton>
        </div>
      )}
      <div className={s.option}>
        <div className={st.rowTitle}>Keep this PC's library on {other}</div>
        <div className={st.rowDesc}>That replaces {other}'s library, so choose it there: scan this PC's code from {other} and pick "Keep the computer's". Its library is backed up on {other} first.</div>
      </div>
      <TextButton onClick={onCancel}>Cancel</TextButton>
    </div>
  );
}

function LinkForm({ empty }: { empty: boolean }) {
  const [address, setAddress] = useState("");
  const [code, setCode] = useState("");
  const [other, setOther] = useState<string | null>(null);
  const [keeping, setKeeping] = useState<Keep | null>(null);
  const pasted = address.trim().startsWith("tachimana://");
  const link = useAction(
    async (keep?: Keep) => {
      setKeeping(keep ?? null);
      try {
        const out = await api.linkDevice(address, code, keep);
        await queryClient.invalidateQueries({ queryKey: keys.links });
        setOther(null);
        return out;
      } catch (e) {
        if ((e as CmdError).kind === "otherProfile") setOther((e as CmdError).message);
        throw e;
      } finally {
        setKeeping(null);
      }
    },
    { ok: (m) => (m.restarting ? `Linked to ${m.link.peerName}. Restarting into its profile…` : `Linked to ${m.link.peerName}.`) },
  );
  const submit = async () => {
    const r = await link.run();
    if (r !== undefined) {
      setAddress("");
      setCode("");
    }
  };
  return (
    <form
      className={s.card}
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <div className={st.rowDesc}>Enter the other device's address and the pairing code it shows, or paste its link code.</div>
      <div className={s.addRow}>
        <input className={l.field} value={address} onChange={(e) => (setAddress(e.target.value), link.setError(null), setOther(null))} placeholder="192.168.x.x:27193 or tachimana://link?…" aria-label="Address or link code" spellCheck={false} maxLength={4096} />
        {!pasted && (
          <input className={`${l.field} ${s.codeInput}`} value={spacedCode(code)} onChange={(e) => (setCode(cleanCode(e.target.value)), setOther(null))} placeholder="6-digit code" inputMode="numeric" aria-label="Their pairing code" autoComplete="off" />
        )}
        <OutlinedButton type="submit" busy={link.pending && other === null} disabled={!address.trim() || (!pasted && code.length !== 6) || other !== null}>
          {!link.pending && <Link2 size={16} />} {link.pending ? "Linking…" : "Link"}
        </OutlinedButton>
      </div>
      {other !== null ? (
        <KeepChoice message={other} other="the other device" empty={empty} busy={link.pending ? keeping : null} onKeep={(k) => void link.run(k)} onCancel={() => (setOther(null), link.setError(null))} />
      ) : (
        link.error && (
          <div className={l.error} role="alert">
            {link.error}
          </div>
        )
      )}
    </form>
  );
}

/** A device that scanned this PC's code from another profile and was turned away. */
function RefusedOffer({ refused, empty }: { refused: NonNullable<LinksDto["refused"]>; empty: boolean }) {
  const [code, setCode] = useState("");
  const [choosing, setChoosing] = useState<Keep | null>(null);
  const take = useAction(async (keep: Keep) => api.linkRefused(code, keep), {
    ok: (m) => `Linked to ${m.link.peerName}. Restarting into its profile…`,
  });
  const dismiss = useAction(async () => {
    await api.dismissRefused();
    await queryClient.invalidateQueries({ queryKey: keys.links });
  });
  const who = refused.peerName;
  return (
    <div className={s.card} role="alert">
      <div className={s.deviceName}>
        {who} tried to link from its own profile, “{refused.profileName}”
      </div>
      {choosing === null ? (
        <>
          <div className={st.rowDesc}>Choose how to bring its library into this PC.</div>
          <div className={st.buttons}>
            <OutlinedButton onClick={() => setChoosing("merge")}>{empty ? "Use it here" : "Merge into this profile"}</OutlinedButton>
            {!empty && <OutlinedButton onClick={() => setChoosing("new")}>Put it in a new profile</OutlinedButton>}
            <TextButton busy={dismiss.pending} onClick={() => void dismiss.run()}>
              Not now
            </TextButton>
          </div>
          <div className={st.rowDesc}>To keep this PC's library on {who} instead, scan again on {who} and pick "Keep the computer's".</div>
        </>
      ) : (
        <>
          <div className={st.rowDesc}>{keepText(choosing, empty, who)} Enter the pairing code shown on {who} (Settings, then LAN sync).</div>
          <div className={s.addRow}>
            <input className={`${l.field} ${s.codeInput}`} value={spacedCode(code)} onChange={(e) => setCode(cleanCode(e.target.value))} placeholder="6-digit code" inputMode="numeric" aria-label={`${who}'s pairing code`} autoComplete="off" autoFocus />
            <OutlinedButton busy={take.pending} disabled={code.length !== 6} onClick={() => void take.run(choosing)}>
              {take.pending ? "Linking…" : choosing === "merge" ? "Link and merge" : "Link and copy its library"}
            </OutlinedButton>
            <TextButton onClick={() => setChoosing(null)}>Back</TextButton>
          </div>
          {take.error && (
            <div className={l.error} role="alert">
              {take.error}
            </div>
          )}
        </>
      )}
    </div>
  );
}

export function LinkedDevices() {
  const q = useLinks();
  const [showText, setShowText] = useState(false);
  if (q.isPending) {
    return (
      <div className={s.card}>
        <Skeleton width="220px" height={220} radius="8px" />
      </div>
    );
  }
  if (q.isError || !q.data) return <div className={l.error}>Couldn't read the linked devices.</div>;
  const d = q.data;
  return (
    <>
      <div className={s.card}>
        <div className={s.linkCode}>
          {d.qrSvg ? (
            <img className={s.qr} src={`data:image/svg+xml;utf8,${encodeURIComponent(d.qrSvg)}`} alt="Link code as a QR code" width={180} height={180} />
          ) : (
            <Skeleton width="180px" height={180} radius="8px" />
          )}
          <div className={s.linkText}>
            <div className={s.deviceName}>Link a phone to {d.profileName ? `“${d.profileName}”` : "this profile"}</div>
            <div className={st.rowDesc}>
              Scan this code with your phone's camera, or in the phone app open Settings, then LAN sync, then Link to a computer, then Scan QR code. If you have more than one install, pick the one to link. Or type one of these addresses and the pairing code {spacedCode(d.pairingCode)}.
            </div>
            {d.addresses.length > 0 ? <div className={st.rowDesc}>{d.addresses.join(" · ")}</div> : <div className={st.rowDesc}>Waiting for the sync server to start…</div>}
            <div className={st.rowDesc}>Linked devices talk over your home network without encryption, the same as sync with the code.</div>
            {d.uri && (
              <TextButton onClick={() => setShowText((v) => !v)} aria-expanded={showText}>
                {showText ? "Hide the link code" : "Show the link code as text"}
              </TextButton>
            )}
            {showText && d.uri && <code className={s.uri}>{d.uri}</code>}
          </div>
        </div>
      </div>
      {d.refused && <RefusedOffer refused={d.refused} empty={d.empty} />}
      <div role="list" aria-label="Linked devices">
        {d.links.length === 0 ? (
          <div className={s.searching}>No devices linked to this profile yet.</div>
        ) : (
          d.links.map((k) => <LinkRow key={k.peerDeviceId} link={k} />)
        )}
      </div>
      <div className={st.fieldLabel}>Link to another device</div>
      <LinkForm empty={d.empty} />
    </>
  );
}
