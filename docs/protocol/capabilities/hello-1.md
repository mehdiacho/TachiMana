# `hello/1`: who a device is and what it implements

State: **built on both sides, not yet seen working together.** The computer has it from 0.1.18.
The phone has it from 0.21.3-custom (TachiMana#43) and already refuses to sync changes with a
computer that has no `/hello`, so a computer older than 0.1.18 does not sync with that phone build.

## Purpose

Before `hello/1` the apps found out what the other one could do by calling a route and seeing
whether it failed. With it, each device says what it implements, and the other one shows or hides
features to match. A feature the other device lacks is never shown to the person as a failure.

## Message

`GET /hello`, no authentication. It says nothing that `/ping` and the app's public release do not
already say.

```json
{
  "protocol": "1",
  "deviceId": "…", "deviceName": "…", "platform": "android",
  "profileId": "…", "profileName": "…",
  "app": {"name": "mihon-custom", "version": "0.19.1-c7"},
  "capabilities": ["hello/1", "link/2", "pull/1", "queue/1", "done/1", "file/1"]
}
```

| Key | Meaning |
| --- | --- |
| `protocol` | Major version of the ground rules (README §2). `"1"` today. |
| `deviceId`, `deviceName`, `profileId`, `profileName` | The same values as `/ping`. |
| `platform` | `"android"`, `"windows"`, `"linux"` or `"macos"`. For showing an icon; never for deciding what to call. |
| `app.name`, `app.version` | For a person to read ("its app is 0.19.1-c7"). Never compared. |
| `capabilities` | Every capability this device implements **its side of**, as `name/major`. |

## Rules

1. A device lists a capability when it implements its own side of it as that capability's page
   describes, whether that side serves the routes, calls them, or both.
2. A device that answers `/hello` has `hello/1`, whether or not it lists it (the phone does not).
   A device lists only what works now. The computer leaves out `changes/1` while it has no
   journal, and lists only `hello/1` and `pull/1` while linking is off.
3. **Use only what both sides list.** Before offering a feature that needs a capability, check
   the other device's list. When it is missing, hide the feature or say in plain words what the
   other app can't do yet.
4. A device whose `protocol` is another major is reachable, and nothing may be used with it.
5. No `/hello` (404, or a body that is not this JSON) while `/ping` answers means an app from
   before `hello/1`: treat it as `link/2`, `pull/1`.
6. Neither route answering means the device is away. That is not a missing feature.
7. Unknown keys are ignored. An unknown capability is kept and never asked for. Entries that are
   not `name/major` (lower-case letters, digits and `-`, a slash, a number) are dropped.
8. A reader keeps at most 64 capabilities and reads at most 64 KB.
9. Ask again from time to time (the computer asks each minute while the Sync screen is open). The
   other app can be updated without the link changing.

## Test files

`docs/protocol/vectors/hello-1/*.json`. Each has the `/hello` body (`null` when the route gives
no success; a string when the body is not JSON), the `/ping` body, and what must follow:
`reachable`, the `capabilities` kept, and `supports` for some capabilities.

The computer runs them in `tm-sync` (`protocol::tests::hello_vectors`).

## On the computer

- `GET /hello` is served by `tm-sync`'s server.
- `SyncClient::hello(host, port)` asks, and falls back to `/ping`.
- The Sync screen asks each linked device and, when its app lacks `changes/1` or `roles/1`,
  says so under the device ("The app on Pixel can't sync changes yet…").
- Not done yet: the calls themselves still try their route and handle a refusal, as before.
  They do not consult `/hello` first.
