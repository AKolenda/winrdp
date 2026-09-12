// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include <QAbstractButton>
#include <QFrame>
#include <QObject>
#include <QPointer>
#include <QRect>
#include <QSize>
class QMainWindow;
class QScreen;
namespace velo {
class CaptionButton final : public QAbstractButton {
    Q_OBJECT
  public:
    enum Action { Minimize, Maximize, Close };
    CaptionButton(Action action, QWidget *window, QWidget *parent = nullptr);
  protected:
    void paintEvent(QPaintEvent *) override;
    void enterEvent(QEnterEvent *) override;
    void leaveEvent(QEvent *) override;
  private:
    Action action_;
    QPointer<QWidget> target_;
    bool hover_ = false;
};
class WindowChrome final : public QObject {
    Q_OBJECT
  public:
    explicit WindowChrome(QMainWindow *window, int titleHeight = 44);
    static WindowChrome *get(QWidget *window);
    static QWidget *controls(QWidget *window, QWidget *parent = nullptr);
    static QWidget *titleBar(QWidget *window, const QString &title);
    void enterFullscreen(QScreen *target = nullptr);
    void leaveFullscreen();
    void toggleFullscreen();
    void recover();
    void ensureVisible();
    void placeOn(QScreen *screen, QSize preferred = QSize(1240, 820));
  protected:
    bool eventFilter(QObject *, QEvent *) override;
  private:
    QPointer<QMainWindow> window_;
    QPointer<QScreen> savedScreen_;
    QRect savedGeometry_;
    bool savedMaximized_ = false;
    int titleHeight_ = 44;
    QScreen *targetScreen() const;
    void restoreSafe();
};
} // namespace velo
