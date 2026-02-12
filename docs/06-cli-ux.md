# CLI / UX Design

## Terminal Layout

The terminal is divided into two regions, managed by crossterm:

```
┌──────────────────────────────────────────────────────────────┐
│  Room: rust-chat                      3 peers online         │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  [14:30] Seung#3f2a: hello everyone                          │
│  [14:31] Mike#7b1c: hey!                                     │
│  [14:32] Seung#3f2a: what's up?                              │
│  [14:33] *** Alice#9d4e joined the room                      │
│  [14:34] Mike#7b1c: welcome Alice                            │
│                                                              │
│  (messages scroll upward as new ones arrive)                 │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│  > type your message here...                                 │
└──────────────────────────────────────────────────────────────┘
```

### Message Pane (top region)

- Occupies all terminal rows except the header and input bar.
- Messages are appended at the bottom; older messages scroll upward.
- Each message line is prefixed with a timestamp `[HH:MM]`.
- System events (join/leave) are prefixed with `***` to distinguish them
  from user messages.
- The pane redraws cleanly on terminal resize.

### Input Bar (bottom region)

- A single fixed line at the bottom of the terminal.
- Prefixed with `> ` to indicate it is the active input.
- The cursor stays in the input bar at all times.
- Incoming messages do not interrupt the typed text; only the message pane
  redraws.

### Header Bar

- A single fixed line at the top showing the current room name and peer count.
- Updated whenever a peer joins or leaves.

---

## Navigation and Interaction

### Main Menu (before entering a room)

Displayed as a simple numbered prompt:

```
=== P2P Chat ===
[1] Create room
[2] Join room
[Q] Quit
>
```

### Create Room Flow

```
Room name: rust-chat
Password (leave blank for none): ••••••••

Room created. Share this code with others:
  7xKpQm3NvBsRtYdEfGhJ2cLwAoP9uXiZ

Waiting for peers... (Ctrl-C to return to menu)
```

### Join Room Flow

```
Enter room code: 7xKpQm3NvBsRtYdEfGhJ2cLwAoP9uXiZ
Password (leave blank for none): ••••••••

Connecting...
Access denied — wrong password.
```

or on success:

```
Connecting...
Joined room: rust-chat  (2 peers online)
```

### In-Room Commands

All commands are typed in the input bar and begin with `/`:

| Command | Action |
|---------|--------|
| `/quit` | Leave the room and return to main menu |
| `/peers` | Print the list of currently connected peer nicknames |
| `/help` | Print the command list |

Any input not beginning with `/` is treated as a chat message and sent.

### Password Input

Password characters are masked with `•` during input. The masking is handled
by the CLI layer using crossterm's raw mode. The actual string is never
echoed to the terminal.

---

## Keyboard Shortcuts

| Key | Behavior |
|-----|---------|
| `Enter` | Send message / confirm input |
| `Ctrl-C` | Quit current context (room → menu, menu → exit) |
| `Backspace` | Delete last character in input bar |
| `↑` / `↓` | Scroll message pane (planned for v1) |

---

## Terminal Compatibility

The interface uses only basic ANSI escape sequences via crossterm. This
ensures compatibility with:

- Standard Linux/macOS terminals (xterm, iTerm2, Terminal.app)
- Windows Terminal and CMD (crossterm handles Windows console API)
- iSH on iPhone (uses hterm, which supports ANSI escapes)

No Unicode box-drawing characters are required for the layout. Plain ASCII
separators are used to ensure the interface renders correctly even in
environments with limited font support.

---

## Error and Status Messages

All status messages are printed to the message pane, prefixed with `[!]`:

```
[!] Could not connect to bootstrap nodes. Check your internet connection.
[!] Peer Mike#7b1c disconnected.
[!] Access denied — wrong password.
[!] Message too long (max 2048 characters).
```

The application never terminates unexpectedly due to a networking error.
It reports the error and returns to the main menu or continues operating
in a degraded state (e.g., mDNS-only if DHT bootstrap fails).
