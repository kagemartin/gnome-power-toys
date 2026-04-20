// Registers Super+V (configurable) via Main.wm.addKeybinding and calls
// the gnome-clips GtkApplication's org.freedesktop.Application.Activate
// method on press. The UI process handles "activate" as "toggle window".

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const UI_BUS_NAME    = 'org.gnome.Clips.Ui';
const UI_OBJECT_PATH = '/org/gnome/Clips/Ui';
const UI_IFACE       = 'org.freedesktop.Application';

export default class GnomeClipsToggleExtension extends Extension {
    enable() {
        this._settings = this.getSettings();
        this._registered = [];

        const ok = Main.wm.addKeybinding(
            'toggle',
            this._settings,
            Meta.KeyBindingFlags.NONE,
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
            () => this._activateUi(),
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
        log('[gnome-clips-toggle] disabled');
    }

    _activateUi() {
        // Signature: Activate(a{sv} platform-data) → nothing. The
        // `parameters` argument to DBus.call() must be a tuple Variant
        // wrapping the method's argument list.
        //
        // We pass `desktop-startup-id` with a valid X-server timestamp so
        // GTK can hand it to Mutter via _NET_STARTUP_ID. Without this,
        // Mutter's focus-stealing-prevention denies focus to the popup
        // intermittently (the activation is not tied to a user event and
        // our bus call arrives on a different client connection).
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
}
