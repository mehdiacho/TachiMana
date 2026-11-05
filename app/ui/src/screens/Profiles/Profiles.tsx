// Profiles: the title-bar switcher and the Settings section. Each profile is a whole separate
// library; switching restarts the app on the other one (docs/rust-port/profiles-and-remote-downloads.md §1).
import { Check, ChevronDown, Plus, Settings2, Trash2, UserRound } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../../api/commands";
import { keys, queryClient, useProfiles } from "../../api/queries";
import { OutlinedButton, PrimaryButton, Skeleton, Spinner, TextButton } from "../../components/ui";
import type { ProfileDto } from "../../gen/ProfileDto";
import { useAction } from "../../hooks/useAction";
import { toast } from "../../store/activity";
import { useUi } from "../../store/ui";
import l from "../Library/Library.module.css";
import st from "../Settings/Settings.module.css";
import s from "./Profiles.module.css";

export const COLORS = ["#4f8cff", "#f5a524", "#22c55e", "#ef4444", "#a855f7", "#14b8a6", "#ec4899", "#94a3b8"];

export function initial(name: string): string {
  return Array.from(name.trim())[0]?.toUpperCase() ?? "?";
}

function Avatar({ p, size = 22 }: { p: Pick<ProfileDto, "name" | "color">; size?: number }) {
  return (
    <span className={s.avatar} style={{ background: p.color, width: size, height: size, fontSize: size * 0.5 }} aria-hidden>
      {initial(p.name)}
    </span>
  );
}

/** "Switching to X…" while the app restarts; an error only if the restart was refused. */
function useSwitch() {
  const [to, setTo] = useState<string | null>(null);
  const run = useAction(async (p: ProfileDto) => {
    setTo(p.name);
    try {
      await api.profileSwitch(p.id);
    } finally {
      // Reached only when the restart didn't happen.
      setTo(null);
    }
  }, { toastError: true });
  return { to, switchTo: (p: ProfileDto) => void run.run(p) };
}

function NewProfileForm({ onDone, autoFocus = true }: { onDone: (created: ProfileDto | null) => void; autoFocus?: boolean }) {
  const [name, setName] = useState("");
  const [color, setColor] = useState(COLORS[1] ?? "#f5a524");
  const create = useAction(async () => {
    const before = new Set((queryClient.getQueryData<{ profiles: ProfileDto[] }>(keys.profiles)?.profiles ?? []).map((p) => p.id));
    const list = await api.profileCreate({ name, color });
    queryClient.setQueryData(keys.profiles, list);
    return list.profiles.find((p) => !before.has(p.id)) ?? null;
  });
  const submit = async () => {
    const made = await create.run();
    if (made !== undefined) onDone(made);
  };
  return (
    <form
      className={s.form}
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <input
        className={l.field}
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Profile name, e.g. Second phone"
        maxLength={40}
        autoFocus={autoFocus}
        aria-label="Profile name"
        onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), onDone(null))}
      />
      <div className={s.swatches} role="radiogroup" aria-label="Colour">
        {COLORS.map((c) => (
          <button key={c} type="button" role="radio" aria-checked={c === color} aria-label={c} className={s.swatch} data-active={c === color} style={{ background: c }} onClick={() => setColor(c)} />
        ))}
      </div>
      {create.error && <div className={l.error}>{create.error}</div>}
      <div className={st.buttons}>
        <PrimaryButton type="submit" busy={create.pending} disabled={name.trim() === ""}>
          {create.pending ? "Creating…" : "Create"}
        </PrimaryButton>
        <TextButton type="button" onClick={() => onDone(null)}>
          Cancel
        </TextButton>
      </div>
    </form>
  );
}

