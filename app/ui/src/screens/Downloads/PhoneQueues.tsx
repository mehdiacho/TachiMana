// The linked phones' download queues (docs/rust-port/profiles-and-remote-downloads.md §3): what is
// queued on each phone, "Download all here", and where the chapters go once downloaded.
import { useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { Check, CircleAlert, CloudUpload, Download, Laptop, Send, Smartphone } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../../api/commands";
import { queryClient, useLinks } from "../../api/queries";
import { Chip, OutlinedButton, Skeleton, TextButton } from "../../components/ui";
import type { Dest } from "../../gen/Dest";
import type { LinkDto } from "../../gen/LinkDto";
import type { PhoneJobDto } from "../../gen/PhoneJobDto";
import { useAction } from "../../hooks/useAction";
import b from "../Browse/Browse.module.css";
import s from "./Downloads.module.css";

const key = (peer: string) => ["phoneQueue", peer] as const;
/** Items shown before "Show all". */
const FOLDED = 6;

const destLabel: Record<Dest, string> = { pc: "This PC", nas: "NAS", phone: "Back to the phone" };

function destText(dest: Dest, phone: string): string {
  if (dest === "pc") return `Kept on this PC. ${phone} is told where they are.`;
  if (dest === "nas") return `Uploaded to the NAS, then removed from this PC. ${phone} reads them from there.`;
  return `Sent to ${phone}'s downloads, then removed from this PC. A chapter waits here while ${phone} is away.`;
}

function jobText(j: PhoneJobDto, phone: string): string {
  switch (j.stage) {
    case "downloading":
      return "downloading here";
    case "uploading":
      return "uploading to the NAS";
    case "sending":
      return j.error ? `waiting to send to ${phone}: ${j.error}` : `sending to ${phone}`;
    case "done":
      return j.dest === "pc" ? "on this PC" : j.dest === "nas" ? "on the NAS" : `on ${phone}`;
    default:
      return `failed, given back to ${phone}${j.error ? `: ${j.error}` : ""}`;
  }
}

function JobIcon({ j }: { j: PhoneJobDto }) {
  if (j.stage === "done") return <Check size={18} />;
  if (j.stage === "failed") return <CircleAlert size={18} />;
  if (j.stage === "uploading") return <CloudUpload size={18} />;
  if (j.stage === "sending") return <Send size={18} />;
  return <Download size={18} />;
}

function PhoneQueue({ link }: { link: LinkDto }) {
  const peer = link.peerDeviceId;
  const phone = link.peerName;
  const q = useQuery({ queryKey: key(peer), queryFn: () => api.phoneQueue(peer), refetchInterval: 10_000 });
  const [dest, setDest] = useState<Dest | null>(null);
  const [open, setOpen] = useState(false);
  const chosen: Dest = dest ?? q.data?.dest ?? "pc";
  const resume = useAction(async () => {
    const r = await api.phoneQueueResume(peer, chosen);
    await queryClient.invalidateQueries({ queryKey: key(peer) });
    return r;
  }, {
    ok: (r) =>
      r.claimed === 0 && r.missing === 0
        ? `Nothing to take over from ${phone}.`
        : `Downloading ${r.claimed} chapter${r.claimed === 1 ? "" : "s"} from ${phone}'s queue here.${r.missing ? ` ${r.missing} aren't in this library yet; sync the library and try again.` : ""}`,
  });
  const clear = useAction(async () => {
    await api.phoneQueueClearFinished(peer);
    await queryClient.invalidateQueries({ queryKey: key(peer) });
  });
  const pick = (d: Dest) => {
    setDest(d);
    void api.phoneQueueSetDest(peer, d).catch(() => undefined);
  };

  const d = q.data;
  const items = d?.items ?? [];
  const shown = open ? items : items.slice(0, FOLDED);
  const jobs = d?.jobs ?? [];
  const working = jobs.filter((j) => j.stage !== "done" && j.stage !== "failed").length;
  const canGet = items.filter((i) => i.here).length;
  return (
    <div className={s.phone} aria-label={`${phone}'s queue`}>
      <div className={s.head} style={{ marginBottom: 0 }}>
        <Smartphone size={18} aria-hidden />
        <span className={b.itemTitle}>
          On {phone}
          {d && !d.error ? ` · ${d.total} queued` : ""}
        </span>
        <span className={s.grow} />
        {jobs.some((j) => j.stage === "done" || j.stage === "failed") && (
          <TextButton busy={clear.pending} onClick={() => void clear.run()}>
            Clear finished
          </TextButton>
        )}
      </div>
      {q.isPending ? (
        <Skeleton width="60%" />
      ) : q.isError ? (
        <div className={b.notice}>Couldn't read {phone}'s queue.</div>
      ) : d?.error ? (
        <div className={b.itemSub}>
          {phone} isn't reachable right now ({d.error}). Open the app on {phone} on the same Wi-Fi.
        </div>
      ) : (
        <>
          {items.length > 0 && (
            <div role="list" aria-label={`Queued on ${phone}`}>
              {shown.map((i, n) => (
                <div key={n} className={b.item} role="listitem">
                  <span className={b.itemText}>
                    <span className={b.itemTitle}>{i.mangaTitle || "Untitled"}</span>
                    <span className={b.itemSub}>
                      {i.chapterName}
                      {i.here ? "" : " · not in this library yet"}
                    </span>
                  </span>
                </div>
              ))}
              {items.length > FOLDED && (
                <TextButton onClick={() => setOpen((v) => !v)} aria-expanded={open}>
                  {open ? "Show fewer" : `Show all ${d?.total ?? items.length}`}
                </TextButton>
              )}
            </div>
          )}
          {items.length === 0 && working === 0 && <div className={b.itemSub}>Nothing queued on {phone}.</div>}
          {items.length > 0 && (
            <>
              <div className={b.itemSub}>Put them on</div>
              <div className={s.dests} role="radiogroup" aria-label="Where the chapters go">
                {(["pc", "nas", "phone"] as Dest[]).map((x) => (
                  <Chip key={x} active={chosen === x} role="radio" aria-checked={chosen === x} disabled={x === "nas" && !d?.nasReady} onClick={() => pick(x)}>
                    {x === "pc" ? <Laptop size={14} /> : x === "nas" ? <CloudUpload size={14} /> : <Smartphone size={14} />} {destLabel[x]}
                  </Chip>
                ))}
              </div>
              <div className={b.itemSub}>
                {destText(chosen, phone)}
                {!d?.nasReady && " Set up the NAS in Settings to use it here."}
              </div>
              <div>
                <OutlinedButton busy={resume.pending} disabled={canGet === 0} onClick={() => void resume.run()}>
                  {resume.pending ? "Taking over…" : `Download all here (${canGet})`}
                </OutlinedButton>
              </div>
              {resume.error && (
                <div className={b.notice} role="alert">
                  {resume.error}
                </div>
              )}
            </>
          )}
        </>
      )}
      {jobs.length > 0 && (
        <div role="list" aria-label={`Downloading for ${phone}`}>
          <div className={b.itemSub}>
            {working > 0 ? `${working} in progress for ${phone}` : `Done for ${phone}`}
            {d && d.unsentEvents > 0 ? ` · ${phone} hasn't been told about ${d.unsentEvents} yet` : ""}
          </div>
          {jobs.slice(0, 50).map((j, n) => (
            <div key={n} className={b.item} role="listitem">
              <span className={s.status} data-status={j.stage === "failed" ? "FAILED" : j.stage === "done" ? "DONE" : "DOWNLOADING"} aria-hidden>
                <JobIcon j={j} />
              </span>
              <span className={b.itemText}>
                <span className={b.itemTitle}>{j.label}</span>
                <span className={b.itemSub}>{jobText(j, phone)}</span>
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/** One section per linked phone; nothing when no phone is linked. */
export function PhoneQueues() {
  const links = useLinks();
  useEffect(() => {
    const off = listen("phone-queue", () => void queryClient.invalidateQueries({ queryKey: ["phoneQueue"] }));
    return () => {
      void off.then((f) => f()).catch(() => undefined);
    };
  }, []);
  const phones = (links.data?.links ?? []).filter((l) => l.platform === "android");
  if (phones.length === 0) return null;
  return (
    <>
      <div className={s.section}>Linked phones</div>
      {phones.map((l) => (
        <PhoneQueue key={l.peerDeviceId} link={l} />
      ))}
    </>
  );
}
