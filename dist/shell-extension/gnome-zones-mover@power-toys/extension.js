// dist/shell-extension/gnome-zones-mover@power-toys/extension.js
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const DBUS_IFACE = `
<node>
  <interface name="org.gnome.Shell.Extensions.GnomeZonesMover">
    <method name="MoveResizeWindow">
      <arg type="t" direction="in" name="window_id" />
      <arg type="i" direction="in" name="x" />
      <arg type="i" direction="in" name="y" />
      <arg type="i" direction="in" name="w" />
      <arg type="i" direction="in" name="h" />
      <arg type="b" direction="out" name="ok" />
    </method>
    <method name="GetFocusedWindowId">
      <arg type="t" direction="out" name="window_id" />
    </method>
    <method name="ListWindowsInRect">
      <arg type="i" direction="in" name="x" />
      <arg type="i" direction="in" name="y" />
      <arg type="i" direction="in" name="w" />
      <arg type="i" direction="in" name="h" />
      <arg type="at" direction="out" name="window_ids" />
    </method>
    <method name="ActivateWindow">
      <arg type="t" direction="in" name="window_id" />
    </method>
    <method name="GetFocusedWindowWorkArea">
      <arg type="i" direction="out" name="x" />
      <arg type="i" direction="out" name="y" />
      <arg type="i" direction="out" name="w" />
      <arg type="i" direction="out" name="h" />
    </method>
    <method name="GetFrameRect">
      <arg type="t" direction="in" name="window_id" />
      <arg type="i" direction="out" name="x" />
      <arg type="i" direction="out" name="y" />
      <arg type="i" direction="out" name="w" />
      <arg type="i" direction="out" name="h" />
    </method>
    <method name="Unmaximize">
      <arg type="t" direction="in" name="window_id" />
      <arg type="b" direction="out" name="ok" />
    </method>
  </interface>
</node>
`;

// (schema-key, daemon-method, GVariant-type, args-builder)
// args-builder receives nothing and returns the Array passed to GLib.Variant.
const HOTKEYS = [
    ...[1,2,3,4,5,6,7,8,9].map(n => ['snap-' + n, 'SnapFocusedToZone', '(ub)', () => [n, false]]),
    ...[1,2,3,4,5,6,7,8,9].map(n => ['span-' + n, 'SnapFocusedToZone', '(ub)', () => [n, true]]),
    ['activator',   'ShowActivator',      null,  () => null],
    ['iter-prev',   'IterateFocusedZone', '(s)', () => ['prev']],
    ['iter-next',   'IterateFocusedZone', '(s)', () => ['next']],
    ['cycle-prev',  'CycleFocusInZone',   '(i)', () => [-1]],
    ['cycle-next',  'CycleFocusInZone',   '(i)', () => [1]],
    ['editor',      'ShowEditor',         null,  () => null],
    ['pause',       'TogglePaused',       null,  () => null],
    ['restore',     'RestoreFocusedWindow', null, () => null],
];

export default class GnomeZonesMoverExtension extends Extension {
    enable() {
        this._impl = Gio.DBusExportedObject.wrapJSObject(DBUS_IFACE, this);
        this._impl.export(Gio.DBus.session, '/org/gnome/Shell/Extensions/GnomeZonesMover');

        this._settings = this.getSettings();
        this._registered = [];

        for (const [key, method, sig, argsOf] of HOTKEYS) {
            const ok = Main.wm.addKeybinding(
                key,
                this._settings,
                Meta.KeyBindingFlags.NONE,
                Shell.ActionMode.NORMAL,
                () => this._callDaemon(method, sig, argsOf()),
            );
            if (ok === Meta.KeyBindingAction.NONE) {
                log('[gnome-zones-mover] failed to grab accelerator for ' + key);
            } else {
                this._registered.push(key);
            }
        }

        log('[gnome-zones-mover] enabled; registered ' + this._registered.length + ' hotkeys');
    }

    disable() {
        for (const key of this._registered) {
            Main.wm.removeKeybinding(key);
        }
        this._registered = [];
        this._settings = null;

        if (this._impl) {
            this._impl.unexport();
            this._impl = null;
        }
        log('[gnome-zones-mover] disabled');
    }

