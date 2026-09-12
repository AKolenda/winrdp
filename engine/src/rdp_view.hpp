// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include "rdp_session.hpp"
#include <QImage>
#include <QRect>
#include <QTimer>
#include <QWidget>
class QKeyEvent;
namespace velo {
class GpuCanvas;
class RdpView final : public QWidget {
    Q_OBJECT
  public:
    explicit RdpView(RdpSession *session, QWidget *parent = nullptr);
    void setFrame(const QImage &frame);
    QString rendererDescription() const;
    void setSourceRect(QRect source); // Empty = entire remote desktop.
    void setDynamicResize(bool enabled);
    void setRemoteCursor(QImage image, QPoint hotspot, bool hidden);
    void releaseCapture();
    void captureKeyboard();
    void sendChord(const QList<quint32> &keys);
  signals:
    void toggleFullscreenRequested();
    void recoverWindowRequested();
    void captureChanged(bool captured);

  protected:
    void paintEvent(QPaintEvent *) override;
    void resizeEvent(QResizeEvent *) override;
    void mouseMoveEvent(QMouseEvent *) override;
    void mousePressEvent(QMouseEvent *) override;
    void mouseReleaseEvent(QMouseEvent *) override;
    void wheelEvent(QWheelEvent *) override;
    void keyPressEvent(QKeyEvent *) override;
    void keyReleaseEvent(QKeyEvent *) override;
    void focusOutEvent(QFocusEvent *) override;
    void inputMethodEvent(QInputMethodEvent *) override;
    QVariant inputMethodQuery(Qt::InputMethodQuery query) const override;
    bool event(QEvent *) override;

  private:
    RdpSession *session_;
    GpuCanvas *gpu_ = nullptr;
    QString fallbackReason_;
    QImage frame_, cursorImage_;
    QRect source_;
    QPoint cursorHotspot_;
    bool dynamicResize_ = true, captured_ = false, cursorHidden_ = false;
    QTimer resizeTimer_;
    QPoint wheelRemainder_;
    QRect effectiveSource() const;
    core::MappedPoint map(QPointF point) const;
    bool localShortcut(QKeyEvent *event, bool trigger);
    std::optional<quint32> scanCode(QKeyEvent *) const;
    void mouseButton(QMouseEvent *, bool down);
    void updateCursor();
};
} // namespace velo
