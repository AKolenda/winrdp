// SPDX-License-Identifier: AGPL-3.0-only
#include "rdp_session.hpp"
#include <QByteArray>
#include <QCryptographicHash>
#include <QSslCertificate>
#include <QStringList>
#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <mutex>
#include <new>
#include <set>
#include <utility>
#include <vector>

#include <freerdp/channels/channels.h>
#include <freerdp/channels/cliprdr.h>
#include <freerdp/channels/disp.h>
#include <freerdp/channels/rdpgfx.h>
#include <freerdp/client.h>
#include <freerdp/client/cliprdr.h>
#include <freerdp/client/cmdline.h>
#include <freerdp/client/disp.h>
#include <freerdp/client/rdpgfx.h>
#include <freerdp/codec/color.h>
#include <freerdp/freerdp.h>
#include <freerdp/gdi/gdi.h>
#include <freerdp/gdi/gfx.h>
#include <freerdp/graphics.h>
#include <freerdp/input.h>
#include <freerdp/settings.h>
#include <winpr/crt.h>
#include <winpr/synch.h>
#include <winpr/wtypes.h>

namespace velo {
namespace {
constexpr UINT32 UnicodeTextFormat = 13; // CF_UNICODETEXT
constexpr std::size_t MaximumQueuedInput = 4096;
void wipe(QByteArray &data) {
    // Best effort only: Qt/FreeRDP/SSPI can have their own copies. Not locked memory.
    volatile char *p = data.data();
    for (qsizetype i = 0; i < data.size(); ++i)
        p[i] = 0;
    data.clear();
}
QString safeText(const char *p) {
    return p ? QString::fromUtf8(p).left(8192) : QString{};
}
QString fingerprint(const char *data, DWORD flags) {
    if (!data)
        return {};
    if (flags & VERIFY_CERT_FLAG_FP_IS_PEM) {
        const auto certs = QSslCertificate::fromData(QByteArray(data), QSsl::Pem);
        if (!certs.isEmpty())
            return QString::fromLatin1(certs.front().digest(QCryptographicHash::Sha256).toHex(':'))
                .toUpper();
        return "Could not decode certificate fingerprint. Do not trust it.";
    }
    return safeText(data);
}
} // namespace
struct RdpSession::Impl {
    struct Context {
        rdpClientContext common;
        Impl *self;
    };
    struct Pointer {
        rdpPointer base;
        QImage *image;
    };
    enum class CommandType { Key, Unicode, Mouse, Release, Clipboard, ClipboardFocus, Resize };
    struct Command {
        CommandType type = CommandType::Release;
        quint32 code = 0;
        quint16 flags = 0;
        int x = 0, y = 0;
        bool down = false, extended = false;
        QString text;
    };
    RdpSession *q;
    Profile profile;
    QByteArray password;
    core::Layout layout;
    core::LatestMailbox<QImage> frames;
    std::atomic<bool> stopping{false}, connected{false};
    std::mutex lifetimeMutex, queueMutex, certificateMutex;
    std::recursive_mutex channelMutex, renderMutex;
    std::condition_variable certificateCondition;
    std::deque<Command> commands;
    rdpContext *context = nullptr;
    HANDLE wake = nullptr;
    CliprdrClientContext *clip = nullptr;
    DispClientContext *disp = nullptr;
    RdpgfxClientContext *gfx = nullptr;
    bool subscribed = false, gdiReady = false, gfxReady = false;
    bool displayCaps = false, layoutDirty = false;
    UINT32 maxMonitors = 1;
    std::vector<core::Monitor> desiredMonitors;
    std::set<quint32> pressed;
    std::set<quint16> mousePressed, extendedMousePressed;
    int mouseX = 0, mouseY = 0;
    bool clipReady = false, clipboardActive = false, remoteHasText = false, requestPending = false;
    quint64 clipboardGeneration = 0, requestedGeneration = 0;
    QString localClipboard;
    quint64 certificateId = 0;
    int certificateDecision = -1;
    QString failure;