    _callDaemon(method, sig, args) {
        const variant = (sig && args) ? new GLib.Variant(sig, args) : null;
        Gio.DBus.session.call(
            'org.gnome.Zones',
            '/org/gnome/Zones',
            'org.gnome.Zones',
            method,
            variant,
            null,
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (conn, res) => {
                try {
                    conn.call_finish(res);
                } catch (e) {
                    logError(e, '[gnome-zones-mover] ' + method + ' failed');
                }
            }
        );
    }

    // --- D-Bus methods (window mover) ---

    MoveResizeWindow(window_id, x, y, w, h) {
        const win = this._findWindow(window_id);
        if (!win) return false;
        try {
            if (win.get_maximized()) {
                win.unmaximize(Meta.MaximizeFlags.BOTH);
            }
            if (win.is_fullscreen()) {
                win.unmake_fullscreen();
            }

            // Mirror Tiling Assistant's tile() sequence (see
            //   /usr/share/gnome-shell/extensions/tiling-assistant@ubuntu.com/
            //     src/extension/tilingWindowManager.js:123-225
            // ) which is known to work for size-hinted apps like
            // gnome-terminal. The critical pieces beyond a plain
            // move_resize_frame are:
            //
            //  1. `override_constraints(...)` — when present on mutter
            //     (GNOME 46+), bypass the size-hint rounding path that
            //     drops the move component.
            //  2. `move_to_monitor(...)` — reset mutter's monitor-relative
            //     state so the position isn't clamped back to the window's
            //     previous monitor.
            //  3. `move_frame` THEN `move_resize_frame` — establish
            //     position first, then apply combined geometry. The
            //     intermediate positioned state survives the size
            //     negotiation.
            if (win.override_constraints && Meta.WindowConstraint) {
                win.override_constraints(
                    Meta.WindowConstraint.WINDOW,
                    Meta.WindowConstraint.WINDOW,
                    Meta.WindowConstraint.WINDOW,
                    Meta.WindowConstraint.WINDOW,
                );
            }
            win.move_to_monitor(win.get_monitor());
            win.move_frame(true, x, y);
            win.move_resize_frame(true, x, y, w, h);
            return true;
        } catch (e) {
            logError(e, '[gnome-zones-mover] MoveResizeWindow failed');
            return false;
        }
    }

    GetFocusedWindowId() {
        const win = global.display.focus_window;
        return win ? win.get_id() : 0;
    }

    ListWindowsInRect(x, y, w, h) {
        const actors = global.get_window_actors();
        const x1 = x + w, y1 = y + h;
        return actors
            .map(a => a.meta_window)
            .filter(w => w && !w.is_hidden() && !w.minimized)
            .filter(w => {
                const r = w.get_frame_rect();
                const cx = r.x + r.width  / 2;
                const cy = r.y + r.height / 2;
                return cx >= x && cx < x1 && cy >= y && cy < y1;
            })
            .map(w => w.get_id());
    }

    ActivateWindow(window_id) {
        const win = this._findWindow(window_id);
        if (win) {
            win.activate(global.get_current_time());
        }
    }

    GetFocusedWindowWorkArea() {
        const win = global.display.focus_window;
        const monitor = win ? win.get_monitor() : global.display.get_primary_monitor();
        const workspace = global.workspace_manager.get_active_workspace();
        const wa = workspace.get_work_area_for_monitor(monitor);
        return [wa.x, wa.y, wa.width, wa.height];
    }

    GetFrameRect(window_id) {
        const win = this._findWindow(window_id);
        if (!win) return [0, 0, 0, 0];
        const r = win.get_frame_rect();
        return [r.x, r.y, r.width, r.height];
    }

    Unmaximize(window_id) {
        const win = this._findWindow(window_id);
        if (!win) return false;
        try {
            if (win.get_maximized()) {
                win.unmaximize(Meta.MaximizeFlags.BOTH);
            }
            return true;
        } catch (e) {
            logError(e, '[gnome-zones-mover] Unmaximize failed');
            return false;
        }
    }

    // --- helpers ---

    _findWindow(id) {
        return global.get_window_actors()
            .map(a => a.meta_window)
            .find(w => w && w.get_id() === id) || null;
    }
}
