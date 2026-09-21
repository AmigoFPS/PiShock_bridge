# Pishock_bridge

Fires PiShock devices from VRChat avatar contacts. Touch a contact on the avatar, the routed device gets a beep, vibration or shock.

## Build

Install Rust (stable, via [rustup](https://rustup.rs)). Then:

```powershell
cargo build --release
```

The binary is `target/release/pishock_bridge.exe`. Run it from a folder that contains your `config.json` (see below), or use `cargo run --release` from the project folder.

## Devices (`config.json`)

Copy `config.example.json` to `config.json` next to the exe and fill it in:

```json
{
  "pishock": {
    "username": "<YOUR_PISHOCK_USERNAME>",
    "api_key": "<YOUR_PISHOCK_API_KEY>",
    "name": "Pishock_bridge",
    "devices": [
      { "name": "Left",  "share_code": "1A2B3C4D5E6" },
      { "name": "Right", "share_code": "https://pishock.com/#/Control?sharecode=1A2B3C4D5E6" },
      { "name": "Both",  "share_code": "https://pishock.com/#/Control?sharecode=1A2B3C4D5E6,1A2B3C4D5E6" }
    ]
  }
}
```

- `username` is your PiShock account name; `api_key` comes from pishock.com → Account → API keys. Both are required.
- `name` is how the app appears in PiShock's logs. Optional, defaults to `Pishock_bridge`.
- `devices` is the list of shockers you can route contacts to. Each entry has:
  - `name`: the label shown in the app and used for routing. Optional (defaults to `Device 1`, `Device 2`, ...), but names must be unique.
  - `share_code`: a share code made on pishock.com → Share (limits set there apply). Any of these forms work:
    - the bare code: `1A2B3C4D5E6`
    - the control link: `https://pishock.com/#/Control?sharecode=7F8E9D0C1B2`
    - a "both" link with two codes: `...sharecode=A1B2C3D4E5F,F5E4D3C2B1A` — this becomes **two** devices, `Both 1` and `Both 2`, one per code.
- Placeholder values in `<angle brackets>`, empty strings and entries without a share code are ignored. Without a username, API key and at least one device the app runs in monitor-only mode and says so under CREDENTIALS.

The older single-device form is still accepted: a `"share_code"` directly under `"pishock"` becomes a device named `PiShock` (or `PiShock 1` / `PiShock 2` for a comma pair). Environment variables `PISHOCK_USERNAME`, `PISHOCK_API_KEY`, `PISHOCK_NAME` and `PISHOCK_SHARE_CODE` override the matching fields.

After editing the file, click **RELOAD CONFIG.JSON** in the app; no restart needed. Each device in the list has a **TEST** button that sends one pulse with the current settings, so you can check a code before routing anything to it.

## Avatar contacts

The app listens for avatar parameters whose name starts with `SHK` (case-sensitive), for example `SHK/Head`, `SHK/Back`, `SHK/HandL`. Every such parameter shows up as a contact in the app. A contact fires when its value goes from ≤ 0.5 to > 0.5.

To add one to your avatar in Unity (VRChat SDK, Avatars 3.0):

1. **Pick the spot.** Select the bone or object where the touch should count — for example the head bone, spine/chest, or a hand bone — and add a child GameObject to it if you want to position the zone freely.

2. **Add a Contact Receiver.** `Add Component` → **VRC Contact Receiver**.
   - **Shape / Radius / Position**: a Sphere or Capsule around the area. Radius is in metres; 0.1-0.2 is a typical pat-sized zone.
   - **Allow Self / Allow Others**: who can trigger it. Turn on *Allow Others* so other players' hands count; *Allow Self* lets your own hands trigger it (handy for testing).
   - **Collision Tags**: which senders trigger it. VRChat's built-in tags on every player are `Head`, `Torso`, `Hand`, `HandL`, `HandR`, `Finger`, `FingerL`, `FingerR`, `FingerIndex`, `FingerMiddle`, `FingerRing`, `FingerLittle` (each with `L`/`R` variants), `Foot`, `FootL`, `FootR`. `Hand` and `Finger` are the usual choice for pats. A custom tag only works with a matching *VRC Contact Sender* on the other avatar.
   - **Receiver Type**:
     - **Constant** — the parameter is 1 while something is inside, 0 otherwise. Recommended: one clean trigger per touch.
     - **Proximity** — the parameter goes from 0 at the edge to 1 at the centre. The app triggers once the sender gets inside half the radius.
     - **OnEnter** — a short 1 when something enters. Also works.
   - **Parameter**: the name, starting with `SHK`, for example `SHK/Head`.

3. **Add the parameter to Expression Parameters.** Open the avatar's *VRC Expression Parameters* asset and add a row with exactly the same name. Type **Float** (works for every receiver type; use **Bool** only for Constant/OnEnter). Default `0`, *Saved* off. *Synced* can be off: VRChat still sends unsynced parameters over OSC, and unsynced ones do not use the 256-bit sync budget. A parameter that is not in this list is never sent over OSC, so the app will not see it.

   The FX animator does not need to use the parameter; the receiver writes it and OSC reads it. If you do add it to an animator, just create a parameter of the same name and type.

4. **Upload** the avatar (Build & Publish) and wear it.

5. **Enable OSC in VRChat**: Action Menu → Options → OSC → Enabled. Start the app. The header shows `VRCHAT CONNECTED` once VRChat starts sending, and every `SHK` parameter appears under AVATAR CONTACTS with a live value bar — touch the zone and watch the bar move. If VRChat is found but nothing arrives, toggle OSC off and on in the Action Menu.

Repeat for as many zones as you like; each one gets its own row in the app and its own device routing.

## Run

1. Start VRChat (OSC enabled) and the app.
2. For each contact row, click the device chips under ROUTE to choose which devices it fires. One contact can fire several devices.
3. Pick BEEP / VIBRATE / SHOCK, intensity and seconds. RANDOM turns those into maximums and rolls a value per activation; COOLDOWN is how long a device ignores further touches after a pulse.
4. Click **ARM OUTPUT**. Touching a routed contact now fires its devices. **Esc** or the same button disarms; routing or setting changes, an avatar change, a VRChat disconnect and any PiShock error also disarm.

The panel at the bottom shows, per device, what was sent and PiShock's answer.
