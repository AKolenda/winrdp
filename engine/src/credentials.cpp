// SPDX-License-Identifier: AGPL-3.0-only
#include "credentials.hpp"
#include "theme.hpp"
#include <QAction>
#include <QCheckBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPointer>
#include <QPushButton>
#include <QTimer>
#include <QVBoxLayout>
#ifdef VELO_HAVE_KEYCHAIN
#include <qt6keychain/keychain.h>
#endif
namespace velo {
bool CredentialStore::available() {
#ifdef VELO_HAVE_KEYCHAIN
    return true;
#else
    return false;
#endif
}
void CredentialStore::read(const QString &key, QObject *lifetime, ReadCallback done) {
#ifdef VELO_HAVE_KEYCHAIN
    auto *job = new QKeychain::ReadPasswordJob("io.velordp.Velo", this);
    job->setKey(key);
    job->setInsecureFallback(false);
    connect(job, &QKeychain::Job::finished, lifetime,
            [job, done = std::move(done)](QKeychain::Job *) {
                if (job->error() == QKeychain::NoError)
                    done(job->textData(), {});
                else if (job->error() == QKeychain::EntryNotFound)
                    done({}, {});
                else
                    done({}, job->errorString());
            });
    job->start();
#else
    Q_UNUSED(key);
    QTimer::singleShot(0, lifetime, [done = std::move(done)] { done({}, {}); });
#endif
}
void CredentialStore::write(const QString &key, const QString &password, QObject *lifetime,
                            WriteCallback done) {
#ifdef VELO_HAVE_KEYCHAIN
    auto *job = new QKeychain::WritePasswordJob("io.velordp.Velo", this);
    job->setKey(key);
    job->setTextData(password);
    job->setInsecureFallback(false);
    connect(job, &QKeychain::Job::finished, lifetime,
            [job, done = std::move(done)](QKeychain::Job *) {
                done(job->error() == QKeychain::NoError ? QString{} : job->errorString());
            });
    job->start();
#else
    Q_UNUSED(key);
    Q_UNUSED(password);
    QTimer::singleShot(0, lifetime, [done = std::move(done)] {
        done("This build does not include secure password storage. Your password was not saved.");
    });
#endif
}
void CredentialStore::forget(const QString &key, QObject *lifetime, WriteCallback done) {
#ifdef VELO_HAVE_KEYCHAIN
    auto *job = new QKeychain::DeletePasswordJob("io.velordp.Velo", this);
    job->setKey(key);
    job->setInsecureFallback(false);
    connect(job, &QKeychain::Job::finished, lifetime,
            [job, done = std::move(done)](QKeychain::Job *) {
                done(job->error() == QKeychain::NoError || job->error() == QKeychain::EntryNotFound
                         ? QString{}
                         : job->errorString());
            });
    job->start();
#else
    Q_UNUSED(key);
    QTimer::singleShot(0, lifetime, [done = std::move(done)] { done({}); });
#endif
}
LoginDialog::LoginDialog(Profile profile, CredentialStore *secrets, QWidget *parent)
    : QDialog(parent), profile_(std::move(profile)) {
    setWindowTitle("Connect to " + profile_.name);
    setMinimumWidth(460);
    setModal(true);
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(28, 26, 28, 24);
    layout->setSpacing(16);
    layout->addWidget(label("A little closer to your PC.", "heading"));
    layout->addWidget(label(profile_.name + "  ·  " + profile_.address, "muted"));
    auto *form = new QFormLayout;
    form->setSpacing(12);
    username_ = new QLineEdit(profile_.username);
    username_->setObjectName("loginUsername");
    username_->setPlaceholderText("username, COMPUTER\\user, or email");
    username_->setMaxLength(256);
    password_ = new QLineEdit;
    password_->setObjectName("loginPassword");
    password_->setEchoMode(QLineEdit::Password);
    password_->setPlaceholderText("Your Windows account password");
    password_->setMaxLength(1024);
    auto *eye = password_->addAction(icon("info"), QLineEdit::TrailingPosition);
    eye->setToolTip("Show or hide password");
    eye->setCheckable(true);
    connect(eye, &QAction::toggled, this, [this](bool shown) {
        password_->setEchoMode(shown ? QLineEdit::Normal : QLineEdit::Password);
    });
    form->addRow("Username", username_);
    form->addRow("Password", password_);
    layout->addLayout(form);
    auto *note = label("Use the account password, not a Windows Hello PIN.", "muted");
    note->setWordWrap(true);
    layout->addWidget(note);
    remember_ = new QCheckBox("Remember securely in my Linux keyring");
    remember_->setEnabled(CredentialStore::available());
    layout->addWidget(remember_);
    hint_ = label(CredentialStore::available()
                      ? "Saved only after the connection succeeds."
                      : "Keyring support is not installed. Passwords stay unsaved.",
                  "muted");
    hint_->setWordWrap(true);
    layout->addWidget(hint_);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Cancel | QDialogButtonBox::Ok);
    auto *ok = buttons->button(QDialogButtonBox::Ok);
    ok->setText("Connect");
    ok->setProperty("primary", true);
    ok->setDefault(true);
    layout->addWidget(buttons);
    auto validate = [this, ok] {
        ok->setEnabled(!username_->text().trimmed().isEmpty() && !password_->text().isEmpty());
    };
    connect(username_, &QLineEdit::textChanged, this, validate);
    connect(password_, &QLineEdit::textChanged, this, validate);
    validate();
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    if (!profile_.username.isEmpty() && CredentialStore::available()) {
        const auto account = profile_.username;
        secrets->read(profile_.credentialKey(), this,
                      [this, account](QString stored, QString error) {
                          if (!error.isEmpty()) {
                              hint_->setText("Keyring unavailable. Enter your password; it will "
                                             "not be saved unless the keyring works.");
                              return;
                          }
                          // Never overwrite text the user has already started typing.
                          if (username_->text() == account && password_->text().isEmpty() &&
                              !stored.isEmpty()) {
                              password_->setText(stored);
                              remember_->setChecked(true);
                          }
                          stored.fill(QChar(0));
                          stored.clear();
                      });
    }
    if (profile_.username.isEmpty())
        username_->setFocus();
    else
        password_->setFocus();
}
LoginDialog::~LoginDialog() {
    password_->clear();
}
QString LoginDialog::username() const {
    return username_->text().trimmed();
}
QString LoginDialog::password() const {
    return password_->text();
}
void LoginDialog::disableRemember() {
    remember_->setChecked(false);
    remember_->setEnabled(false);
    hint_->setText("Quick connections do not save passwords. Add this PC to your library to "
                   "remember it securely.");
}
bool LoginDialog::remember() const {
    return remember_->isEnabled() && remember_->isChecked();
}
} // namespace velo
