# Message Persistence Design

## Philosophy

Message persistence in this application is intentionally local and simple.
There is no distributed message store, no server-side history, and no
synchronization between peers. Each peer keeps its own log of messages
it personally received or sent.

---

## Log Location

Message logs are stored in a directory configurable via `~/.chatrc`.
The default location is:

```
~/.chat_logs/
```

Each room gets its own log file, named after the room:

```
~/.chat_logs/rust-chat.log
~/.chat_logs/general.log
```

On iSH (iPhone), the home directory is `/root` or the iSH app's home,
so logs are stored at `/root/.chat_logs/`.

---

## Log Format

Logs are plain UTF-8 text files. Each line represents one event.

### Chat Message

```
[2026-02-12T14:32:05Z] Seung#3f2a: hello everyone
```

### System Event

```
[2026-02-12T14:33:00Z] *** Alice#9d4e joined the room
[2026-02-12T14:45:12Z] *** Mike#7b1c disconnected
```

The format is intentionally human-readable. No binary format, no database.
The user can open a log file in any text editor.

---

## Write Strategy

- Messages are **appended** to the log file as they arrive.
- The file handle is opened once when the room is joined and closed when
  the room is left.
- Each write is followed by a flush to ensure messages are not lost if the
  application crashes.
- Log files are never truncated or rotated by the application in v1.

---

## What Is Logged

| Event | Logged? |
|-------|---------|
| Received chat messages (successfully decrypted) | Yes |
| Sent chat messages | Yes |
| Peer join / leave events | Yes |
| Password verification failures | No |
| Network errors | No |
| Messages that failed to decrypt | No |

Failed decryptions are not logged because they represent either noise from
unrelated peers on the same topic or a wrong-password attempt. Logging them
would produce meaningless binary data.

---

## What Is Not Provided

- **Log replay on join**: Logs are written locally and are not shared with
  other peers. A peer who joins a room late cannot retrieve messages sent
  before they arrived.
- **Log encryption**: Log files are stored in plaintext on disk. The
  assumption is that disk-level security is the user's responsibility.
- **Log rotation**: Log files grow indefinitely in v1. The user must manage
  disk usage manually.
- **Cross-device sync**: Logs on a PC and logs on iSH are separate. There
  is no mechanism to synchronize them.

---

## Accessing Logs

The user accesses logs directly from the filesystem:

```
cat ~/.chat_logs/rust-chat.log
grep "Seung" ~/.chat_logs/rust-chat.log
```

No in-app log viewer is provided in v1.
