// SPDX-License-Identifier: AGPL-3.0-only
#include "window_chrome.hpp"
#include "window_geometry.hpp"
#include "theme.hpp"
#include <QApplication>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QLabel>
#include <QMainWindow>
#include <QMouseEvent>
#include <QPainter>
#include <QScreen>
#include <QTabBar>
#include <QTimer>
#include <QWindow>
namespace velo {
namespace {
QRect clampTo(QRect r, QRect a) {
    const auto v = geometry::clamp({r.x(),r.y(),r.width(),r.height()}, {a.x(),a.y(),a.width(),a.height()});
    return {v.x,v.y,v.width,v.height};
}
QRect centeredOn(QRect a, QSize size) {
    const auto v = geometry::centered({a.x(),a.y(),a.width(),a.height()}, size.width(),size.height());
    return {v.x,v.y,v.width,v.height};
}
}
CaptionButton::CaptionButton(Action action, QWidget *target, QWidget *parent)
    : QAbstractButton(parent), action_(action), target_(target) {
    setFixedSize(46, 40);
    setFocusPolicy(Qt::StrongFocus);
    setObjectName(action == Close ? "captionClose" : action == Maximize ? "captionMaximize" : "captionMinimize");
    setAccessibleName(action == Close ? "Close window" : action == Maximize ? "Maximize or restore window" : "Minimize window");
    setToolTip(accessibleName());
    connect(this, &QAbstractButton::clicked, this, [this] {
        if (!target_) return;
        if (action_ == Close) target_->close();
        else if (action_ == Minimize) target_->showMinimized();
        else if (target_->isFullScreen()) { if (auto *c = WindowChrome::get(target_)) c->leaveFullscreen(); }
        else if (target_->isMaximized()) target_->showNormal();
        else target_->showMaximized();
        update();
    });
}
void CaptionButton::enterEvent(QEnterEvent *e) { hover_ = true; update(); QAbstractButton::enterEvent(e); }
void CaptionButton::leaveEvent(QEvent *e) { hover_ = false; update(); QAbstractButton::leaveEvent(e); }
void CaptionButton::paintEvent(QPaintEvent *) {
    QPainter p(this);
    const auto t = Theme::current();
    if (hover_ || isDown()) p.fillRect(rect(), action_ == Close ? QColor(isDown()?"#bb2935":"#c42b1c") : t.hover);
    p.setRenderHint(QPainter::Antialiasing, false);
    p.setPen(QPen(hover_ && action_ == Close ? QColor(Qt::white) : t.text, 1));
    const int x = width()/2-5, y = height()/2-5;
    if (action_ == Minimize) p.drawLine(x,y+5,x+10,y+5);
    else if (action_ == Close) { p.drawLine(x,y,x+10,y+10); p.drawLine(x+10,y,x,y+10); }
    else if (target_ && (target_->isMaximized() || target_->isFullScreen())) {
        p.drawLine(x+3,y,x+10,y); p.drawLine(x+10,y,x+10,y+7); p.drawRect(x,y+3,7,7);
    } else p.drawRect(x,y,10,10);
    if (hasFocus()) { p.setPen(QPen(t.accent,1,Qt::DotLine)); p.drawRect(rect().adjusted(4,4,-5,-5)); }
}
WindowChrome::WindowChrome(QMainWindow *window, int titleHeight)
    : QObject(window), window_(window), titleHeight_(titleHeight) {
    setObjectName("winWindowChrome");
    window->setWindowFlag(Qt::FramelessWindowHint, true);
    window->setMouseTracking(true);
    qApp->installEventFilter(this);
    connect(qApp, &QGuiApplication::screenRemoved, this, [this](QScreen *) {
        // Output hot-unplug is asynchronous. Recover after Qt updates its screen list.
        QTimer::singleShot(0, this, [this] { if (window_ && window_->isWindow()) recover(); });
    });
}
WindowChrome *WindowChrome::get(QWidget *window) {
    return window ? window->findChild<WindowChrome *>("winWindowChrome", Qt::FindDirectChildrenOnly) : nullptr;
}
QWidget *WindowChrome::controls(QWidget *window, QWidget *parent) {
    auto *w = new QWidget(parent); w->setObjectName("captionControls");
    auto *l = new QHBoxLayout(w); l->setSpacing(0); l->setContentsMargins(0,0,0,0);
    for (auto action : {CaptionButton::Minimize, CaptionButton::Maximize, CaptionButton::Close}) l->addWidget(new CaptionButton(action,window,w));
    return w;
}
QWidget *WindowChrome::titleBar(QWidget *window, const QString &title) {
    auto *w = new QFrame(window); w->setObjectName("windowTitleBar"); w->setProperty("windowDragArea", true);
    w->setFixedHeight(40);
    auto *l = new QHBoxLayout(w); l->setSpacing(10); l->setContentsMargins(12,0,0,0);
    auto *i = label({}); i->setPixmap(icon("logo",Theme::current().accent).pixmap(20,20)); i->setAttribute(Qt::WA_TransparentForMouseEvents); l->addWidget(i);
    auto *text = label(title); text->setAttribute(Qt::WA_TransparentForMouseEvents); l->addWidget(text); l->addStretch();
    l->addWidget(controls(window,w));
    return w;
}
QScreen *WindowChrome::targetScreen() const {
    if (window_ && window_->windowHandle() && window_->windowHandle()->screen()) return window_->windowHandle()->screen();
    if (savedScreen_) return savedScreen_;
    return QGuiApplication::primaryScreen();
}
void WindowChrome::placeOn(QScreen *screen, QSize size) {
    if (!window_ || !window_->isWindow() || !screen) return;
    window_->winId();
    window_->windowHandle()->setScreen(screen);
    window_->setGeometry(centeredOn(screen->availableGeometry(),size));
}
void WindowChrome::enterFullscreen(QScreen *target) {
    if (!window_ || !window_->isWindow() || window_->isFullScreen()) return;
    if (!target) target = targetScreen();
    if (!target) return;
    savedScreen_ = target;
    savedMaximized_ = window_->isMaximized();
    savedGeometry_ = savedMaximized_ ? window_->normalGeometry() : window_->geometry();
    if (!savedGeometry_.isValid()) savedGeometry_ = centeredOn(target->availableGeometry(),QSize(1240,820));
    // Pin the toplevel to the current output BEFORE requesting fullscreen.
    // In particular: no screen-union size and no physical-pixel multiplication.
    window_->winId();
    if (window_->windowHandle()->screen() != target) {
        window_->showNormal();
        window_->windowHandle()->setScreen(target);
        window_->setGeometry(centeredOn(target->availableGeometry(),window_->size()));
    }
    window_->showFullScreen();
    window_->raise(); window_->activateWindow();
}
void WindowChrome::restoreSafe() {
    if (!window_) return;
    QScreen *s = savedScreen_ ? savedScreen_.data() : targetScreen();
    if (!s) return;
    const QRect a = s->availableGeometry();
    window_->winId(); window_->windowHandle()->setScreen(s);
    window_->setGeometry(clampTo(savedGeometry_.isValid() ? savedGeometry_ : centeredOn(a,QSize(1240,820)),a));
}
void WindowChrome::leaveFullscreen() {
    if (!window_ || !window_->isWindow()) return;
    window_->showNormal(); restoreSafe();
    if (savedMaximized_) window_->showMaximized();
    window_->raise(); window_->activateWindow();
}
void WindowChrome::toggleFullscreen() {
    if (!window_) return;
    if (window_->isFullScreen()) leaveFullscreen(); else enterFullscreen();
}
void WindowChrome::ensureVisible() {
    if (!window_ || !window_->isWindow()) return;
    // Leave valid maximized/fullscreen state intact at startup. Only recover a
    // genuinely inaccessible restored window (e.g. a removed second display).
    if (window_->isMaximized() || window_->isFullScreen()) return;
    const QRect r = window_->geometry();
    for (auto *screen : QGuiApplication::screens())
        if (screen->availableGeometry().contains(r)) return;
    savedScreen_ = targetScreen(); savedGeometry_ = r;
    restoreSafe();
}
void WindowChrome::recover() {
    if (!window_ || !window_->isWindow()) return;
    if (!window_->isFullScreen()) savedGeometry_ = window_->isMaximized() ? window_->normalGeometry() : window_->geometry();
    window_->showNormal();
    if (!savedScreen_) savedScreen_ = targetScreen();
    restoreSafe(); window_->raise(); window_->activateWindow();
}
bool WindowChrome::eventFilter(QObject *object, QEvent *event) {
    if (!window_ || !window_->isWindow()) return false;
    auto *widget = qobject_cast<QWidget *>(object);
    if (!widget || widget->window() != window_.data()) return false;
    if (event->type() == QEvent::WindowStateChange) {
        for (auto *button : window_->findChildren<CaptionButton *>()) button->update();
        return false;
    }
    if (event->type() != QEvent::MouseButtonPress && event->type() != QEvent::MouseButtonDblClick && event->type() != QEvent::MouseMove) return false;
    auto *mouse = static_cast<QMouseEvent *>(event);
    const QPoint at = window_->mapFromGlobal(mouse->globalPosition().toPoint());
    Qt::Edges edges;
    if (!window_->isMaximized() && !window_->isFullScreen()) {
        if (at.x() < 6) edges |= Qt::LeftEdge;
        if (at.x() >= window_->width()-6) edges |= Qt::RightEdge;
        if (at.y() < 5) edges |= Qt::TopEdge;
        if (at.y() >= window_->height()-6) edges |= Qt::BottomEdge;
    }
    if (event->type() == QEvent::MouseMove && mouse->buttons() == Qt::NoButton) {
        if (edges == (Qt::TopEdge|Qt::LeftEdge) || edges == (Qt::BottomEdge|Qt::RightEdge)) window_->setCursor(Qt::SizeFDiagCursor);
        else if (edges == (Qt::TopEdge|Qt::RightEdge) || edges == (Qt::BottomEdge|Qt::LeftEdge)) window_->setCursor(Qt::SizeBDiagCursor);
        else if (edges.testFlag(Qt::LeftEdge) || edges.testFlag(Qt::RightEdge)) window_->setCursor(Qt::SizeHorCursor);
        else if (edges) window_->setCursor(Qt::SizeVerCursor);
        else window_->unsetCursor();
        return false;
    }
    if (mouse->button() != Qt::LeftButton) return false;
    if (edges && event->type() == QEvent::MouseButtonPress) {
        window_->winId();
        return window_->windowHandle()->startSystemResize(edges);
    }
    bool draggable = widget->property("windowDragArea").toBool();
    if (auto *bar = qobject_cast<QTabBar *>(widget)) draggable = bar->tabAt(mouse->position().toPoint()) < 0;
    // Empty part of the top tab strip is a drag area; actual buttons/tabs are not.
    if (widget->objectName() == "sessionTabs" && at.y() < titleHeight_) draggable = true;
    if (!draggable || window_->isFullScreen()) return false;
    if (event->type() == QEvent::MouseButtonDblClick) {
        if (window_->isMaximized()) window_->showNormal(); else window_->showMaximized();
        return true;
    }
    if (event->type() == QEvent::MouseButtonPress) {
        window_->winId(); return window_->windowHandle()->startSystemMove();
    }
    return false;
}
} // namespace velo
