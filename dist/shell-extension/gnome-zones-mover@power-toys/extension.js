// dist/shell-extension/gnome-zones-mover@power-toys/extension.js
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';

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
  </interface>
</node>
`;

export default class GnomeZonesMoverExtension {
    constructor(metadata) {
        this._metadata = metadata;
        this._impl = null;
    }

    enable() {
        this._impl = Gio.DBusExportedObject.wrapJSObject(DBUS_IFACE, this);
        this._impl.export(Gio.DBus.session, '/org/gnome/Shell/Extensions/GnomeZonesMover');
        log('[gnome-zones-mover] enabled');
    }

    disable() {
        if (this._impl) {
            this._impl.unexport();
            this._impl = null;
        }
        log('[gnome-zones-mover] disabled');
    }

    // --- D-Bus methods ---

    MoveResizeWindow(window_id, x, y, w, h) {
        const win = this._findWindow(window_id);
        if (!win) return false;
        try {
            // Unmaximize first — spec §4. Otherwise move_resize_frame is ignored.
            if (win.get_maximized()) {
                win.unmaximize(Meta.MaximizeFlags.BOTH);
            }
            if (win.is_fullscreen()) {
                win.unmake_fullscreen();
            }
            // `true` = user-resize, so GTK clients pick up the new size.
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

    // --- helpers ---

    _findWindow(id) {
        return global.get_window_actors()
            .map(a => a.meta_window)
            .find(w => w && w.get_id() === id) || null;
    }
}
