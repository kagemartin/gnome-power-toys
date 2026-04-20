// Owns the Super+V hotkey and the auto-paste injection that makes a
// clipboard-history selection actually land in the previously-focused
// window (same UX as GPaste / clipmenu).
//
// Flow:
//  1. Hotkey fires  -> remember global.display.focus_window, call Activate
//                       on the gnome-clips GtkApplication over D-Bus.
//  2. User picks a clip in the popup -> UI writes it to the system
//     clipboard via the daemon and then calls InjectPaste on us.
//  3. InjectPaste   -> re-focus the remembered window and synthesize
//                       Shift+Insert via Clutter's virtual keyboard
//                       device so the target app receives the paste.

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const UI_BUS_NAME    = 'org.gnome.Clips.Ui';
const UI_OBJECT_PATH = '/org/gnome/Clips/Ui';
const UI_IFACE       = 'org.freedesktop.Application';

const EXT_OBJECT_PATH = '/org/gnome/Shell/Extensions/GnomeClipsToggle';
const EXT_IFACE       = 'org.gnome.Shell.Extensions.GnomeClipsToggle';

const DBUS_INTERFACE = `
<node>
  <interface name="${EXT_IFACE}">
    <method name="InjectPaste"/>
  </interface>
</node>
`;

export default class GnomeClipsToggleExtension extends Extension {
    enable() {
        this._settings = this.getSettings();
        this._registered = [];
        this._preFocused = null;

        this._impl = Gio.DBusExportedObject.wrapJSObject(DBUS_INTERFACE, this);
        this._impl.export(Gio.DBus.session, EXT_OBJECT_PATH);

        const ok = Main.wm.addKeybinding(
            'toggle',
            this._settings,
            Meta.KeyBindingFlags.NONE,
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
            () => this._onHotkey(),
        );
        if (ok === Meta.KeyBindingAction.NONE) {
            log('[gnome-clips-toggle] failed to grab accelerator for toggle');
        } else {
            this._registered.push('toggle');
            log('[gnome-clips-toggle] enabled');
        }
    }

    disable() {
        for (const key of this._registered) {
            Main.wm.removeKeybinding(key);
        }
        this._registered = [];
        this._settings = null;
        this._preFocused = null;
        if (this._impl) {
            this._impl.unexport();
            this._impl = null;
        }
        log('[gnome-clips-toggle] disabled');
    }

    _onHotkey() {
        // Remember what had focus before the popup so we can send the
        // paste back there. Skip our own popup if that's what's focused
        // (shouldn't happen — we're the thing that's about to show).
        const cur = global.display.focus_window;
        this._preFocused = (cur && cur.get_wm_class() !== 'org.gnome.Clips.Ui')
            ? cur
            : null;
        this._activateUi();
    }

    _activateUi() {
        // Signature: Activate(a{sv} platform-data) -> nothing. The
        // `parameters` argument to DBus.call() must be a tuple Variant
        // wrapping the method's argument list. desktop-startup-id with a
        // valid X-server timestamp stops Mutter's focus-stealing
        // prevention from denying the popup focus.
        const time = global.display.get_current_time_roundtrip();
        const startupId = `gnome-clips-toggle_TIME${time}`;
        const platformData = {
            'desktop-startup-id': new GLib.Variant('s', startupId),
        };
        const parameters = new GLib.Variant('(a{sv})', [platformData]);
        Gio.DBus.session.call(
            UI_BUS_NAME,
            UI_OBJECT_PATH,
            UI_IFACE,
            'Activate',
            parameters,
            null,
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (conn, res) => {
                try {
                    conn.call_finish(res);
                } catch (e) {
                    logError(e, '[gnome-clips-toggle] Activate failed (is gnome-clips running?)');
                }
            },
        );
    }

    // D-Bus: called by gnome-clips right after it writes the selected
    // clip to the system clipboard and hides the popup. Restore focus
    // to the window the user was in before Super+V, then synthesize the
    // paste shortcut appropriate for that window type.
    InjectPaste() {
        const target = this._preFocused;
        this._preFocused = null;

        const useShiftCtrl = isTerminal(target);

        if (target && !target.is_hidden()) {
            const now = global.display.get_current_time_roundtrip();
            try {
                target.activate(now);
            } catch (e) {
                logError(e, '[gnome-clips-toggle] refocus failed');
            }
        }

        // Give Mutter a tick to actually move focus before we inject.
        // One main-loop idle pass is usually enough; a 30 ms timeout is
        // a conservative upper bound.
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 30, () => {
            this._synthesizePaste(useShiftCtrl);
            return GLib.SOURCE_REMOVE;
        });
    }

    // Sends Ctrl+V (GUI apps) or Ctrl+Shift+V (terminals). Shift+Insert
    // is not portable: VTE terminals map it to PRIMARY selection, so we
    // avoid it entirely.
    _synthesizePaste(useShiftCtrl) {
        try {
            const seat = Clutter.get_default_backend().get_default_seat();
            const vdev = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
            const t = Clutter.get_current_event_time();

            vdev.notify_keyval(t, Clutter.KEY_Control_L, Clutter.KeyState.PRESSED);
            if (useShiftCtrl) {
                vdev.notify_keyval(t, Clutter.KEY_Shift_L, Clutter.KeyState.PRESSED);
            }
            vdev.notify_keyval(t, Clutter.KEY_v, Clutter.KeyState.PRESSED);
            vdev.notify_keyval(t, Clutter.KEY_v, Clutter.KeyState.RELEASED);
            if (useShiftCtrl) {
                vdev.notify_keyval(t, Clutter.KEY_Shift_L, Clutter.KeyState.RELEASED);
            }
            vdev.notify_keyval(t, Clutter.KEY_Control_L, Clutter.KeyState.RELEASED);
        } catch (e) {
            logError(e, '[gnome-clips-toggle] key injection failed');
        }
    }
}

// Heuristic — terminals treat Ctrl+V as verbatim-insert, not paste, and
// expect Ctrl+Shift+V instead. Match the common VTE/Konsole/etc. wm
// classes; everything else gets Ctrl+V.
function isTerminal(win) {
    if (!win) return false;
    const cls = (win.get_wm_class() || '').toLowerCase();
    const name = (win.get_wm_class_instance?.() || '').toLowerCase();
    const s = cls + ' ' + name;
    const matchers = [
        'terminal',  // gnome-terminal, xfce4-terminal, lxterminal, etc.
        'xterm',
        'konsole',
        'alacritty',
        'kitty',
        'tilix',
        'wezterm',
        'rxvt',
        'foot',
        'ptyxis',
    ];
    return matchers.some(m => s.includes(m));
}
