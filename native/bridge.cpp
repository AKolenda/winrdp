// SPDX-License-Identifier: AGPL-3.0-only
// GTK headers precede Qt headers: avoid Qt's signals/sessions keyword macros in C headers.
#include <gtk/gtk.h>
#include "bridge.h"
#include "rdp_session.hpp"
#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMutex>
#include <QMutexLocker>
#include <algorithm>
#include <cstring>
#include <deque>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>

namespace {
using velo::RdpSession;
constexpr int MaxSessions = 8;
constexpr guint16 Move = 0x0800, Down = 0x8000, Left = 0x1000, Right = 0x2000;
constexpr guint16 Middle = 0x4000, Wheel = 0x0200, HWheel = 0x0400;
struct Slot {
    guint64 id = 0;
    std::unique_ptr<RdpSession> session;
    GtkWidget *area = nullptr;
    QImage frame;
    QMutex pendingLock;
    QImage cursor;
    QPoint hotSpot;
    bool cursorPending = false, cursorHidden = false, remove = false;
    QString clipboard;
    bool clipboardPending = false, shareClipboard = true;
    QSize desired, sentSize;
    gint64 resizeAt = 0;
    bool dynamicResize = true;
    double scrollX = 0, scrollY = 0;
    ~Slot() {
        // Callers only destroy sessions after the worker has finished.
        if (area) {
            g_signal_handlers_disconnect_by_data(area, this);
            gtk_widget_destroy(area);
            g_object_unref(area);
        }
    }
};
GtkWidget *web = nullptr;
GtkWidget *stack = nullptr;
GtkWidget *grip = nullptr;
GtkBox *box = nullptr;
// Height of the HTML chrome (title bar plus session toolbar), matching app.css.
constexpr int kChromeHeight = 92;
// WebKitWebView reports its natural height as its current allocation, and a GtkBox
// gives a non-expanding child its natural size. A minimum size request on the webview
// therefore cannot shrink it. Pinning the stack's minimum instead leaves no slack, so
// the webview collapses to exactly the chrome height.
bool sessionMode = false;
int pinnedHeight = -1;
void pinSurface(int totalHeight) {
    if (!stack) return;
    if (!sessionMode) {
        if (pinnedHeight != -1) { pinnedHeight = -1; gtk_widget_set_size_request(stack, -1, -1); }
        return;
    }
    const int reserved = grip && gtk_widget_get_visible(grip) ? gtk_widget_get_allocated_height(grip) : 0;
    const int height = std::max(0, totalHeight - kChromeHeight - reserved);
    if (height == pinnedHeight) return;
    pinnedHeight = height;
    gtk_widget_set_size_request(stack, -1, height);
}
gboolean configured(GtkWidget *, GdkEventConfigure *e, gpointer) {
    pinSurface(e->height);
    return FALSE;
}
std::map<guint64, std::unique_ptr<Slot>> sessions;
guint64 nextId = 1, active = 0;
std::deque<QJsonObject> events;
QMutex eventLock;
std::unique_ptr<QCoreApplication> qt;

void push(guint64 id, const QString &kind, QJsonObject value = {}) {
    value["session"] = QString::number(id); value["kind"] = kind;
    QMutexLocker guard(&eventLock);
    if (events.size() >= 128) {
        const auto transient = std::find_if(events.begin(), events.end(), [](const QJsonObject &e) {
            return e["kind"] != "certificate" && e["kind"] != "ended";
        });
        if (transient != events.end()) events.erase(transient);
        else return; // At most eight workers can have outstanding certificate requests.
    }
    events.push_back(std::move(value));
}
char *encode(const QJsonValue &v) {
    const QByteArray bytes = v.isArray() ? QJsonDocument(v.toArray()).toJson(QJsonDocument::Compact)
                                       : QJsonDocument(v.toObject()).toJson(QJsonDocument::Compact);
    return g_strndup(bytes.constData(), static_cast<gsize>(bytes.size()));
}
char *error(const QString &s) { return encode(QJsonObject{{"error", s}}); }
Slot *lookup(guint64 id) {
    const auto it = sessions.find(id);
    return it == sessions.end() ? nullptr : it->second.get();
}
Slot &get(const QJsonObject &o) {
    bool valid = false;
    const auto id = o["session"].toString().toULongLong(&valid);
    auto *s = valid ? lookup(id) : nullptr;
    if (!s || s->remove) throw std::runtime_error("This session is no longer available.");
    return *s;
}
void deactivate() {
    for (auto &item : sessions) {
        item.second->session->releaseAllKeys();
        item.second->session->setClipboardActive(false);
    }
}
void showLibrary() {
    deactivate(); active = 0;
    sessionMode = false;
    gtk_widget_hide(stack);
    pinSurface(0);
    gtk_widget_set_size_request(web, -1, -1);
    gtk_widget_set_vexpand(web, TRUE);
    gtk_box_set_child_packing(box, web, TRUE, TRUE, 0, GTK_PACK_START);
}
struct ClipboardRequest { guint64 id; };
void clipboardRead(GtkClipboard *, const gchar *text, gpointer data) {
    std::unique_ptr<ClipboardRequest> request(static_cast<ClipboardRequest *>(data));
    Slot *s = lookup(request->id);
    if (!s || s->remove || active != request->id || !gtk_widget_has_focus(s->area)) return;
    s->session->setLocalClipboard(text ? QString::fromUtf8(text, static_cast<int>(strnlen(text, 2 * 1024 * 1024))) : QString{});
}
void requestClipboard(Slot &s) {
    if (s.shareClipboard) gtk_clipboard_request_text(
        gtk_clipboard_get(GDK_SELECTION_CLIPBOARD), clipboardRead, new ClipboardRequest{s.id});
}
void select(Slot &s) {
    deactivate(); active = s.id;
    const auto name = std::to_string(s.id);
    gtk_stack_set_visible_child_name(GTK_STACK(stack), name.c_str());
    gtk_widget_set_size_request(web, -1, kChromeHeight);
    gtk_widget_set_vexpand(web, FALSE);
    gtk_box_set_child_packing(box, web, FALSE, FALSE, 0, GTK_PACK_START);
    sessionMode = true;
    pinSurface(gtk_widget_get_allocated_height(GTK_WIDGET(box)));
    gtk_widget_show(stack); gtk_widget_show(s.area); gtk_widget_grab_focus(s.area);
    s.session->setClipboardActive(s.shareClipboard);
    requestClipboard(s);
}
velo::core::MappedPoint point(Slot &s, double x, double y) {
    return velo::core::mapPointer(gtk_widget_get_allocated_width(s.area),
        gtk_widget_get_allocated_height(s.area), {0, 0, s.frame.width(), s.frame.height()}, x, y);
}
gboolean draw(GtkWidget *widget, cairo_t *cr, gpointer data) {
    auto &s = *static_cast<Slot *>(data);
    cairo_set_source_rgb(cr, 0.035, 0.043, 0.055); cairo_paint(cr);
    if (s.frame.isNull()) return FALSE;
    const auto w = gtk_widget_get_allocated_width(widget), h = gtk_widget_get_allocated_height(widget);
    const double scale = std::min(double(w) / s.frame.width(), double(h) / s.frame.height());
    if (scale <= 0) return FALSE;
    cairo_surface_t *surface = cairo_image_surface_create_for_data(s.frame.bits(), CAIRO_FORMAT_RGB24,
        s.frame.width(), s.frame.height(), s.frame.bytesPerLine());
    if (cairo_surface_status(surface) != CAIRO_STATUS_SUCCESS) { cairo_surface_destroy(surface); return FALSE; }
    cairo_save(cr);
    cairo_translate(cr, (w - scale * s.frame.width()) / 2.0, (h - scale * s.frame.height()) / 2.0);
    cairo_scale(cr, scale, scale);
    cairo_set_source_surface(cr, surface, 0, 0);
    cairo_pattern_set_filter(cairo_get_source(cr), scale == 1.0 ? CAIRO_FILTER_NEAREST : CAIRO_FILTER_BILINEAR);
    cairo_paint(cr); cairo_restore(cr); cairo_surface_destroy(surface);
    return FALSE;
}
gboolean motion(GtkWidget *, GdkEventMotion *e, gpointer data) {
    auto &s = *static_cast<Slot *>(data); const auto p = point(s, e->x, e->y);
    if (p.inside) s.session->sendMouse(Move, p.x, p.y);
    return TRUE;
}
gboolean button(GtkWidget *, GdkEventButton *e, gpointer data) {
    auto &s = *static_cast<Slot *>(data);
    if (e->type != GDK_BUTTON_PRESS && e->type != GDK_BUTTON_RELEASE) return TRUE;
    const bool down = e->type == GDK_BUTTON_PRESS;
    const auto p = point(s, e->x, e->y);
    if (down && !p.inside) return TRUE;
    gtk_widget_grab_focus(s.area);
    const guint16 code = e->button == 1 ? Left : e->button == 2 ? Middle : e->button == 3 ? Right : 0;
    if (code) s.session->sendMouse(code | (down ? Down : 0), p.x, p.y);
    else if (e->button == 8 || e->button == 9)
        s.session->sendMouse((e->button == 8 ? 1 : 2) | (down ? Down : 0), p.x, p.y, true);
    return TRUE;
}
gboolean scroll(GtkWidget *, GdkEventScroll *e, gpointer data) {
    auto &s = *static_cast<Slot *>(data); const auto p = point(s, e->x, e->y);
    if (!p.inside) return TRUE;
    double dx = 0, dy = 0;
    if (!gdk_event_get_scroll_deltas(reinterpret_cast<GdkEvent *>(e), &dx, &dy)) {
        if (e->direction == GDK_SCROLL_UP) dy = -1;
        if (e->direction == GDK_SCROLL_DOWN) dy = 1;
        if (e->direction == GDK_SCROLL_LEFT) dx = -1;
        if (e->direction == GDK_SCROLL_RIGHT) dx = 1;
    }
    s.scrollX += dx * 120; s.scrollY -= dy * 120;
    for (int axis = 0; axis < 2; ++axis) {
        auto &remainder = axis ? s.scrollX : s.scrollY;
        const int delta = std::clamp(static_cast<int>(remainder), -255, 255);
        if (delta) { s.session->sendMouse((axis ? HWheel : Wheel) | (delta & 0x01ff), p.x, p.y); remainder -= delta; }
    }
    return TRUE;
}
gboolean key(GtkWidget *widget, GdkEventKey *e, gpointer data) {
    auto &s = *static_cast<Slot *>(data); const bool down = e->type == GDK_KEY_PRESS;
    const bool local = (e->state & GDK_CONTROL_MASK) && (e->state & GDK_MOD1_MASK);
    if (local && (e->keyval == GDK_KEY_Escape || e->keyval == GDK_KEY_Home || e->keyval == GDK_KEY_Return)) {
        s.session->releaseAllKeys();
        if (down) {
            if (e->keyval == GDK_KEY_Escape) {
                s.session->setClipboardActive(false); gtk_widget_grab_focus(web);
                push(s.id, "input-released");
            } else {
                auto *top = gtk_widget_get_toplevel(widget);
                const auto state = gdk_window_get_state(gtk_widget_get_window(top));
                const bool leave = e->keyval == GDK_KEY_Home || (state & GDK_WINDOW_STATE_FULLSCREEN);
                if (leave) gtk_window_unfullscreen(GTK_WINDOW(top)); else gtk_window_fullscreen(GTK_WINDOW(top));
                push(s.id, "fullscreen", {{"enabled", !leave}});
            }
        }
        return TRUE;
    }
    // GDK X11/Wayland hardware keycodes use the XKB evdev+8 convention on our Linux target.
    if (e->hardware_keycode >= 8) {
        const auto scan = velo::core::evdevToScanCode(e->hardware_keycode - 8);
        if (scan) s.session->sendScanCode(*scan, down);
    }
    return TRUE;
}
gboolean focus(GtkWidget *, GdkEventFocus *e, gpointer data) {
    auto &s = *static_cast<Slot *>(data);
    const bool enabled = e->in && active == s.id && !s.remove;
    s.session->setClipboardActive(enabled && s.shareClipboard);
    if (!enabled) s.session->releaseAllKeys(); else requestClipboard(s);
    return FALSE;
}
void allocated(GtkWidget *w, GtkAllocation *a, gpointer data) {
    auto &s = *static_cast<Slot *>(data);
    if (!s.dynamicResize || a->width < 200 || a->height < 200) return;
    const int scale = gtk_widget_get_scale_factor(w);
    const auto size = velo::core::safeDesktopSize(a->width * scale, a->height * scale);
    s.desired = QSize(size.first, size.second); s.resizeAt = g_get_monotonic_time() + 300000;
}
void freePixels(guchar *pixels, gpointer) { g_free(pixels); }
void applyCursor(Slot &s, const QImage &image, QPoint hot, bool hidden) {
    GdkWindow *window = gtk_widget_get_window(s.area); if (!window) return;
    auto *display = gdk_window_get_display(window);
    GdkCursor *cursor = nullptr;
    if (hidden) cursor = gdk_cursor_new_for_display(display, GDK_BLANK_CURSOR);
    else if (!image.isNull()) {
        const auto rgba = image.convertToFormat(QImage::Format_RGBA8888);
        auto *bytes = static_cast<guchar *>(g_memdup2(rgba.constBits(), rgba.sizeInBytes()));
        auto *pixbuf = gdk_pixbuf_new_from_data(bytes, GDK_COLORSPACE_RGB, TRUE, 8,
            rgba.width(), rgba.height(), rgba.bytesPerLine(), freePixels, nullptr);
        if (pixbuf) { cursor = gdk_cursor_new_from_pixbuf(display, pixbuf,
            std::clamp(hot.x(), 0, rgba.width()-1), std::clamp(hot.y(), 0, rgba.height()-1)); g_object_unref(pixbuf); }
        else g_free(bytes);
    }
    gdk_window_set_cursor(window, cursor); if (cursor) g_object_unref(cursor);
}
gboolean tick(gpointer) {
    for (auto it = sessions.begin(); it != sessions.end();) {
        auto &s = *it->second;
        if (s.remove && s.session->isFinished()) { it = sessions.erase(it); continue; }
        if (auto frame = s.session->takeFrame()) {
            s.frame = frame->convertToFormat(QImage::Format_RGB32);
            if (active == s.id) gtk_widget_queue_draw(s.area);
        }
        if (!s.remove && active == s.id && s.dynamicResize && s.resizeAt &&
            g_get_monotonic_time() >= s.resizeAt && s.session->isConnected()) {
            if (s.desired != s.sentSize) { s.session->resizeDesktop(s.desired); s.sentSize = s.desired; }
            s.resizeAt = 0;
        }
        {
            QMutexLocker guard(&s.pendingLock);
            if (s.cursorPending && active == s.id) { applyCursor(s, s.cursor, s.hotSpot, s.cursorHidden); s.cursorPending = false; }
            if (s.clipboardPending) {
                if (s.shareClipboard && active == s.id && gtk_widget_has_focus(s.area)) {
                    const auto text = s.clipboard.toUtf8();
                    gtk_clipboard_set_text(gtk_clipboard_get(GDK_SELECTION_CLIPBOARD), text.constData(), text.size());
                }
                s.clipboard.clear(); s.clipboardPending = false;
            }
        }
        ++it;
    }
    return G_SOURCE_CONTINUE;
}
void create(const QJsonObject &o, const char *password, QJsonObject &result) {
    if (!box) throw std::runtime_error("Native desktop surface is not attached.");
    if (sessions.size() >= MaxSessions) throw std::runtime_error("Close a session before opening another (limit: 8).");
    QString why; auto profile = velo::Profile::fromJson(o["profile"].toObject(), why);
    if (!profile) throw std::runtime_error(why.toStdString());
    if (profile->allMonitors) throw std::runtime_error("All-monitor mode is not available in this migration preview. Choose Window.");
    const auto size = velo::core::safeDesktopSize(1600, 900);
    auto layout = velo::core::normalizeMonitors({{{0, 0, size.first, size.second}, true, 310, 174}});
    auto slot = std::make_unique<Slot>(); auto *s = slot.get(); s->id = nextId++;
    s->shareClipboard = profile->clipboard;
    s->session = std::make_unique<RdpSession>(*profile, QString::fromUtf8(password ? password : ""), layout);
    s->area = gtk_drawing_area_new(); g_object_ref_sink(s->area);
    gtk_widget_set_can_focus(s->area, TRUE); gtk_widget_set_hexpand(s->area, TRUE); gtk_widget_set_vexpand(s->area, TRUE);
    gtk_widget_add_events(s->area, GDK_POINTER_MOTION_MASK | GDK_BUTTON_PRESS_MASK | GDK_BUTTON_RELEASE_MASK |
        GDK_KEY_PRESS_MASK | GDK_KEY_RELEASE_MASK | GDK_SCROLL_MASK | GDK_SMOOTH_SCROLL_MASK | GDK_FOCUS_CHANGE_MASK);
    g_signal_connect(s->area, "draw", G_CALLBACK(draw), s);
    g_signal_connect(s->area, "motion-notify-event", G_CALLBACK(motion), s);
    g_signal_connect(s->area, "button-press-event", G_CALLBACK(button), s);
    g_signal_connect(s->area, "button-release-event", G_CALLBACK(button), s);
    g_signal_connect(s->area, "scroll-event", G_CALLBACK(scroll), s);
    g_signal_connect(s->area, "key-press-event", G_CALLBACK(key), s);
    g_signal_connect(s->area, "key-release-event", G_CALLBACK(key), s);
    g_signal_connect(s->area, "focus-in-event", G_CALLBACK(focus), s);
    g_signal_connect(s->area, "focus-out-event", G_CALLBACK(focus), s);
    g_signal_connect(s->area, "size-allocate", G_CALLBACK(allocated), s);
    const auto name = std::to_string(s->id); gtk_stack_add_named(GTK_STACK(stack), s->area, name.c_str());
    auto *q = s->session.get();
    // Direct connections only append protected messages/buffers; worker callbacks never touch GTK.
    QObject::connect(q, &RdpSession::stageChanged, q, [s](QString text) { push(s->id, "stage", {{"text", text}}); }, Qt::DirectConnection);
    QObject::connect(q, &RdpSession::connected, q, [s]() { push(s->id, "connected"); }, Qt::DirectConnection);
    QObject::connect(q, &RdpSession::ended, q, [s](QString text, QString detail, bool requested) {
        push(s->id, "ended", {{"text", text}, {"detail", detail}, {"requested", requested}});
    }, Qt::DirectConnection);
    QObject::connect(q, &RdpSession::warning, q, [s](QString text) { push(s->id, "warning", {{"text", text}}); }, Qt::DirectConnection);
    QObject::connect(q, &RdpSession::certificateRequested, q, [s](quint64 request, QString host, QString details, bool changed) {
        push(s->id, "certificate", {{"request", QString::number(request)}, {"host", host}, {"details", details}, {"changed", changed}});
    }, Qt::DirectConnection);
    QObject::connect(q, &RdpSession::cursorChanged, q, [s](QImage image, QPoint hot, bool hidden) {
        QMutexLocker guard(&s->pendingLock); s->cursor = image; s->hotSpot = hot; s->cursorHidden = hidden; s->cursorPending = true;
    }, Qt::DirectConnection);
    QObject::connect(q, &RdpSession::clipboardReceived, q, [s](QString text) {
        QMutexLocker guard(&s->pendingLock); s->clipboard = text; s->clipboardPending = true;
    }, Qt::DirectConnection);
    const auto id = s->id; sessions.emplace(id, std::move(slot));
    q->start(); result["session"] = QString::number(id);
}
} // namespace
extern "C" char *wr_attach(void *vertical_box, void *webview) {
    try {
        if (box) return encode(QJsonObject{{"ok", true}});
        if (!GTK_IS_BOX(vertical_box) || !GTK_IS_WIDGET(webview)) return error("Tauri did not supply a GTK box/webview.");
        static int argc = 1; static char appName[] = "winrdp-next"; static char *argv[] = {appName, nullptr};
        if (!QCoreApplication::instance()) qt = std::make_unique<QCoreApplication>(argc, argv);
        QCoreApplication::setOrganizationName("io.winrdp"); QCoreApplication::setApplicationName("WinRDPNext");
        if (gtk_widget_get_parent(GTK_WIDGET(webview)) != GTK_WIDGET(vertical_box))
            return error("Unsupported GTK webview layout: native attachment was not performed.");
        box = GTK_BOX(vertical_box); web = GTK_WIDGET(webview);
        stack = gtk_stack_new(); gtk_stack_set_transition_type(GTK_STACK(stack), GTK_STACK_TRANSITION_TYPE_NONE);
        gtk_box_pack_start(box, stack, TRUE, TRUE, 0); gtk_widget_set_no_show_all(stack, TRUE);
        gtk_widget_hide(stack); g_timeout_add(16, tick, nullptr);
        // Re-pin before GTK distributes the new allocation, so the surface never lags a frame.
        // Without this the surface still sizes correctly on connect, just not on resize.
        auto *top = gtk_widget_get_toplevel(GTK_WIDGET(box));
        if (top && GTK_IS_WINDOW(top))
            g_signal_connect(top, "configure-event", G_CALLBACK(configured), nullptr);
        else g_warning("winrdp: no toplevel window; the surface will not track resizes.");
        grip = gtk_drawing_area_new(); gtk_widget_set_size_request(grip, 18, 8);
        gtk_widget_set_halign(grip, GTK_ALIGN_END); gtk_widget_add_events(grip, GDK_BUTTON_PRESS_MASK);
        gtk_box_pack_end(box, grip, FALSE, FALSE, 0); gtk_widget_show(grip);
        g_signal_connect(grip, "draw", G_CALLBACK(+[](GtkWidget *, cairo_t *cr, gpointer) -> gboolean {
            cairo_set_source_rgb(cr, 0.50, 0.52, 0.56); cairo_set_line_width(cr, 1);
            for (int n = 0; n < 3; ++n) { cairo_move_to(cr, 16-4*n, 1); cairo_line_to(cr, 10-4*n, 7); }
            cairo_stroke(cr); return FALSE;
        }), nullptr);
        g_signal_connect(grip, "button-press-event", G_CALLBACK(+[](GtkWidget *w, GdkEventButton *e, gpointer) -> gboolean {
            if (e->button != 1) return FALSE;
            gtk_window_begin_resize_drag(GTK_WINDOW(gtk_widget_get_toplevel(w)), GDK_WINDOW_EDGE_SOUTH_EAST,
                e->button, static_cast<int>(e->x_root), static_cast<int>(e->y_root), e->time); return TRUE;
        }), nullptr);
        g_signal_connect(gtk_clipboard_get(GDK_SELECTION_CLIPBOARD), "owner-change",
            G_CALLBACK(+[](GtkClipboard *, GdkEventOwnerChange *, gpointer) {
                auto *s = lookup(active); if (s && gtk_widget_has_focus(s->area)) requestClipboard(*s);
            }), nullptr);
        return encode(QJsonObject{{"ok", true}, {"backend", RdpSession::backendVersion()}});
    } catch (const std::exception &e) { return error(QString::fromUtf8(e.what())); }
}
extern "C" char *wr_command(const char *operation, const char *json, const char *password) {
    try {
        QJsonParseError parse;
        const auto doc = QJsonDocument::fromJson(QByteArray(json ? json : "{}"), &parse);
        if (parse.error != QJsonParseError::NoError || !doc.isObject()) return error("Invalid native request.");
        const auto o = doc.object(); QJsonObject result{{"ok", true}};
        const std::string op = operation ? operation : "";
        if (op == "info") { result["attached"] = box != nullptr; result["backend"] = RdpSession::backendVersion(); }
        else if (op == "validate") {
            QString why; if (!velo::Profile::fromJson(o, why)) return error(why);
        } else if (op == "connect") create(o, password, result);
        else if (op == "select") select(get(o));
        else if (op == "library") { if (box) showLibrary(); }
        else if (op == "disconnect") {
            auto &s = get(o); s.session->stop(); s.remove = true;
            if (active == s.id) showLibrary();
        } else if (op == "certificate") {
            auto &s = get(o); bool valid = false; const auto request = o["request"].toString().toULongLong(&valid);
            const int decision = o["decision"].toInt(-1);
            if (!valid || decision < 0 || decision > 2) return error("Invalid certificate response.");
            s.session->answerCertificate(request, decision);
        } else if (op == "clipboard") {
            auto &s = get(o); s.shareClipboard = o["enabled"].toBool();
            s.session->setClipboardActive(s.shareClipboard && active == s.id && gtk_widget_has_focus(s.area));
            if (s.shareClipboard && active == s.id) requestClipboard(s);
        } else if (op == "release") get(o).session->releaseAllKeys();
        else if (op == "cad") {
            auto &s = get(o); s.session->releaseAllKeys();
            s.session->sendScanCode(0x1d, true); s.session->sendScanCode(0x38, true);
            s.session->sendScanCode(0x153, true); s.session->sendScanCode(0x153, false);
            s.session->sendScanCode(0x38, false); s.session->sendScanCode(0x1d, false);
        } else if (op == "stop-all") {
            deactivate(); for (auto &item : sessions) { item.second->session->stop(); item.second->remove = true; }
            if (box) showLibrary();
        } else if (op == "shutdown-ready") result["ready"] = sessions.empty();
        else return error("Unsupported native command.");
        return encode(result);
    } catch (const std::exception &e) { return error(QString::fromUtf8(e.what())); }
}
extern "C" char *wr_poll() {
    try { QJsonArray array; QMutexLocker guard(&eventLock);
        while (!events.empty()) { array.append(events.front()); events.pop_front(); }
        return encode(array);
    } catch (const std::exception &e) { return error(QString::fromUtf8(e.what())); }
}
extern "C" void wr_free(char *value) { g_free(value); }
