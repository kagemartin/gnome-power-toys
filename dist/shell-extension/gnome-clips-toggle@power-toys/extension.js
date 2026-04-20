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
        const parameters = new GLib.Variant('(a{sv})', [{}]);
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
