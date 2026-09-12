// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include <QDateTime>
#include <QJsonObject>
#include <QObject>
#include <QString>
#include <QVector>
#include <optional>

namespace velo {
struct Profile {
    QString id, name, address, username, group = "Personal";
    bool favorite = false, clipboard = true, audio = true;
    bool allMonitors = false, fullscreen = false, compatibility = false;
    QString graphics = "auto"; // auto, avc420, avc444; compatibility is separate.
    quint32 keyboardLayout = 0x00000409;
    QDateTime lastConnected;
    static Profile create();
    static std::optional<Profile> fromJson(const QJsonObject &, QString &error);
    QJsonObject toJson() const;
    QString validationError() const;
    QString credentialKey() const;
};
class ProfileStore : public QObject {
    Q_OBJECT
  public:
    explicit ProfileStore(QString directory = {}, QObject *parent = nullptr);
    bool load(QString &error);
    bool upsert(Profile profile, QString &error);
    bool remove(const QString &id, QString &error);
    bool recordConnection(const QString &id, QString &error);
    const QVector<Profile> &profiles() const {
        return profiles_;
    }
    std::optional<Profile> find(const QString &id) const;
    QString path() const;
  signals:
    void changed();

  private:
    bool commit(const QVector<Profile> &next, QString &error);
    QString directory_;
    QVector<Profile> profiles_;
    bool writable_ = true;
};
} // namespace velo
