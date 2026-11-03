// Sync (behaviour from ui/sync/SyncPage.kt): this device's pairing code and addresses, pulling
// another device's library over the LAN, and Mihon-compatible backup files.
import { Laptop, Plus, RefreshCw, Save, Wifi } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/commands";
import { queryClient, keys, useSyncStatus } from "../../api/queries";
import { ErrorState, Loading, OutlinedButton, PrimaryButton } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { PeerDto } from "../../gen/PeerDto";
import { cleanCode, isCode, parsePort, reportLine, spacedCode, splitAddress } from "../../lib/sync";
import l from "../Library/Library.module.css";
import st from "../Settings/Settings.module.css";
import { RestoreBackup } from "./RestoreBackup";
import { RestoreOffer } from "./RestoreOffer";
import s from "./Sync.module.css";

const DEFAULT_PORT = "27193";

function PeerRow({ peer, canPull, pulling, onPull }: { peer: PeerDto; canPull: boolean; pulling: boolean; onPull: () => void }) {
  return (
    <div className={st.row} role="listitem">
      <Laptop size={20} className={st.rowIcon} />
      <div className={st.rowText}>
        <span className={st.rowTitle}>{peer.name}</span>
        <span className={st.rowDesc}>
          {peer.host}:{peer.port}
        </span>
      </div>
      <PrimaryButton onClick={onPull} disabled={!canPull} busy={pulling} title={canPull ? undefined : "Enter the other device's pairing code first"}>
        {pulling ? "Syncing…" : "Pull"}
      </PrimaryButton>
    </div>
  );
}

