// Linked devices (docs/rust-port/profiles-and-remote-downloads.md §2): this profile's link code as
// a QR code and as text, linking to another device, and the list with Unlink. A linked device
// talks to this one without the six-digit code.
import { Laptop, Link2, Smartphone, Unlink } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/commands";
import { keys, queryClient, useLinks } from "../../api/queries";
import { OutlinedButton, Skeleton, TextButton } from "../../components/ui";
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

function LinkForm() {
  const [address, setAddress] = useState("");
  const [code, setCode] = useState("");
  const pasted = address.trim().startsWith("tachimana://");
  const link = useAction(
    async () => {
      const made = await api.linkDevice(address, code);
      await queryClient.invalidateQueries({ queryKey: keys.links });
      return made;
    },
    { ok: (m) => `Linked to ${m.peerName}.` },
  );
  const submit = async () => {
    if ((await link.run()) !== undefined) {
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
        <input className={l.field} value={address} onChange={(e) => (setAddress(e.target.value), link.setError(null))} placeholder="192.168.x.x:27193 or tachimana://link?…" aria-label="Address or link code" spellCheck={false} maxLength={4096} />
        {!pasted && (
          <input className={`${l.field} ${s.codeInput}`} value={spacedCode(code)} onChange={(e) => setCode(cleanCode(e.target.value))} placeholder="6-digit code" inputMode="numeric" aria-label="Their pairing code" autoComplete="off" />
        )}
        <OutlinedButton type="submit" busy={link.pending} disabled={!address.trim() || (!pasted && code.length !== 6)}>
          {!link.pending && <Link2 size={16} />} {link.pending ? "Linking…" : "Link"}
        </OutlinedButton>
      </div>
      {link.error && (
        <div className={l.error} role="alert">
          {link.error}
        </div>
      )}
    </form>
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
              In the phone app, open Settings, then LAN sync, then Link to a computer, and scan this code. Or type one of these addresses and the pairing code {spacedCode(d.pairingCode)}.
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
      <div role="list" aria-label="Linked devices">
        {d.links.length === 0 ? (
          <div className={s.searching}>No devices linked to this profile yet.</div>
        ) : (
          d.links.map((k) => <LinkRow key={k.peerDeviceId} link={k} />)
        )}
      </div>
      <div className={st.fieldLabel}>Link to another device</div>
      <LinkForm />
    </>
  );
}
