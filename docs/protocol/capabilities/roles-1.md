# `roles/1`: who leads, and the one-off copies

Status: **proposed, 2026-11-14.** Mehdi's decisions (system plan §9): two-way by default (D1); in
a one-way mode the follower works normally and its changes are not sent (D2). The phone side
asked to fold mihon#9 ("keep the computer's library") into this.

## Purpose

After two devices link, each holds the same answer to "how do these two stay in step", and either
can change it. The same capability carries the questions asked right after scanning: which library
to start from, and the one-off "send everything / take everything".

## The mode of a link

Each side stores, per link: `mode` and `modeAt` (ms, when it was last changed, on the device that
changed it).

| `mode` on the wire | Meaning |
| --- | --- |
| `both` | changes go both ways (`changes/1` in full). Default. |
| `callerLeads` | the device that sent the message leads; the other follows |
| `serverLeads` | the device that answered leads; the caller follows |

Each app turns this into "I lead / I follow / both" for itself; the wire names are always from the
point of view of the message, so they cannot be read backwards.

Rules, used by `changes/1`:
- A leader ignores entries made on a device that follows it.
- A follower applies the leader's entries and keeps journaling its own, which are not sent. (A
  follower may still serve them; the leader ignores them.)

## Messages

### `GET /roles` (with `X-Link-Key`)

```json
{"mode": "both", "modeAt": 1763130000000}
```

### `PUT /roles` (with `X-Link-Key`)

```json
{"mode": "serverLeads", "modeAt": 1763130500000}
```

- `200` with the stored value when `modeAt` is later than the stored one.
- `409` with the stored value when the stored one is later (the other side changed it meanwhile);
  the caller takes the stored value and shows it.
- The device that receives a new mode shows a short notice ("Study PC now follows Mehdi's phone").

### `POST /roles/take` (with `X-Link-Key`)

The caller asks the server to take the caller's library once. This is "Send everything to the
phone once" pressed on the computer, and the start-from question when the person picks the
caller's library.

```json
{"how": "merge", "requestId": "…"}
```

- `how`: `merge` (combine; nothing is removed, the caller's values win where both have one) or
  `replace` (the server's library becomes the caller's; the server saves a backup of its own
  first).
- The server answers `202 {"requestId": "…"}` and asks its user to confirm. `replace` always asks,
  naming both counts ("Replace this phone's 1,688 manga with Study PC's 212?"). `merge` may be
  confirmed once and remembered for this link.
- The server then copies through `pull/1` (`GET /backup`, `GET /manifest`) from the caller, and
  reports with a done event on `done/1`'s outbox: `{"kind": "take", "requestId": "…", "ok": true}`
  or `"ok": false, "message": "…"`. Declining is `ok: false` with the message "Declined".
- A repeated `requestId` is answered `202` again and not run twice.

"Take everything from the phone once" pressed on the computer needs no message: the computer
copies from the phone through `pull/1` itself, as Sync from another device does today.

After a take, each side sets its `changes/1` cursor for the other to the other's current `next`,
so the copy is not followed by a replay of old entries.

## The questions after scanning

Asked on the device where the person is (the one that scanned or showed the code), in this order:

1. Start from which library: only when both have one. Mine, the other's, or combine both.
   "The other's" is a local take (`pull/1`); "mine" is `POST /roles/take` with `replace`;
   "combine" is a local merge followed by `POST /roles/take` with `merge`.
2. How to stay in step: both (default), this device leads, the other leads. Sent with `PUT /roles`.

Both questions are reachable again later from the Sync screen ("Change…").

## Test files

`docs/protocol/vectors/roles-1/`: `PUT /roles` with an older and a newer `modeAt`; `take` with a
repeated `requestId`.