/** The avatar and name in the title bar, with a menu of the other profiles. */
export function ProfileSwitcher() {
  const profiles = useProfiles();
  const setScreen = useUi((u) => u.setScreen);
  const closeReader = useUi((u) => u.closeReader);
  const [open, setOpen] = useState(false);
  const [adding, setAdding] = useState(false);
  const { to, switchTo } = useSwitch();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && (setOpen(false), setAdding(false));
    const esc = (e: KeyboardEvent) => e.key === "Escape" && (setOpen(false), setAdding(false));
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  const list = profiles.data?.profiles ?? [];
  const current = list.find((p) => p.open);
  if (profiles.isPending) return <Skeleton width="96px" height={22} radius="11px" />;
  if (!current) return null;

  return (
    <div className={s.switcher} ref={ref}>
      <button type="button" className={s.trigger} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((v) => !v)} title={open ? undefined : "Switch profile"}>
        {to ? <Spinner size={14} /> : <Avatar p={current} size={18} />}
        <span className={s.triggerName}>{to ? `Switching to ${to}…` : current.name}</span>
        <ChevronDown size={14} />
      </button>
      {open && (
        <div className={s.menu} role="menu" aria-label="Profiles">
          <div className={s.menuHead}>Profiles</div>
          {list.map((p) => (
            <button
              key={p.id}
              type="button"
              role="menuitemradio"
              aria-checked={p.open}
              className={s.item}
              disabled={to !== null}
              onClick={() => (p.open ? setOpen(false) : switchTo(p))}
            >
              <Avatar p={p} />
              <span className={s.itemName}>{p.name}</span>
              {p.open ? <Check size={16} /> : to === p.name ? <Spinner size={14} /> : null}
            </button>
          ))}
          <div className={s.sep} />
          {adding ? (
            <NewProfileForm
              onDone={(made) => {
                setAdding(false);
                if (made) toast.ok(`Created ${made.name}. Switch to it to set it up.`);
              }}
            />
          ) : (
            <>
              <button type="button" role="menuitem" className={s.item} onClick={() => setAdding(true)}>
                <Plus size={18} className={s.itemIcon} />
                <span className={s.itemName}>New profile…</span>
              </button>
              <button
                type="button"
                role="menuitem"
                className={s.item}
                onClick={() => {
                  setOpen(false);
                  closeReader();
                  setScreen("settings");
                }}
              >
                <Settings2 size={18} className={s.itemIcon} />
                <span className={s.itemName}>Manage profiles…</span>
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function ProfileRow({ p, only }: { p: ProfileDto; only: boolean }) {
  const [mode, setMode] = useState<"view" | "edit" | "delete">("view");
  const [name, setName] = useState(p.name);
  const [color, setColor] = useState(p.color);
  const [confirm, setConfirm] = useState("");
  const { to, switchTo } = useSwitch();
  const save = useAction(async () => queryClient.setQueryData(keys.profiles, await api.profileEdit(p.id, { name, color })), {
    ok: () => "Profile saved.",
  });
  const remove = useAction(async () => queryClient.setQueryData(keys.profiles, await api.profileDelete(p.id)), {
    ok: () => `Deleted ${p.name}. Its files were moved to the .deleted folder inside the profiles folder, in case you need them.`,
  });

  if (mode === "edit") {
    return (
      <div className={s.rowEdit}>
        <input className={l.field} value={name} onChange={(e) => setName(e.target.value)} maxLength={40} aria-label="Profile name" autoFocus />
        <div className={s.swatches} role="radiogroup" aria-label="Colour">
          {COLORS.map((c) => (
            <button key={c} type="button" role="radio" aria-checked={c === color} aria-label={c} className={s.swatch} data-active={c === color} style={{ background: c }} onClick={() => setColor(c)} />
          ))}
        </div>
        {save.error && <div className={l.error}>{save.error}</div>}
        <div className={st.buttons}>
          <PrimaryButton busy={save.pending} onClick={() => void save.run().then((r) => r !== undefined && setMode("view"))}>
            {save.pending ? "Saving…" : "Save"}
          </PrimaryButton>
          <TextButton onClick={() => (setMode("view"), setName(p.name), setColor(p.color))}>Cancel</TextButton>
        </div>
      </div>
    );
  }

  if (mode === "delete") {
    return (
      <div className={s.rowEdit} role="alertdialog" aria-label={`Delete ${p.name}?`}>
        <div className={st.rowTitle}>Delete {p.name}?</div>
        <div className={st.rowDesc}>
          Its library, downloads, extensions and settings leave TachiMana. The files are moved to a .deleted folder inside the profiles folder rather than erased. Type the profile's name to confirm.
        </div>
        <input className={l.field} value={confirm} onChange={(e) => setConfirm(e.target.value)} placeholder={p.name} aria-label="Type the profile name to confirm" autoFocus />
        {remove.error && <div className={l.error}>{remove.error}</div>}
        <div className={st.buttons}>
          <OutlinedButton busy={remove.pending} disabled={confirm.trim().toLowerCase() !== p.name.toLowerCase()} onClick={() => void remove.run()}>
            {remove.pending ? "Deleting…" : "Delete profile"}
          </OutlinedButton>
          <TextButton onClick={() => (setMode("view"), setConfirm(""))}>Cancel</TextButton>
        </div>
      </div>
    );
  }

  return (
    <div className={st.row}>
      <Avatar p={p} size={28} />
      <div className={st.rowText}>
        <span className={st.rowTitle}>{p.name}</span>
        <span className={st.rowDesc}>{p.open ? "Open now" : to ? "Restarting into this profile…" : "Its own library, extensions, downloads and settings"}</span>
      </div>
      <div className={st.buttons}>
        {!p.open && (
          <OutlinedButton busy={to !== null} onClick={() => switchTo(p)}>
            {to ? "Switching…" : "Switch"}
          </OutlinedButton>
        )}
        <TextButton onClick={() => setMode("edit")}>Edit</TextButton>
        {!p.open && !only && p.removable && (
          <TextButton onClick={() => setMode("delete")} aria-label={`Delete ${p.name}`}>
            <Trash2 size={16} />
          </TextButton>
        )}
      </div>
    </div>
  );
}

/** Settings → Profiles. */
export function ProfilesSection() {
  const profiles = useProfiles();
  const [adding, setAdding] = useState(false);
  const list = profiles.data?.profiles ?? [];
  if (profiles.isPending) {
    return (
      <div className={st.row}>
        <Skeleton width="28px" height={28} radius="14px" />
        <div className={st.rowText}>
          <Skeleton width="40%" height={14} />
          <Skeleton width="60%" height={12} />
        </div>
      </div>
    );
  }
  if (profiles.isError) return <div className={st.err}>Couldn't read the profiles: {(profiles.error as { message?: string }).message}</div>;
  return (
    <>
      <div className={st.rowDesc} style={{ padding: "0 var(--tm-space-xl)" }}>
        Each profile is a whole separate TachiMana, like a second Mihon install: its own library, extensions, downloads, NAS, tracking and settings. Switching restarts the app on the other profile.
      </div>
      {list.map((p) => (
        <ProfileRow key={p.id} p={p} only={list.length === 1} />
      ))}
      {adding ? (
        <div className={s.rowEdit}>
          <NewProfileForm
            onDone={(made) => {
              setAdding(false);
              if (made) toast.ok(`Created ${made.name}.`);
            }}
          />
        </div>
      ) : (
        <div className={st.row}>
          <UserRound size={20} className={st.rowIcon} />
          <div className={st.rowText}>
            <span className={st.rowTitle}>New profile</span>
            <span className={st.rowDesc}>Starts empty. Restore a backup into it after switching to bring a library over.</span>
          </div>
          <OutlinedButton onClick={() => setAdding(true)}>
            <Plus size={16} /> New profile
          </OutlinedButton>
        </div>
      )}
    </>
  );
}
