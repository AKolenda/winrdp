// SPDX-License-Identifier: AGPL-3.0-only
#include "rdp_view.hpp"
#include "gpu_canvas.hpp"
#include "graphics_options.hpp"
#include <QApplication>
#include <QCursor>
#include <QFocusEvent>
#include <QGuiApplication>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QResizeEvent>
#include <QWheelEvent>
#include <algorithm>
#include <cmath>
namespace velo {
namespace {
constexpr quint16 Move = 0x0800, Down = 0x8000, Left = 0x1000, Right = 0x2000, Middle = 0x4000;
constexpr quint16 Wheel = 0x0200, HWheel = 0x0400;
} // namespace
RdpView::RdpView(RdpSession *session, QWidget *parent) : QWidget(parent), session_(session) {
    setObjectName("remoteDesktop");
    setFocusPolicy(Qt::StrongFocus);
    setMouseTracking(true);
    setAttribute(Qt::WA_OpaquePaintEvent);
    setAttribute(Qt::WA_InputMethodEnabled);
    QString device;
    if (GraphicsOptions::useOpenGL() && GpuCanvas::probe(&device)) {
        gpu_ = new GpuCanvas(this);
        gpu_->setGeometry(rect());
        connect(gpu_, &GpuCanvas::unavailable, this, [this](QString reason) {
            fallbackReason_ = reason;
            auto *old = gpu_; gpu_ = nullptr;
            if (old) { old->hide(); old->deleteLater(); }
            update();
        });
        gpu_->show();
    } else fallbackReason_ = "Software presentation (selected or hardware OpenGL unavailable)";
    resizeTimer_.setSingleShot(true);
    resizeTimer_.setInterval(240);
    connect(&resizeTimer_, &QTimer::timeout, this, [this] {
        if (dynamicResize_ && isVisible() && session_->isConnected())
            session_->resizeDesktop(size());
    });
}
void RdpView::setFrame(const QImage &image) {
    const bool changed = frame_.size() != image.size();
    frame_ = image;
    if (gpu_) gpu_->present(frame_,source_);
    else update();
    if (changed)
        updateCursor();
}
void RdpView::setSourceRect(QRect rect) {
    source_ = rect;
    if (gpu_) gpu_->present(frame_,source_);
    else update();
    updateCursor();
}
void RdpView::setDynamicResize(bool enabled) {
    dynamicResize_ = enabled;
    if (enabled)
        resizeTimer_.start();
    else
        resizeTimer_.stop();
}
QRect RdpView::effectiveSource() const {
    return source_.isEmpty() ? frame_.rect() : source_.intersected(frame_.rect());
}
core::MappedPoint RdpView::map(QPointF p) const {
    const auto r = effectiveSource();
    return core::mapPointer(width(), height(), {r.x(), r.y(), r.width(), r.height()}, p.x(), p.y());
}
QString RdpView::rendererDescription() const {
    return gpu_ ? gpu_->description() : fallbackReason_;
}
void RdpView::paintEvent(QPaintEvent *) {
    if (gpu_) return;
    QPainter p(this);
    p.fillRect(rect(), QColor("#101723"));
    const auto source = effectiveSource();
    if (frame_.isNull() || source.isEmpty())
        return;
    QSize target = source.size();
    target.scale(size(), Qt::KeepAspectRatio);
    const QRect destination(
        QPoint((width() - target.width()) / 2, (height() - target.height()) / 2), target);
    p.setRenderHint(QPainter::SmoothPixmapTransform, target != source.size());
    p.drawImage(destination, frame_, source);
}
void RdpView::resizeEvent(QResizeEvent *e) {
    QWidget::resizeEvent(e);
    if (gpu_) gpu_->setGeometry(rect());
    if (dynamicResize_)
        resizeTimer_.start();
    updateCursor();
}
void RdpView::mouseMoveEvent(QMouseEvent *e) {
    const auto p = map(e->position());
    if (p.inside || e->buttons() != Qt::NoButton)
        session_->sendMouse(Move, p.x, p.y);
    e->accept();
}
void RdpView::mouseButton(QMouseEvent *e, bool down) {
    const auto p = map(e->position());
    if (down && !p.inside)
        return;
    if (down) {
        setFocus(Qt::MouseFocusReason);
    }
    quint16 flag = 0;
    bool extended = false;
    switch (e->button()) {
    case Qt::LeftButton:
        flag = Left;
        break;
    case Qt::RightButton:
        flag = Right;
        break;
    case Qt::MiddleButton:
        flag = Middle;
        break;
    case Qt::BackButton:
        flag = 1;
        extended = true;
        break;
    case Qt::ForwardButton:
        flag = 2;
        extended = true;
        break;
    default:
        return;
    }
    session_->sendMouse(flag | (down ? Down : 0), p.x, p.y, extended);
    e->accept();
}
void RdpView::mousePressEvent(QMouseEvent *e) {
    mouseButton(e, true);
}
void RdpView::mouseReleaseEvent(QMouseEvent *e) {
    mouseButton(e, false);
}
void RdpView::wheelEvent(QWheelEvent *e) {
    const auto p = map(e->position());
    if (!p.inside)
        return;
    QPoint delta = e->angleDelta();
    if (delta.isNull())
        delta = e->pixelDelta() * 3;
    wheelRemainder_ += delta;
    auto send = [&](int &remaining, quint16 axis) {
        while (std::abs(remaining) >= 15) {
            const int amount = std::clamp(remaining, -120, 120);
            session_->sendMouse(axis | (quint16(amount) & 0x01ff), p.x, p.y);
            remaining -= amount;
        }
    };
    int x = wheelRemainder_.x(), y = wheelRemainder_.y();
    send(y, Wheel);
    send(x, HWheel);
    wheelRemainder_ = {x, y};
    e->accept();
}
void RdpView::captureKeyboard() {
    setFocus();
    if (QGuiApplication::platformName() == "xcb")
        grabKeyboard();
    captured_ = true;
    emit captureChanged(true);
}
void RdpView::releaseCapture() {
    if (QWidget::keyboardGrabber() == this)
        releaseKeyboard();
    captured_ = false;
    session_->releaseAllKeys();
    emit captureChanged(false);
}
void RdpView::focusOutEvent(QFocusEvent *e) {
    releaseCapture();
    QWidget::focusOutEvent(e);
}
void RdpView::sendChord(const QList<quint32> &keys) {
    session_->releaseAllKeys();
    for (auto key : keys)
        session_->sendScanCode(key, true);
    for (auto it = keys.crbegin(); it != keys.crend(); ++it)
        session_->sendScanCode(*it, false);
    setFocus();
}
bool RdpView::localShortcut(QKeyEvent *e, bool trigger) {
    const auto modifiers = e->modifiers();
    if (modifiers.testFlag(Qt::ControlModifier) && modifiers.testFlag(Qt::AltModifier)) {
        if (e->key() == Qt::Key_Home) {
            if (trigger && !e->isAutoRepeat()) { releaseCapture(); emit recoverWindowRequested(); }
            return true;
        }
        if (e->key() == Qt::Key_Escape) {
            if (trigger && !e->isAutoRepeat()) {
                releaseCapture();
                clearFocus();
            }
            return true;
        }
        if (e->key() == Qt::Key_Return || e->key() == Qt::Key_Enter) {
            if (trigger && !e->isAutoRepeat()) {
                releaseCapture();
                emit toggleFullscreenRequested();
            }
            return true;
        }
    }
    return false;
}
std::optional<quint32> RdpView::scanCode(QKeyEvent *e) const {
    // Qt's Linux xcb/Wayland backends normally expose XKB keycodes = evdev + 8.
    // Synthetic events and other platform plugins use the explicit fallback below.
    const auto platform = QGuiApplication::platformName();
    if ((platform == "xcb" || platform.startsWith("wayland")) && e->nativeScanCode() >= 9) {
        if (auto key = core::evdevToScanCode(e->nativeScanCode() - 8))
            return *key;
    }
    switch (e->key()) {
    case Qt::Key_Escape:
        return 0x01;
    case Qt::Key_Tab:
    case Qt::Key_Backtab:
        return 0x0f;
    case Qt::Key_Return:
        return 0x1c;
    case Qt::Key_Enter:
        return 0x11c;
    case Qt::Key_Backspace:
        return 0x0e;
    case Qt::Key_Space:
        return 0x39;
    case Qt::Key_Control:
        return 0x1d;
    case Qt::Key_Shift:
        return 0x2a;
    case Qt::Key_Alt:
        return 0x38;
    case Qt::Key_AltGr:
        return 0x138;
    case Qt::Key_Meta:
        return 0x15b;
    case Qt::Key_Left:
        return 0x14b;
    case Qt::Key_Right:
        return 0x14d;
    case Qt::Key_Up:
        return 0x148;
    case Qt::Key_Down:
        return 0x150;
    case Qt::Key_Home:
        return 0x147;
    case Qt::Key_End:
        return 0x14f;
    case Qt::Key_PageUp:
        return 0x149;
    case Qt::Key_PageDown:
        return 0x151;
    case Qt::Key_Insert:
        return 0x152;
    case Qt::Key_Delete:
        return 0x153;
    case Qt::Key_CapsLock:
        return 0x3a;
    case Qt::Key_NumLock:
        return 0x45;
    case Qt::Key_ScrollLock:
        return 0x46;
    default:
        break;
    }
    if (e->key() >= Qt::Key_F1 && e->key() <= Qt::Key_F10)
        return 0x3b + (e->key() - Qt::Key_F1);
    if (e->key() == Qt::Key_F11)
        return 0x57;
    if (e->key() == Qt::Key_F12)
        return 0x58;
    return std::nullopt;
}
void RdpView::keyPressEvent(QKeyEvent *e) {
    if (localShortcut(e, true)) {
        e->accept();
        return;
    }
    if (auto key = scanCode(e))
        session_->sendScanCode(*key, true);
    else if (!e->text().isEmpty() && !e->modifiers().testFlag(Qt::ControlModifier) &&
             !e->modifiers().testFlag(Qt::AltModifier))
        session_->sendUnicode(e->text());
    e->accept();
}
void RdpView::keyReleaseEvent(QKeyEvent *e) {
    if (e->isAutoRepeat() || localShortcut(e, false)) {
        e->accept();
        return;
    }
    if (auto key = scanCode(e))
        session_->sendScanCode(*key, false);
    e->accept();
}
bool RdpView::event(QEvent *e) {
    if (e->type() == QEvent::ShortcutOverride) {
        e->accept();
        return true;
    }
    if (e->type() == QEvent::KeyPress) {
        keyPressEvent(static_cast<QKeyEvent *>(e));
        return true;
    }
    if (e->type() == QEvent::KeyRelease) {
        keyReleaseEvent(static_cast<QKeyEvent *>(e));
        return true;
    }
    return QWidget::event(e);
}
void RdpView::inputMethodEvent(QInputMethodEvent *e) {
    if (!e->commitString().isEmpty())
        session_->sendUnicode(e->commitString());
    e->accept();
}
QVariant RdpView::inputMethodQuery(Qt::InputMethodQuery query) const {
    if (query == Qt::ImEnabled)
        return true;
    if (query == Qt::ImCursorRectangle)
        return QRect(width() / 2, height() / 2, 1, 20);
    return QWidget::inputMethodQuery(query);
}
void RdpView::setRemoteCursor(QImage image, QPoint hotspot, bool hidden) {
    cursorImage_ = std::move(image);
    cursorHotspot_ = hotspot;
    cursorHidden_ = hidden;
    updateCursor();
}
void RdpView::updateCursor() {
    if (cursorHidden_) {
        setCursor(Qt::BlankCursor);
        return;
    }
    if (cursorImage_.isNull()) {
        setCursor(Qt::ArrowCursor);
        return;
    }
    auto source = effectiveSource();
    const double scale = source.isEmpty() ? 1.0
                                          : std::min(double(width()) / source.width(),
                                                     double(height()) / source.height());
    const QSize target(std::clamp(int(std::round(cursorImage_.width() * scale)), 1, 384),
                       std::clamp(int(std::round(cursorImage_.height() * scale)), 1, 384));
    auto pixmap = QPixmap::fromImage(
        cursorImage_.scaled(target, Qt::IgnoreAspectRatio, Qt::SmoothTransformation));
    setCursor(QCursor(pixmap, std::clamp(int(cursorHotspot_.x() * scale), 0, target.width() - 1),
                      std::clamp(int(cursorHotspot_.y() * scale), 0, target.height() - 1)));
}
} // namespace velo