    Impl(RdpSession *owner, Profile p, QString secret, core::Layout monitorLayout)
        : q(owner), profile(std::move(p)), password(secret.toUtf8()),
          layout(std::move(monitorLayout)) {
        secret.fill(QChar(0));
        secret.clear();
        wake = CreateEvent(nullptr, TRUE, FALSE, nullptr);
        desiredMonitors = layout.monitors;
    }
    ~Impl() {
        wipe(password);
        if (wake)
            CloseHandle(wake);
    }
    static Impl *self(rdpContext *c) {
        return reinterpret_cast<Context *>(c)->self;
    }
    void enqueue(Command cmd) {
        if (stopping.load())
            return;
        {
            std::lock_guard<std::mutex> lock(queueMutex);
            // Coalesce only adjacent replaceable events. Never reorder keys/buttons.
            if (!commands.empty()) {
                auto &previous = commands.back();
                const bool moves = cmd.type == CommandType::Mouse && cmd.flags == PTR_FLAGS_MOVE &&
                                   previous.type == cmd.type && previous.flags == PTR_FLAGS_MOVE;
                if (moves || (cmd.type == CommandType::Resize && previous.type == cmd.type) ||
                    (cmd.type == CommandType::Clipboard && previous.type == cmd.type)) {
                    previous = std::move(cmd);
                    if (wake)
                        SetEvent(wake);
                    return;
                }
            }
            if (commands.size() >= MaximumQueuedInput) {
                commands.clear();
                commands.push_back(Command{});
                emit q->warning("Input briefly fell behind. Held keys and buttons were released.");
            }
            commands.push_back(std::move(cmd));
        }
        if (wake)
            SetEvent(wake);
    }
    static BOOL clientNew(freerdp *instance, rdpContext *) {
        instance->PreConnect = preConnect;
        instance->PostConnect = postConnect;
        instance->PostDisconnect = postDisconnect;
        instance->PostFinalDisconnect = postFinalDisconnect;
        instance->AuthenticateEx = authenticate;
        instance->VerifyCertificateEx = verifyCertificate;
        instance->VerifyChangedCertificateEx = verifyChangedCertificate;
        // Unsupported interactive enterprise flows must not fall back to stdin.
        instance->GetAccessToken = nullptr;
        instance->ChooseSmartcard = nullptr;
        return TRUE;
    }
    static BOOL authenticate(freerdp *f, char **username, char **secret, char **domain,
                             rdp_auth_reason reason) {
        auto *d = self(f->context);
        if (d->stopping.load() || reason != AUTH_NLA || !username || !secret || !domain)
            return FALSE;
        emit d->q->stageChanged("Authenticating");
        // Settings already own the account and secret; do not read terminal input.
        return *username && **username && *secret && **secret;
    }
    int askCertificate(const char *host, UINT16 port, const char *subject, const char *issuer,
                       const char *next, const char *old, DWORD flags, bool changed) {
        if (stopping.load())
            return 0;
        const auto nextFingerprint = fingerprint(next, flags);
        if (nextFingerprint.startsWith("Could not decode"))
            return 0;
        QString details = "Destination: " + safeText(host) + ":" + QString::number(port) +
                          "\n\nSubject: " + safeText(subject) + "\nIssuer: " + safeText(issuer) +
                          "\n\nPresented fingerprint:\n" + nextFingerprint;
        if (old)
            details += "\n\nPreviously remembered fingerprint:\n" + fingerprint(old, flags);
        if (flags & VERIFY_CERT_FLAG_MISMATCH)
            details += "\n\nThe certificate name does not match the destination.";
        std::unique_lock<std::mutex> lock(certificateMutex);
        const quint64 id = ++certificateId;
        certificateDecision = -1;
        emit q->stageChanged("Check this computer's identity");
        emit q->certificateRequested(id, safeText(host), details, changed);
        // No blocking UI invocation: cancellation can always wake this wait.
        certificateCondition.wait(lock,
                                  [&] { return stopping.load() || certificateDecision >= 0; });
        return stopping.load() ? 0 : certificateDecision;
    }
    static DWORD verifyCertificate(freerdp *f, const char *host, UINT16 port, const char *,
                                   const char *subject, const char *issuer, const char *fp,
                                   DWORD flags) {
        return self(f->context)
            ->askCertificate(host, port, subject, issuer, fp, nullptr, flags, false);
    }
    static DWORD verifyChangedCertificate(freerdp *f, const char *host, UINT16 port, const char *,
                                          const char *subject, const char *issuer, const char *next,
                                          const char *, const char *, const char *old,
                                          DWORD flags) {
        return self(f->context)
            ->askCertificate(host, port, subject, issuer, next, old, flags, true);
    }
    bool configure(rdpSettings *s) {
        const auto parsed = core::parseEndpoint(profile.address.toStdString());
        if (!parsed.value) {
            failure = QString::fromStdString(parsed.error);
            return false;
        }
        // Parse ONLY internal constant options. This is not a subprocess, and neither
        // credentials nor user-provided text are ever inserted into process argv.
        // Client-side tuning only: none of these change any setting on the remote computer.
        // WINRDP_NETWORK picks the server's adaptive profile; "auto" enables RDP network
        // autodetection so the host adapts quality to measured bandwidth and round-trip time.
        QString network = qEnvironmentVariable("WINRDP_NETWORK", "lan");
        if (network != "lan" && network != "auto" && network != "broadband" &&
            network != "wan" && network != "modem")
            network = "lan";
        std::vector<QByteArray> options = {"velordp", "/v:localhost",        "/bpp:32",
                                           "/gdi:sw", ("/network:" + network).toLatin1(), "/sec:nla",
                                           "+dynamic-resolution", "/timeout:15000"};
        // Font smoothing (ClearType) costs bandwidth; WINRDP_FONTS=0 trades it for latency.
        if (qEnvironmentVariable("WINRDP_FONTS", "1") != "0")
            options.emplace_back("+fonts");
        if (profile.clipboard)
            options.emplace_back("/clipboard");
        if (profile.audio)
            options.emplace_back("/sound");
        if (!profile.compatibility) {
            // The saved profile has no interface control yet, so WINRDP_GFX overrides it.
            // H.264 usually wins on video and scrolling; plain /gfx can be sharper on text.
            const QString graphics = qEnvironmentVariable("WINRDP_GFX", profile.graphics);
            if (graphics == "avc420") options.emplace_back("/gfx:AVC420");
            else if (graphics == "avc444") options.emplace_back("/gfx:AVC444");
            else options.emplace_back("/gfx");
        }
        std::vector<char *> argv;
        for (auto &option : options)
            argv.push_back(option.data());
        if (freerdp_client_settings_parse_command_line(s, int(argv.size()), argv.data(), FALSE) <
            0) {
            failure = "The installed FreeRDP library rejected the session options.";
            return false;
        }
        auto setString = [&](FreeRDP_Settings_Keys_String key, const QByteArray &value) {
            return freerdp_settings_set_string(s, key, value.constData()) != FALSE;
        };
        QString user = profile.username, domain;
        const auto slash = user.indexOf('\\');
        if (slash >= 0) {
            domain = user.left(slash);
            user = user.mid(slash + 1);
        }
        if (user.isEmpty() || password.isEmpty()) {
            failure = "A Windows username and account password are required.";
            return false;
        }
        bool ok =
            setString(FreeRDP_ServerHostname, QByteArray::fromStdString(parsed.value->host)) &&
            setString(FreeRDP_UserSpecifiedServerName,
                      QByteArray::fromStdString(parsed.value->host)) &&
            freerdp_settings_set_uint32(s, FreeRDP_ServerPort, parsed.value->port) &&
            setString(FreeRDP_Username, user.toUtf8()) &&
            setString(FreeRDP_Domain, domain.toUtf8()) && setString(FreeRDP_Password, password) &&
            freerdp_settings_set_uint32(s, FreeRDP_KeyboardLayout, profile.keyboardLayout) &&
            freerdp_settings_set_bool(s, FreeRDP_IgnoreCertificate, FALSE) &&
            freerdp_settings_set_bool(s, FreeRDP_AutoAcceptCertificate, FALSE) &&
            freerdp_settings_set_bool(s, FreeRDP_CertificateCallbackPreferPEM, TRUE) &&
            freerdp_settings_set_bool(s, FreeRDP_UseCommonStdioCallbacks, FALSE) &&
            freerdp_settings_set_bool(s, FreeRDP_AsyncUpdate, FALSE) &&
            freerdp_settings_set_bool(s, FreeRDP_SoftwareGdi, TRUE);
        if (!ok) {
            failure = "Could not initialize the FreeRDP session settings.";
            return false;
        }
        if (!layout) {
            failure = QString::fromStdString(layout.error);
            return false;
        }
        std::vector<rdpMonitor> monitors;
        for (std::size_t i = 0; i < layout.monitors.size(); ++i) {
            const auto &source = layout.monitors[i];
            rdpMonitor m{};
            m.x = source.rect.x;
            m.y = source.rect.y;
            m.width = source.rect.width;
            m.height = source.rect.height;
            m.is_primary = source.primary;
            m.orig_screen = UINT32(i);
            m.attributes.physicalWidth =
                std::clamp(source.physicalWidth > 0 ? source.physicalWidth : 310, 10, 10000);
            m.attributes.physicalHeight =
                std::clamp(source.physicalHeight > 0 ? source.physicalHeight : 174, 10, 10000);
            m.attributes.desktopScaleFactor = 100;
            m.attributes.deviceScaleFactor = 100;
            monitors.push_back(m);
        }
        ok = freerdp_settings_set_monitor_def_array_sorted(s, monitors.data(), monitors.size()) &&
             freerdp_settings_set_bool(s, FreeRDP_UseMultimon, monitors.size() > 1) &&
             freerdp_settings_set_bool(s, FreeRDP_Fullscreen,
                                       profile.fullscreen || monitors.size() > 1) &&
             freerdp_settings_set_uint32(s, FreeRDP_DesktopWidth, UINT32(layout.bounds.width)) &&
             freerdp_settings_set_uint32(s, FreeRDP_DesktopHeight, UINT32(layout.bounds.height));
        if (!ok)
            failure = "FreeRDP could not apply this monitor layout.";
        return ok;
    }
    static BOOL preConnect(freerdp *f) {
        auto *d = self(f->context);
        if (d->stopping.load())
            return FALSE;
        if (PubSub_SubscribeChannelConnected(f->context->pubSub, channelConnected) < 0)
            return FALSE;
        if (PubSub_SubscribeChannelDisconnected(f->context->pubSub, channelDisconnected) < 0) {
            PubSub_UnsubscribeChannelConnected(f->context->pubSub, channelConnected);
            return FALSE;
        }
        d->subscribed = true;
        return TRUE; // Common client LoadChannels loads the configured add-ins.
    }
    static BOOL beginPaint(rdpContext *c) {
        auto *d = self(c);
        d->renderMutex.lock();
        if (c->gdi && c->gdi->primary && c->gdi->primary->hdc && c->gdi->primary->hdc->hwnd)
            c->gdi->primary->hdc->hwnd->invalid->null = TRUE;
        return TRUE;
    }
    bool copyFrame(rdpContext *c) {
        auto *g = c->gdi;
        if (!g || !g->primary_buffer || g->width < 1 || g->height < 1)
            return true;
        if (g->width > 16384 || g->height > 16384 ||
            static_cast<quint64>(g->width) * g->height > 64ULL * 1024 * 1024)
            return false;
        if (g->stride < static_cast<quint64>(g->width) * 4)
            return false;
        QImage borrowed(g->primary_buffer, g->width, g->height, g->stride, QImage::Format_RGBX8888);
        auto owned = borrowed.copy();
        if (owned.isNull())
            return false;
        frames.put(std::move(owned));
        return true;
    }
    static BOOL endPaint(rdpContext *c) {
        auto *d = self(c);
        bool ok = true;
        auto *g = c->gdi;
        if (g && g->primary && g->primary->hdc && g->primary->hdc->hwnd &&
            !g->primary->hdc->hwnd->invalid->null)
            ok = d->copyFrame(c);
        d->renderMutex.unlock();
        return ok ? TRUE : FALSE;
    }
    static BOOL desktopResize(rdpContext *c) {
        auto *d = self(c);
        std::lock_guard<std::recursive_mutex> guard(d->renderMutex);
        const auto width = freerdp_settings_get_uint32(c->settings, FreeRDP_DesktopWidth),
                   height = freerdp_settings_get_uint32(c->settings, FreeRDP_DesktopHeight);
        if (!width || !height || width > 16384 || height > 16384 ||
            quint64(width) * height > 64ULL * 1024 * 1024)
            return FALSE;
        if (!c->gdi || !gdi_resize(c->gdi, width, height))
            return FALSE;
        emit d->q->desktopSizeChanged(QSize(int(width), int(height)));
        return TRUE;
    }
    static BOOL pointerNew(rdpContext *c, rdpPointer *base) {
        if (!base || base->width > 384 || base->height > 384 || !base->width || !base->height)
            return FALSE;
        auto *ptr = reinterpret_cast<Pointer *>(base);
        ptr->image =
            new (std::nothrow) QImage(int(base->width), int(base->height), QImage::Format_RGBA8888);
        if (!ptr->image || ptr->image->isNull()) {
            delete ptr->image;
            ptr->image = nullptr;
            return FALSE;
        }
        ptr->image->fill(Qt::transparent);
        const BOOL ok = freerdp_image_copy_from_pointer_data(
            ptr->image->bits(), PIXEL_FORMAT_RGBA32, UINT32(ptr->image->bytesPerLine()), 0, 0,
            base->width, base->height, base->xorMaskData, base->lengthXorMask, base->andMaskData,
            base->lengthAndMask, base->xorBpp, &c->gdi->palette);
        if (!ok) {
            delete ptr->image;
            ptr->image = nullptr;
        }
        return ok;
    }
    static void pointerFree(rdpContext *, rdpPointer *base) {
        auto *p = reinterpret_cast<Pointer *>(base);
        delete p->image;
        p->image = nullptr;
    }
    static BOOL pointerSet(rdpContext *c, rdpPointer *base) {
        auto *p = reinterpret_cast<Pointer *>(base);
        if (!p->image)
            return FALSE;
        emit self(c)->q->cursorChanged(*p->image, QPoint(int(base->xPos), int(base->yPos)), false);
        return TRUE;
    }
    static BOOL pointerNull(rdpContext *c) {
        emit self(c)->q->cursorChanged({}, QPoint(), true);
        return TRUE;
    }
    static BOOL pointerDefault(rdpContext *c) {
        emit self(c)->q->cursorChanged({}, QPoint(), false);
        return TRUE;
    }
    static BOOL postConnect(freerdp *f) {
        auto *d = self(f->context);
        std::lock_guard<std::recursive_mutex> channels(d->channelMutex);
        const auto w = freerdp_settings_get_uint32(f->context->settings, FreeRDP_DesktopWidth),
                   h = freerdp_settings_get_uint32(f->context->settings, FreeRDP_DesktopHeight);
        if (!w || !h || w > 16384 || h > 16384 || quint64(w) * h > 64ULL * 1024 * 1024)
            return FALSE;
        if (!gdi_init(f, PIXEL_FORMAT_RGBX32))
            return FALSE;
        d->gdiReady = true;
        f->context->update->BeginPaint = beginPaint;
        f->context->update->EndPaint = endPaint;
        f->context->update->DesktopResize = desktopResize;
        rdpPointer pointer{};
        pointer.size = sizeof(Pointer);
        pointer.New = pointerNew;
        pointer.Free = pointerFree;
        pointer.Set = pointerSet;
        pointer.SetNull = pointerNull;
        pointer.SetDefault = pointerDefault;
        graphics_register_pointer(f->context->graphics, &pointer);
        if (d->gfx && !d->gfxReady) {
            if (!gdi_graphics_pipeline_init(f->context->gdi, d->gfx))
                return FALSE;
            d->gfxReady = true;
        }
        emit d->q->desktopSizeChanged(QSize(int(w), int(h)));
        return TRUE;
    }
    static void postDisconnect(freerdp *f) {
        self(f->context)->connected.store(false);
    }
    static void postFinalDisconnect(freerdp *f) {
        self(f->context)->finalCleanup(f);
    }
    void finalCleanup(freerdp *f) {
        // Called after channel threads are stopped. Never free a framebuffer while
        // a channel or GUI painter may still be using the library's storage.
        if (subscribed) {
            PubSub_UnsubscribeChannelConnected(f->context->pubSub, channelConnected);
            PubSub_UnsubscribeChannelDisconnected(f->context->pubSub, channelDisconnected);
            subscribed = false;
        }
        std::lock_guard<std::recursive_mutex> guard(channelMutex);
        if (gfxReady && gfx && f->context->gdi)
            gdi_graphics_pipeline_uninit(f->context->gdi, gfx);
        gfxReady = false;
        gfx = nullptr;
        clip = nullptr;
        disp = nullptr;
        displayCaps = false;
        clipReady = false;
        if (gdiReady) {
            gdi_free(f);
            gdiReady = false;
        }
    }
    static void channelConnected(void *context, const ChannelConnectedEventArgs *event) {
        auto *c = static_cast<rdpContext *>(context);
        auto *d = self(c);
        std::lock_guard<std::recursive_mutex> guard(d->channelMutex);
        if (std::strcmp(event->name, CLIPRDR_SVC_CHANNEL_NAME) == 0) {
            d->clip = static_cast<CliprdrClientContext *>(event->pInterface);
            if (!d->clip)
                return;
            auto *clip = d->clip;
            clip->custom = d;
            clip->MonitorReady = clipboardReady;
            clip->ServerCapabilities = clipboardCapabilities;
            clip->ServerFormatList = clipboardFormats;
            clip->ServerFormatListResponse = clipboardListResponse;
            clip->ServerFormatDataRequest = clipboardDataRequest;
            clip->ServerFormatDataResponse = clipboardDataResponse;
        } else if (std::strcmp(event->name, DISP_DVC_CHANNEL_NAME) == 0) {
            d->disp = static_cast<DispClientContext *>(event->pInterface);
            if (!d->disp)
                return;
            d->disp->custom = d;
            d->disp->DisplayControlCaps = displayCapabilities;
        } else if (std::strcmp(event->name, RDPGFX_DVC_CHANNEL_NAME) == 0) {
            d->gfx = static_cast<RdpgfxClientContext *>(event->pInterface);
            if (d->gfx && d->gdiReady) {
                d->gfxReady = gdi_graphics_pipeline_init(c->gdi, d->gfx);
                if (!d->gfxReady)
                    emit d->q->warning(
                        "Could not initialize the graphics pipeline. Try Compatibility graphics.");
            }
            emit d->q->featureChanged("Graphics",
                                      d->gfxReady ? "RDP graphics / software" : "Negotiating");
        } else if (std::strcmp(event->name, "rdpsnd") == 0) {
            // The FreeRDP rdpsnd plug-in owns decoding and its Linux audio device.
            emit d->q->featureChanged("Audio", "Channel open");
        }
    }
    static void channelDisconnected(void *context, const ChannelDisconnectedEventArgs *event) {
        auto *c = static_cast<rdpContext *>(context);
        auto *d = self(c);
        std::lock_guard<std::recursive_mutex> guard(d->channelMutex);
        if (std::strcmp(event->name, CLIPRDR_SVC_CHANNEL_NAME) == 0) {
            if (d->clip)
                d->clip->custom = nullptr;
            d->clip = nullptr;
            d->clipReady = false;
            d->requestPending = false;
            emit d->q->featureChanged("Clipboard", "Closed");
        } else if (std::strcmp(event->name, DISP_DVC_CHANNEL_NAME) == 0) {
            if (d->disp)
                d->disp->custom = nullptr;
            d->disp = nullptr;
            d->displayCaps = false;
            emit d->q->featureChanged("Resize", "Closed");
        } else if (std::strcmp(event->name, RDPGFX_DVC_CHANNEL_NAME) == 0) {
            if (d->gfxReady && d->gfx && c->gdi)
                gdi_graphics_pipeline_uninit(c->gdi, d->gfx);
            d->gfxReady = false;
            d->gfx = nullptr;
        }
    }
    static UINT displayCapabilities(DispClientContext *context, UINT32 count, UINT32, UINT32) {
        auto *d = static_cast<Impl *>(context->custom);
        if (!d)
            return CHANNEL_RC_OK;
        std::lock_guard<std::recursive_mutex> guard(d->channelMutex);
        d->displayCaps = true;
        d->maxMonitors = count;
        d->layoutDirty = true;
        emit d->q->featureChanged("Resize", "Ready");
        if (d->wake)
            SetEvent(d->wake);
        return CHANNEL_RC_OK;
    }
    void applyLayout() {
        std::lock_guard<std::recursive_mutex> guard(channelMutex);
        if (!layoutDirty || !disp || !displayCaps || !connected.load())
            return;
        layoutDirty = false;
        if (desiredMonitors.empty() || desiredMonitors.size() > maxMonitors) {
            emit q->warning("The remote host does not accept this number of monitors.");
            return;
        }
        std::vector<DISPLAY_CONTROL_MONITOR_LAYOUT> out;
        for (const auto &m : desiredMonitors) {
            DISPLAY_CONTROL_MONITOR_LAYOUT item{};
            item.Flags = m.primary ? DISPLAY_CONTROL_MONITOR_PRIMARY : 0;
            item.Left = m.rect.x;
            item.Top = m.rect.y;
            item.Width = m.rect.width;
            item.Height = m.rect.height;
            item.PhysicalWidth = std::clamp(m.physicalWidth > 0 ? m.physicalWidth : 310, 10, 10000);
            item.PhysicalHeight =
                std::clamp(m.physicalHeight > 0 ? m.physicalHeight : 174, 10, 10000);
            item.Orientation = 0;
            item.DesktopScaleFactor = 100;
            item.DeviceScaleFactor = 100;
            out.push_back(item);
        }
        const UINT rc = disp->SendMonitorLayout(disp, UINT32(out.size()), out.data());
        if (rc != CHANNEL_RC_OK)
            emit q->warning("The remote host rejected the display resize. The existing desktop "
                            "will be scaled to fit.");
    }
    static UINT clipboardCapabilities(CliprdrClientContext *, const CLIPRDR_CAPABILITIES *) {
        return CHANNEL_RC_OK;
    }
    static UINT clipboardListResponse(CliprdrClientContext *,
                                      const CLIPRDR_FORMAT_LIST_RESPONSE *) {
        return CHANNEL_RC_OK;
    }
    UINT advertiseClipboard() {
        if (!clip || !clipReady)
            return CHANNEL_RC_OK;
        CLIPRDR_FORMAT format{};
        format.formatId = UnicodeTextFormat;
        CLIPRDR_FORMAT_LIST list{};
        // Advertise no formats when unfocused: the remote cannot pull a stale secret.
        list.numFormats = clipboardActive && !localClipboard.isNull() ? 1 : 0;
        list.formats = list.numFormats ? &format : nullptr;
        return clip->ClientFormatList(clip, &list);
    }
    static UINT clipboardReady(CliprdrClientContext *context, const CLIPRDR_MONITOR_READY *) {
        auto *d = static_cast<Impl *>(context->custom);
        if (!d)
            return CHANNEL_RC_OK;
        std::lock_guard<std::recursive_mutex> guard(d->channelMutex);
        CLIPRDR_GENERAL_CAPABILITY_SET general{};
        general.capabilitySetType = CB_CAPSTYPE_GENERAL;
        general.capabilitySetLength = 12;
        general.version = CB_CAPS_VERSION_2;
        general.generalFlags = CB_USE_LONG_FORMAT_NAMES;
        CLIPRDR_CAPABILITIES caps{};
        caps.cCapabilitiesSets = 1;
        caps.capabilitySets = reinterpret_cast<CLIPRDR_CAPABILITY_SET *>(&general);
        const UINT rc = context->ClientCapabilities(context, &caps);
        if (rc != CHANNEL_RC_OK)
            return rc;
        d->clipReady = true;
        emit d->q->featureChanged("Clipboard", "Text ready");
        return d->advertiseClipboard();
    }
    UINT requestRemoteClipboard() {
        if (!clip || !clipboardActive || !remoteHasText || requestPending)
            return CHANNEL_RC_OK;
        requestPending = true;
        requestedGeneration = clipboardGeneration;
        CLIPRDR_FORMAT_DATA_REQUEST request{};
        request.requestedFormatId = UnicodeTextFormat;
        const auto rc = clip->ClientFormatDataRequest(clip, &request);
        if (rc != CHANNEL_RC_OK)
            requestPending = false;
        return rc;
    }
    static UINT clipboardFormats(CliprdrClientContext *context, const CLIPRDR_FORMAT_LIST *list) {
        auto *d = static_cast<Impl *>(context->custom);
        if (!d)
            return CHANNEL_RC_OK;
        std::lock_guard<std::recursive_mutex> guard(d->channelMutex);
        d->remoteHasText = false;
        ++d->clipboardGeneration;
        if (list->numFormats > 65536 || (list->numFormats && !list->formats))
            return ERROR_INVALID_DATA;
        for (UINT32 i = 0; i < list->numFormats; ++i)
            if (list->formats[i].formatId == UnicodeTextFormat)
                d->remoteHasText = true;
        CLIPRDR_FORMAT_LIST_RESPONSE response{};
        response.common.msgFlags = CB_RESPONSE_OK;
        const auto rc = context->ClientFormatListResponse(context, &response);
        if (rc != CHANNEL_RC_OK)
            return rc;
        return d->requestRemoteClipboard();
    }
    static UINT clipboardDataRequest(CliprdrClientContext *context,
                                     const CLIPRDR_FORMAT_DATA_REQUEST *request) {
        auto *d = static_cast<Impl *>(context->custom);
        if (!d)
            return CHANNEL_RC_OK;
        std::lock_guard<std::recursive_mutex> guard(d->channelMutex);
        CLIPRDR_FORMAT_DATA_RESPONSE response{};
        response.common.msgFlags = CB_RESPONSE_FAIL;
        std::vector<std::uint8_t> bytes;
        if (request->requestedFormatId == UnicodeTextFormat && d->clipboardActive &&
            !d->localClipboard.isNull())
            bytes = core::encodeClipboard(d->localClipboard.toStdU16String());
        if (!bytes.empty()) {
            response.common.msgFlags = CB_RESPONSE_OK;
            response.common.dataLen = UINT32(bytes.size());
            response.requestedFormatData = bytes.data();
        }
        return context->ClientFormatDataResponse(context, &response);
    }
    static UINT clipboardDataResponse(CliprdrClientContext *context,
                                      const CLIPRDR_FORMAT_DATA_RESPONSE *response) {
        auto *d = static_cast<Impl *>(context->custom);
        if (!d)
            return CHANNEL_RC_OK;
        std::lock_guard<std::recursive_mutex> guard(d->channelMutex);
        if (!d->requestPending)
            return CHANNEL_RC_OK;
        const auto requested = d->requestedGeneration;
        d->requestPending = false;
        if (requested == d->clipboardGeneration) {
            d->remoteHasText = false;
            if ((response->common.msgFlags & CB_RESPONSE_OK) && d->clipboardActive) {
                const auto text =
                    core::decodeClipboard(response->requestedFormatData, response->common.dataLen);
                if (text)
                    emit d->q->clipboardReceived(QString::fromStdU16String(*text));
            }
        } else
            return d->requestRemoteClipboard();
        return CHANNEL_RC_OK;
    }
    void releaseInput() {
        if (!context || !connected.load())
            return;
        for (const auto code : pressed)
            freerdp_input_send_keyboard_event_ex(context->input, FALSE, FALSE, code);
        pressed.clear();
        for (const auto button : mousePressed)
            freerdp_input_send_mouse_event(context->input, button, UINT16(mouseX), UINT16(mouseY));
        mousePressed.clear();
        for (const auto button : extendedMousePressed)
            freerdp_input_send_extended_mouse_event(context->input, button, UINT16(mouseX),
                                                    UINT16(mouseY));
        extendedMousePressed.clear();
    }
    bool processCommands() {
        std::deque<Command> pending;
        {
            std::lock_guard<std::mutex> lock(queueMutex);
            pending.swap(commands);
            if (wake)
                ResetEvent(wake);
        }
        bool ok = true;
        for (const auto &cmd : pending) {
            if (stopping.load())
                break;
            switch (cmd.type) {
            case CommandType::Key:
                if (connected.load()) {
                    ok = freerdp_input_send_keyboard_event_ex(
                             context->input, cmd.down, pressed.count(cmd.code) > 0, cmd.code) &&
                         ok;
                    if (cmd.down)
                        pressed.insert(cmd.code);
                    else
                        pressed.erase(cmd.code);
                }
                break;
            case CommandType::Unicode:
                if (connected.load())
                    for (const auto unit : cmd.text) {
                        ok = freerdp_input_send_unicode_keyboard_event(context->input, 0,
                                                                       unit.unicode()) &&
                             ok;
                        ok = freerdp_input_send_unicode_keyboard_event(
                                 context->input, KBD_FLAGS_RELEASE, unit.unicode()) &&
                             ok;
                    }
                break;
            case CommandType::Mouse:
                if (connected.load()) {
                    mouseX = std::clamp(cmd.x, 0, 65535);
                    mouseY = std::clamp(cmd.y, 0, 65535);
                    if (cmd.extended) {
                        const auto button =
                            quint16(cmd.flags & (PTR_XFLAGS_BUTTON1 | PTR_XFLAGS_BUTTON2));
                        if (button) {
                            if (cmd.flags & PTR_XFLAGS_DOWN)
                                extendedMousePressed.insert(button);
                            else
                                extendedMousePressed.erase(button);
                        }
                        ok = freerdp_input_send_extended_mouse_event(
                                 context->input, cmd.flags, UINT16(mouseX), UINT16(mouseY)) &&
                             ok;
                    } else {
                        const auto button =
                            quint16(cmd.flags &
                                    (PTR_FLAGS_BUTTON1 | PTR_FLAGS_BUTTON2 | PTR_FLAGS_BUTTON3));
                        if (button) {
                            if (cmd.flags & PTR_FLAGS_DOWN)
                                mousePressed.insert(button);
                            else
                                mousePressed.erase(button);
                        }
                        ok = freerdp_input_send_mouse_event(context->input, cmd.flags,
                                                            UINT16(mouseX), UINT16(mouseY)) &&
                             ok;
                    }
                }
                break;
            case CommandType::Release:
                releaseInput();
                break;
            case CommandType::Clipboard: {
                std::lock_guard<std::recursive_mutex> guard(channelMutex);
                localClipboard = cmd.text;
                ++clipboardGeneration;
                remoteHasText = false;
                if (advertiseClipboard() != CHANNEL_RC_OK)
                    emit q->warning("Could not synchronize the local text clipboard.");
                break;
            }
            case CommandType::ClipboardFocus: {
                std::lock_guard<std::recursive_mutex> guard(channelMutex);
                clipboardActive = cmd.down;
                if (clipboardActive && remoteHasText)
                    requestRemoteClipboard();
                else
                    advertiseClipboard();
                break;
            }
            case CommandType::Resize: {
                std::lock_guard<std::recursive_mutex> guard(channelMutex);
                if (!profile.allMonitors) {
                    const auto size = core::safeDesktopSize(cmd.x, cmd.y);
                    desiredMonitors = {{{0, 0, size.first, size.second}, true, 310, 174}};
                    layoutDirty = true;
                }
                break;
            }
            }
        }
        return ok;
    }
};

RdpSession::RdpSession(Profile p, QString secret, core::Layout layout, QObject *parent)
    : QThread(parent),
      d_(std::make_unique<Impl>(this, std::move(p), std::move(secret), std::move(layout))) {}
RdpSession::~RdpSession() {
    stop();
    wait();
}
bool RdpSession::isConnected() const {
    return d_->connected.load();
}
QString RdpSession::backendVersion() {
    return safeText(freerdp_get_version_string());
}
QString RdpSession::backendBuild() {
    return safeText(freerdp_get_build_config());
}
std::optional<QImage> RdpSession::takeFrame() {
    return d_->frames.take();
}
void RdpSession::stop() {
    d_->stopping.store(true);
    d_->certificateCondition.notify_all();
    if (d_->wake)
        SetEvent(d_->wake);
    std::lock_guard<std::mutex> lock(d_->lifetimeMutex);
    if (d_->context)
        freerdp_abort_connect_context(d_->context);
}
void RdpSession::answerCertificate(quint64 id, int decision) {
    std::lock_guard<std::mutex> lock(d_->certificateMutex);
    if (id != d_->certificateId || d_->certificateDecision >= 0)
        return;
    d_->certificateDecision = decision >= 0 && decision <= 2 ? decision : 0;
    d_->certificateCondition.notify_all();
}
void RdpSession::sendScanCode(quint32 code, bool down) {
    Impl::Command c;
    c.type = Impl::CommandType::Key;
    c.code = code;
    c.down = down;
    d_->enqueue(std::move(c));
}
void RdpSession::sendUnicode(const QString &text) {
    if (text.size() > 4096)
        return;
    Impl::Command c;
    c.type = Impl::CommandType::Unicode;
    c.text = text;
    d_->enqueue(std::move(c));
}
void RdpSession::sendMouse(quint16 flags, int x, int y, bool extended) {
    Impl::Command c;
    c.type = Impl::CommandType::Mouse;
    c.flags = flags;
    c.x = x;
    c.y = y;
    c.extended = extended;
    d_->enqueue(std::move(c));
}
void RdpSession::releaseAllKeys() {
    d_->enqueue(Impl::Command{});
}
void RdpSession::setLocalClipboard(const QString &text) {
    Impl::Command c;
    c.type = Impl::CommandType::Clipboard;
    c.text = text.size() <= int(core::MaxClipboardBytes / 4) ? text : QString{};
    d_->enqueue(std::move(c));
}
void RdpSession::setClipboardActive(bool active) {
    Impl::Command c;
    c.type = Impl::CommandType::ClipboardFocus;
    c.down = active;
    d_->enqueue(std::move(c));
}
void RdpSession::resizeDesktop(QSize size) {
    Impl::Command c;
    c.type = Impl::CommandType::Resize;
    c.x = size.width();
    c.y = size.height();
    d_->enqueue(std::move(c));
}
void RdpSession::run() {
    QString message, diagnostic;
    rdpContext *context = nullptr;
    if (!d_->wake) {
        emit ended("Could not create the session event loop.", {}, false);
        return;
    }
    if (d_->stopping.load()) {
        emit ended("Connection cancelled.", {}, true);
        return;
    }
    emit stageChanged("Connecting");
    RDP_CLIENT_ENTRY_POINTS entry{};
    entry.Size = sizeof(entry);
    entry.Version = RDP_CLIENT_INTERFACE_VERSION;
    entry.ContextSize = sizeof(Impl::Context);
    entry.ClientNew = Impl::clientNew;
    context = freerdp_client_context_new(&entry);
    if (!context) {
        emit ended("FreeRDP could not create a session.", {}, false);
        return;
    }
    reinterpret_cast<Impl::Context *>(context)->self = d_.get();
    {
        std::lock_guard<std::mutex> lock(d_->lifetimeMutex);
        d_->context = context;
    }
    bool attempted = false, wasConnected = false;
    if (d_->configure(context->settings) && !d_->stopping.load()) {
        attempted = true;
        if (freerdp_connect(context->instance) && !d_->stopping.load()) {
            wasConnected = true;
            d_->connected.store(true);
            emit stageChanged("Connected");
            emit connected();
            emit featureChanged("Security", "NLA required");
            bool healthy = true;
            while (!d_->stopping.load() && !freerdp_shall_disconnect_context(context)) {
                if (!d_->processCommands()) {
                    healthy = false;
                    break;
                }
                d_->applyLayout();
                HANDLE handles[MAXIMUM_WAIT_OBJECTS]{};
                const DWORD count =
                    freerdp_get_event_handles(context, handles, MAXIMUM_WAIT_OBJECTS - 1);
                if (!count) {
                    healthy = false;
                    break;
                }
                handles[count] = d_->wake;
                const DWORD status = WaitForMultipleObjects(count + 1, handles, FALSE, 100);
                if (status == WAIT_FAILED) {
                    healthy = false;
                    break;
                }
                if (!freerdp_check_event_handles(context)) {
                    healthy = false;
                    break;
                }
            }
            if (!healthy)
                message = "The connection was interrupted. Check your LAN or VPN, then reconnect.";
        }
    }
    if (!d_->failure.isEmpty())
        message = d_->failure;
    const auto error = freerdp_get_last_error(context);
    diagnostic =
        QString("FreeRDP %1\nError: %2 (0x%3)\n%4")
            .arg(backendVersion(), safeText(freerdp_get_last_error_name(error)),
                 QString::number(error, 16), safeText(freerdp_get_last_error_string(error)));
    if (message.isEmpty() && !d_->stopping.load()) {
        const auto name = safeText(freerdp_get_last_error_name(error));
        if (name.contains("AUTHENTICATION") || name.contains("LOGON"))
            message = "Windows did not accept the sign-in. Check the username and account password "
                      "(not the PIN).";
        else if (name.contains("CERT"))
            message = "The computer's identity was not accepted. Verify its certificate before "
                      "reconnecting.";
        else if (name.contains("CONNECT") || name.contains("DNS"))
            message = "Could not reach this PC. Check its address, Remote Desktop setting, and "
                      "your LAN or VPN.";
        else
            message = error ? "The remote session ended with an error. Open Details for the "
                              "FreeRDP diagnostic."
                            : "The remote session ended.";
    }
    if (d_->stopping.load())
        message = wasConnected ? "Disconnected. Your Windows session has not been signed out."
                               : "Connection cancelled.";
    d_->releaseInput();
    d_->connected.store(false);
    if (attempted)
        freerdp_disconnect(context->instance);
    d_->finalCleanup(context->instance);
    {
        std::lock_guard<std::mutex> lock(d_->lifetimeMutex);
        d_->context = nullptr;
    }
    freerdp_client_context_free(context);
    wipe(d_->password);
    emit ended(message, diagnostic, d_->stopping.load());
}
} // namespace velo
