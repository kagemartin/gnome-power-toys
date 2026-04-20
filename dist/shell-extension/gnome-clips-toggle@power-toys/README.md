# gnome-clips-toggle

Tiny GNOME Shell extension. Owns the `Super+V` hotkey and calls
`gnome-clips`'s `GtkApplication` over D-Bus (`org.freedesktop.Application.Activate`)
so it toggles whether the binary is already running or not.

This is the only reliable way to get a global shortcut under GNOME
Shell on Wayland: the media-keys "Custom Shortcut" mechanism runs an
external command and does not fire once Mutter is holding the key
grab, so it is the wrong abstraction for a session-long GtkApplication.

## Install

```bash
# Link (dev) or copy (install) into the shell's extension directory.
mkdir -p ~/.local/share/gnome-shell/extensions
ln -sfn "$(pwd)/dist/shell-extension/gnome-clips-toggle@power-toys" \
    ~/.local/share/gnome-shell/extensions/gnome-clips-toggle@power-toys

# Restart GNOME Shell so it picks up the new extension:
#   X11:     Alt+F2, type `r`, Enter
#   Wayland: log out and back in
# Then enable:
gnome-extensions enable gnome-clips-toggle@power-toys
```

## Configure

The accelerator lives in GSettings:

```bash
gsettings get  org.gnome.shell.extensions.gnome-clips-toggle toggle
gsettings set  org.gnome.shell.extensions.gnome-clips-toggle toggle "['<Super>v']"
```

## How it works

On enable, the extension calls `Main.wm.addKeybinding('toggle', …)`.
When the key fires, it makes a single D-Bus call:

- bus name  `org.gnome.Clips.Ui`
- object     `/org/gnome/Clips/Ui`
- interface  `org.freedesktop.Application`
- method     `Activate`

`gnome-clips` handles `Activate` by toggling the popup window's
visibility. If `gnome-clips` is not running the call fails (the extension
logs to `journalctl --user -u gnome-shell`); packaging will ship an
autostart file.
