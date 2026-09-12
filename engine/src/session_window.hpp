// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include "credentials.hpp"
#include "rdp_view.hpp"
#include <QMainWindow>
#include <QMap>
#include <QPointer>
#include <QTimer>
class QLabel;
class QPushButton;
class QStackedWidget;
class QPlainTextEdit;
class QScreen;
namespace velo {
class SessionWindow final : public QMainWindow {
    Q_OBJECT
  public:
    SessionWindow(Profile profile, QString password, bool remember, CredentialStore *secrets,
                  QWidget *parent = nullptr);
    ~SessionWindow() override;
    bool running() const;
    void bringForward();
    QString computerName() const { return profile_.name; }
    bool usesAllMonitors() const { return profile_.allMonitors; }
    void setTabMode(bool tabbed);
    void refreshFocus();
    void requestShutdown();
  signals:
    void successfulConnection(QString profileId);
    void activateRequested();
    void libraryRequested();
    void detachRequested(bool fullscreen);
    void attachRequested();
    void sessionClosing();
    void reconnectRequested(Profile profile);

  protected:
    void closeEvent(QCloseEvent *) override;

  private:
    Profile profile_;
    CredentialStore *secrets_;
    QString pendingSecret_;
    bool remember_ = false, closePending_ = false, allowClose_ = false, applyingClipboard_ = false,
         activeClipboard_ = false;
    bool multiActive_ = false, multiMatched_ = false;
    bool tabbed_ = false;
    QWidget *caption_ = nullptr, *grip_ = nullptr;
    QPushButton *tabButton_ = nullptr;
    RdpSession *session_ = nullptr;
    RdpView *view_ = nullptr;
    QStackedWidget *pages_ = nullptr;
    QLabel *stage_ = nullptr;
    QLabel *detail_ = nullptr;
    QLabel *status_ = nullptr;
    QLabel *notice_ = nullptr;
    QPlainTextEdit *diagnostic_ = nullptr;
    QPushButton *retry_ = nullptr;
    QPushButton *disconnect_ = nullptr;
    QTimer frameTimer_;
    QMap<QString, QString> features_;
    QList<QPointer<QWidget>> companions_;
    QList<QPointer<RdpView>> views_;
    QList<QPointer<QScreen>> screens_;
    core::Layout layout_;
    void begin(QString password);
    void makeCompanions();
    void closeCompanions();
    void toggleFullscreen();
    void syncClipboardFocus();
    void showCertificate(quint64 id, QString host, QString details, bool changed);
    void setNotice(const QString &message);
    void showDiagnostics();
};
} // namespace velo
