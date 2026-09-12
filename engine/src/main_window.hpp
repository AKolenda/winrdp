// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include "credentials.hpp"
#include "profile.hpp"
#include <QDialog>
#include <QMainWindow>
#include <QMap>
#include <QPointer>
class QCheckBox;
class QComboBox;
class QGridLayout;
class QLabel;
class QLineEdit;
class QPushButton;
class QResizeEvent;
class QStackedWidget;
class QTreeWidget;
class QTabWidget;
class QCloseEvent;
namespace velo {
class SessionWindow;
class ProfileDialog final : public QDialog {
    Q_OBJECT
  public:
    ProfileDialog(Profile profile, bool adding, QWidget *parent = nullptr);
    Profile profile() const;

  private:
    Profile original_;
    QLineEdit *name_, *address_, *username_, *group_;
    QCheckBox *clipboard_, *audio_, *monitors_, *fullscreen_, *compatibility_;
    QComboBox *keyboard_, *graphics_;
    QLabel *error_;
};
class MainWindow final : public QMainWindow {
    Q_OBJECT
  public:
    MainWindow(ProfileStore *store, bool demo = false, QWidget *parent = nullptr);
    void connectTo(Profile profile);
    void openSettings();
    void openHost();
    int visibleComputerCount() const {
        return visibleCount_;
    }

  protected:
    void resizeEvent(QResizeEvent *) override;
    void closeEvent(QCloseEvent *) override;
    bool eventFilter(QObject *, QEvent *) override;

  private:
    ProfileStore *store_;
    CredentialStore *secrets_;
    bool demo_ = false;
    int navigation_ = 0, columns_ = 0, visibleCount_ = 0;
    QStackedWidget *pages_ = nullptr;
    QTabWidget *tabs_ = nullptr;
    QWidget *library_ = nullptr;
    QLabel *selection_ = nullptr;
    bool closing_ = false;
    QTreeWidget *tree_ = nullptr;
    QLineEdit *search_ = nullptr, *quick_ = nullptr;
    QComboBox *groups_ = nullptr;
    QWidget *cards_ = nullptr;
    QGridLayout *grid_ = nullptr;
    QLabel *count_ = nullptr, *empty_ = nullptr, *banner_ = nullptr, *heading_ = nullptr;
    QList<QPushButton *> navigationButtons_;
    QMap<QString, QPointer<SessionWindow>> sessions_;
    void rebuild();
    void refreshGroups();
    void edit(Profile profile, bool adding);
    void removeProfile(Profile profile);
    void showSettings();
    void showShortcuts();
    void report(const QString &error);
    void forget(const Profile &profile);
    QWidget *makeSettingsPage();
    QWidget *makeHostPage();
    void quickConnect();
    void attachSession(SessionWindow *session);
    void detachSession(SessionWindow *session, bool fullscreen);
    void finishClosing();
    void launchSetup(const QString &page);
    Profile defaultProfile() const;
};
} // namespace velo
