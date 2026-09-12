// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include "profile.hpp"
#include <QDialog>
#include <QObject>
#include <functional>
class QLineEdit;
class QCheckBox;
class QLabel;
class QPushButton;
namespace velo {
// Secret Service / KWallet through QtKeychain. Insecure fallback is always disabled.
class CredentialStore : public QObject {
    Q_OBJECT
  public:
    using ReadCallback = std::function<void(QString password, QString error)>;
    using WriteCallback = std::function<void(QString error)>;
    explicit CredentialStore(QObject *parent = nullptr) : QObject(parent) {}
    static bool available();
    void read(const QString &key, QObject *lifetime, ReadCallback done);
    void write(const QString &key, const QString &password, QObject *lifetime, WriteCallback done);
    void forget(const QString &key, QObject *lifetime, WriteCallback done);
};
class LoginDialog : public QDialog {
    Q_OBJECT
  public:
    LoginDialog(Profile profile, CredentialStore *secrets, QWidget *parent = nullptr);
    ~LoginDialog() override;
    QString username() const;
    QString password() const;
    bool remember() const;
    void disableRemember();

  private:
    Profile profile_;
    QLineEdit *username_ = nullptr;
    QLineEdit *password_ = nullptr;
    QCheckBox *remember_ = nullptr;
    QLabel *hint_ = nullptr;
};
} // namespace velo
