# SupaOmaMail

SupaOmaMail is a fork of [Omamail](https://github.com/huacnlee/omamail) by huacnlee: a native email and calendar app with multiple accounts and keyboard navigation, running as an Omarchy shell plugin. It adds Gmail triage features on top, and it installs beside Omamail with its own settings, sign-ins and data, so the two never touch each other.

<img width="800" alt="Reading mail with AI assistance" src="docs/images/full-mail.webp" />

## What SupaOmaMail adds

- **Primary and Other tabs for the Gmail Inbox.** Turn them on per mailbox in Settings → Mailboxes → "Split the Inbox into Primary and Other". Promotions, Social and Forums go to Other; everything else, Updates included, stays in Primary. `Tab` and `Shift+Tab` switch tabs.
- **Gmail's importance marker.** Important messages show the marker in the list; `+` (or `=`) marks a message important and `-` marks it not important, which is what Gmail learns from.
- **Snooze on `h`** (or Gmail's own `b`). Type when the message should come back: `2m` offers two minutes, then two months, then two Mondays; `tom 3` offers tomorrow at 15:00, then 03:00; `fri 9am`, `oct 5`, `next week` and the like work too, and the arrows walk the answers. Day-only answers wake at 08:00. The message leaves the Inbox at once, comes back unread at its time with a "Back from snooze" notification, and a Snoozed mailbox lists what is waiting. `Alt+Z` or the toast's Undo takes a snooze back.

Snoozing needs the backend built from this repository (see below). A snoozed message is filed under the Gmail label `SupaOmaMail/Snoozed` while it is away, so Gmail on the web and your phone show it there too; it comes back only while SupaOmaMail's backend is running, which on Omarchy is whenever the shell is.

Everything else is Omamail's: Gmail, Outlook, HEY, JMAP and IMAP/SMTP mailboxes; mail and calendar; keyboard navigation (`?` shows every key, or see the [keyboard guide](docs/KEYS.md)); AI assistance through your Omarchy agent; native notifications; credentials in the system keyring; and remote images blocked until you load them.

## Install on Omarchy

SupaOmaMail is installed from source, because its backend has features no Omamail release carries. It needs **Omarchy 4** and the build tools:

```bash
sudo pacman -S --needed rustup cmake qt6-declarative nodejs python
rustup default stable
```

Clone it, run the tests, then install:

```bash
git clone https://github.com/jrwoods99/supaomamail.git ~/supaomamail
cd ~/supaomamail
make test
make install
```

`make install` builds the backend into `~/.local/share/supaomamail/`, links the plugin into `~/.config/omarchy/plugins/supaomamail`, and restarts the Omarchy shell. A second envelope appears in the bar; an Omamail you already have stays exactly as it is. To update later, `git pull` in the checkout and run `make install` again.

Then add your mailbox in SupaOmaMail. It keeps its own accounts, so you sign in once here even if Omamail already has the mailbox. Gmail needs a Google OAuth client: reuse Omamail's by copying `~/.config/omamail/credentials.json` to `~/.config/supaomamail/credentials.json`, or set one up as described in [mailbox setup](docs/MAILBOXES.md).

To remove SupaOmaMail, delete the plugin link and restart the shell; its data is in `~/.config/supaomamail`, `~/.cache/supaomamail`, `~/.local/state/supaomamail` and `~/.local/share/supaomamail`.

```bash
rm ~/.config/omarchy/plugins/supaomamail
omarchy restart shell
```

## Make SupaOmaMail the default mail client

Installing SupaOmaMail never takes `mailto:` links or `SUPER+SHIFT+E` from anything else. Choose **Settings → Default mail client → Set as default** to make it open `mailto:` links, `SUPER+SHIFT+E`, and `SUPER+SHIFT+ALT+E` for a new message. The key bindings go in a clearly marked block in `~/.config/hypr/bindings.lua`; **Undo** removes the block. If Omamail was made the default too, whichever was set last is the one the keys open.

To open SupaOmaMail with another key, add this to `~/.config/hypr/bindings.lua`:

```lua
o.bind("SUPER + SHIFT + G", "SupaOmaMail", "omarchy shell shell toggle supaomamail '{}'")
```

## Help and contributing

- [Backend installation, updates and recovery](docs/BACKEND-RUNTIME.md); the documents under `docs/` are Omamail's and still use its name.
- The features above are kept on their own branches in Omamail's naming, so each can be offered back to Omamail.

SupaOmaMail and Omamail are independent projects, not affiliated with Google, Microsoft or 37signals. Gmail, Outlook and HEY belong to their respective trademark owners.

Licensed under the [MIT License](LICENSE), as Omamail is; Omamail's copyright notice is kept in it.
