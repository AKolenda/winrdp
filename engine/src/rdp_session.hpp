// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include "core.hpp"
#include "profile.hpp"
#include <QImage>
#include <QPoint>
#include <QSize>
#include <QThread>
#include <memory>
#include <optional>

namespace velo {
// UI-facing boundary. No FreeRDP types leak into the UI.
// All network/input writes go through the worker's command queue.
class RdpSession final : public QThread {
    Q_OBJECT
  public:
    RdpSession(Profile profile, QString password, core::Layout layout, QObject *parent = nullptr);
    ~RdpSession() override;
    void stop();
    void sendScanCode(quint32 scanCode, bool down);
    void sendUnicode(const QString &text);
    void sendMouse(quint16 flags, int x, int y, bool extended = false);
    void releaseAllKeys();
    void setLocalClipboard(const QString &text); // null QString means "not text"
    void setClipboardActive(bool active);
    void resizeDesktop(QSize size);
    void answerCertificate(quint64 requestId, int decision); // 0 reject, 1 remember, 2 once
    std::optional<QImage> takeFrame();
    bool isConnected() const;
    static QString backendVersion();
    static QString backendBuild();
  signals:
    void stageChanged(QString stage);
    void connected();
    void ended(QString message, QString diagnostic, bool userInitiated);
    void desktopSizeChanged(QSize size);
    void cursorChanged(QImage image, QPoint hotSpot, bool hidden);
    void clipboardReceived(QString text);
    void featureChanged(QString feature, QString state);
    void certificateRequested(quint64 requestId, QString host, QString details, bool changed);
    void warning(QString message);

  protected:
    void run() override;

  private:
    struct Impl;
    std::unique_ptr<Impl> d_;
};
} // namespace velo
