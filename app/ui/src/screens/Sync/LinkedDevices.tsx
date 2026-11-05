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

/** What "keep theirs" does here, in a sentence. */
function keepTheirsText(empty: boolean, other: string): string {
  return empty
    ? `This profile is empty, so it becomes ${other}'s profile and copies its library. TachiMana restarts to do it.`
    : `This profile already has a library, so it stays as it is and a new profile is made for ${other}'s library. TachiMana restarts into it.`;
}

/** The choice when the two devices are on different profiles. */
function KeepChoice({ message, other, empty, busy, onTheirs, onCancel }: { message: string; other: string; empty: boolean; busy: boolean; onTheirs: () => void; onCancel: () => void }) {
  return (
    <div className={s.choice} role="alertdialog" aria-label="Choose whose library to keep">
      <div className={st.rowDesc}>{message}</div>
      <div className={s.option}>
        <div className={st.rowTitle}>Keep {other}'s library</div>
        <div className={st.rowDesc}>{keepTheirsText(empty, other)}</div>
        <OutlinedButton busy={busy} onClick={onTheirs}>
          {busy ? "Linking…" : `Keep ${other}'s`}
        </OutlinedButton>
      </div>
      <div className={s.option}>
        <div className={st.rowTitle}>Keep this PC's library</div>
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
  const pasted = address.trim().startsWith("tachimana://");
  const link = useAction(
    async (keep?: "theirs") => {
      try {
        const out = await api.linkDevice(address, code, keep);
        await queryClient.invalidateQueries({ queryKey: keys.links });
        setOther(null);
        return out;
      } catch (e) {
        if ((e as CmdError).kind === "otherProfile") setOther((e as CmdError).message);
        throw e;
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
        <KeepChoice message={other} other="the other device" empty={empty} busy={link.pending} onTheirs={() => void link.run("theirs")} onCancel={() => (setOther(null), link.setError(null))} />
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
  const [choosing, setChoosing] = useState(false);
  const take = useAction(async () => api.linkRefused(code), {
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
      {!choosing ? (
        <>
          <div className={st.rowDesc}>Its library and this PC's are different. Choose whose to keep.</div>
          <div className={st.buttons}>
            <OutlinedButton onClick={() => setChoosing(true)}>Keep {who}'s</OutlinedButton>
            <TextButton busy={dismiss.pending} onClick={() => void dismiss.run()}>
              Keep this PC's
            </TextButton>
          </div>
          <div className={st.rowDesc}>Keeping this PC's replaces {who}'s library, so finish it on {who}: scan again and pick "Keep the computer's".</div>
        </>
      ) : (
        <>
          <div className={st.rowDesc}>{keepTheirsText(empty, who)} Enter the pairing code shown on {who} (Settings, then LAN sync).</div>
          <div className={s.addRow}>
            <input className={`${l.field} ${s.codeInput}`} value={spacedCode(code)} onChange={(e) => setCode(cleanCode(e.target.value))} placeholder="6-digit code" inputMode="numeric" aria-label={`${who}'s pairing code`} autoComplete="off" autoFocus />
            <OutlinedButton busy={take.pending} disabled={code.length !== 6} onClick={() => void take.run()}>
              {take.pending ? "Linking…" : "Link and copy its library"}
            </OutlinedButton>
            <TextButton onClick={() => setChoosing(false)}>Back</TextButton>
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