export function SyncScreen() {
  const status = useSyncStatus();
  const [code, setCode] = useState("");
  const [host, setHost] = useState("");
  const [port, setPort] = useState(DEFAULT_PORT);
  const [addNote, setAddNote] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [pullingId, setPullingId] = useState<string | null>(null);
  const [backupNote, setBackupNote] = useState<{ ok: boolean; text: string } | null>(null);
  const [backupBusy, setBackupBusy] = useState<"export" | null>(null);

  if (status.isPending) return <Loading label="Starting sync…" />;
  if (status.isError) return <ErrorState message={(status.error as unknown as CmdError).message} onRetry={() => void status.refetch()} />;
  const st_ = status.data;
  const d = st_.device;
  const busy = st_.progress !== null || pullingId !== null;

  const pull = async (peer: PeerDto) => {
    setPullingId(peer.id);
    try {
      await api.syncPull(peer.id, code);
    } catch {
      // The error is in the status (lastError), shown below.
    } finally {
      setPullingId(null);
      void queryClient.invalidateQueries({ queryKey: keys.sync });
    }
  };

  const add = async () => {
    const p = parsePort(port);
    if (!host.trim() || p === null) {
      setAddNote("Enter an address and a port between 1 and 65535.");
      return;
    }
    setAdding(true);
    setAddNote(null);
    try {
      await api.syncAddPeer(host.trim(), p);
      setHost("");
    } catch (e) {
      setAddNote((e as CmdError).message);
    } finally {
      setAdding(false);
    }
  };

  const save = async () => {
    setBackupBusy("export");
    setBackupNote(null);
    try {
      const path = await api.backupExport();
      if (path) setBackupNote({ ok: true, text: `Saved ${path}` });
    } catch (e) {
      setBackupNote({ ok: false, text: (e as CmdError).message });
    } finally {
      setBackupBusy(null);
    }
  };

  const r = st_.lastReport;

  return (
    <div className={st.screen}>
      <div className={st.col}>
        <h1 className={st.title}>Sync</h1>

        <div className={st.section}>This device</div>
        <div className={s.card}>
          <div className={s.deviceName}>{d.name}</div>
          <div className={st.fieldLabel}>Pairing code</div>
          <div className={s.code} aria-label={`Pairing code ${d.pairingCode.split("").join(" ")}`}>
            {spacedCode(d.pairingCode)}
          </div>
          <div className={st.rowDesc}>Another device enters this code to pull your library.</div>
          {d.port !== null && (
            <div className={s.addresses}>
              <Wifi size={16} className={st.rowIcon} />
              <span className={st.rowDesc}>
                Listening on port {d.port}
                {d.addresses.length > 0 && <> · {d.addresses.map((a) => `${a}:${d.port}`).join(" · ")}</>}
              </span>
            </div>
          )}
          {d.error && (
            <div className={l.error} role="alert">
              {d.error}
            </div>
          )}
        </div>

        <div className={st.section}>Sync from another device</div>
        <label className={st.field}>
          <span className={st.fieldLabel}>Their pairing code</span>
          <input
            className={`${l.field} ${s.codeInput}`}
            value={spacedCode(code)}
            onChange={(e) => setCode(cleanCode(e.target.value))}
            placeholder="6-digit code"
            inputMode="numeric"
            autoComplete="off"
            spellCheck={false}
          />
        </label>
        <div className={st.fieldLabel}>Found on this network</div>
        <div role="list">
          {st_.peers.length === 0 ? (
            <div className={s.searching}>
              <RefreshCw size={16} className={s.spin} aria-hidden /> Looking for other TachiMana devices on this network…
            </div>
          ) : (
            st_.peers.map((p) => <PeerRow key={p.id} peer={p} canPull={isCode(code) && !busy} pulling={pullingId === p.id} onPull={() => void pull(p)} />)
          )}
        </div>
        <div className={st.fieldLabel}>Add by address</div>
        <div className={s.addRow}>
          <input
            className={l.field}
            value={host}
            onChange={(e) => {
              const sp = splitAddress(e.target.value);
              setHost(sp.host);
              if (sp.port) setPort(sp.port);
              setAddNote(null);
            }}
            onKeyDown={(e) => e.key === "Enter" && void add()}
            placeholder="192.168.x.x"
            aria-label="Address"
            spellCheck={false}
            maxLength={255}
          />
          <input className={`${l.field} ${s.port}`} value={port} onChange={(e) => setPort(e.target.value.replace(/\D/g, "").slice(0, 5))} aria-label="Port" inputMode="numeric" />
          <OutlinedButton onClick={() => void add()} disabled={!host.trim()} busy={adding}>
            {!adding && <Plus size={16} />} {adding ? "Checking…" : "Add"}
          </OutlinedButton>
        </div>
        {addNote && (
          <div className={l.error} role="alert">
            {addNote}
          </div>
        )}
        {st_.progress && (
          <div className={s.card} role="status">
            <span className={s.progress}>
              <RefreshCw size={16} className={s.spin} aria-hidden /> {st_.progress}
            </span>
          </div>
        )}
        {!st_.progress && st_.lastError && (
          <div className={l.error} role="alert">
            Sync failed: {st_.lastError}
          </div>
        )}
        {!st_.progress && r && (
          <div className={s.card} role="status">
            <div className={st.rowTitle}>Synced from {r.peerName}</div>
            <div className={st.rowDesc}>{reportLine(r)}</div>
            {r.summary.chaptersUpdated > 0 && <div className={st.rowDesc}>{r.summary.chaptersUpdated} chapters' progress merged</div>}
          </div>
        )}

        <div className={st.section}>Backup</div>
        <RestoreBackup disabled={backupBusy !== null || busy} />
        <div className={st.row}>
          <Save size={20} className={st.rowIcon} />
          <div className={st.rowText}>
            <span className={st.rowTitle}>Export backup</span>
            <span className={st.rowDesc}>Save a .tachibk that Mihon, Yōkai or Tachiyomi can import: your library, read progress, categories, tracking, extension repos, the extensions you have installed and which chapters are downloaded (not the files). Settings and passwords are not included.</span>
          </div>
          <OutlinedButton onClick={() => void save()} busy={backupBusy !== null}>
            {backupBusy === "export" ? "Saving…" : "Save file…"}
          </OutlinedButton>
        </div>
        {backupNote && (
          <div className={backupNote.ok ? st.ok : l.error} role={backupNote.ok ? "status" : "alert"}>
            {backupNote.text}
          </div>
        )}
        <RestoreOffer offer={st_.offer} />
      </div>
    </div>
  );
}
