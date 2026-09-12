// SPDX-License-Identifier: AGPL-3.0-only
#include "profile.hpp"
#include "core.hpp"
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSaveFile>
#include <QSet>
#include <QStandardPaths>
#include <QUuid>
#include <algorithm>

namespace velo {
Profile Profile::create() {
    Profile p;
    p.id = QUuid::createUuid().toString(QUuid::WithoutBraces);
    return p;
}
QString Profile::validationError() const {
    if (QUuid(id).isNull())
        return "The computer profile has an invalid identifier.";
    if (name.trimmed().isEmpty() || name.size() > 100)
        return "Give this computer a name (up to 100 characters).";
    const auto ep = core::parseEndpoint(address.toStdString());
    if (!ep.value)
        return QString::fromStdString(ep.error);
    if (username.size() > 256 || group.size() > 80)
        return "The username or group name is too long.";
    for (const auto &s : {name, username, group})
        for (const auto c : s)
            if (c.unicode() < 32 || c.unicode() == 127)
                return "Names cannot contain control characters.";
    if (graphics != "auto" && graphics != "avc420" && graphics != "avc444")
        return "Choose a supported graphics mode.";
    if (!keyboardLayout)
        return "Choose a Windows keyboard layout.";
    return {};
}
QJsonObject Profile::toJson() const {
    // Deliberate allowlist: secrets can never be serialized through this API.
    return {{"id", id},
            {"name", name},
            {"address", address},
            {"username", username},
            {"group", group},
            {"favorite", favorite},
            {"clipboard", clipboard},
            {"audio", audio},
            {"allMonitors", allMonitors},
            {"fullscreen", fullscreen},
            {"compatibility", compatibility},
            {"graphics", graphics},
            {"keyboardLayout", double(keyboardLayout)},
            {"lastConnected", lastConnected.toUTC().toString(Qt::ISODateWithMs)}};
}
std::optional<Profile> Profile::fromJson(const QJsonObject &o, QString &error) {
    Profile p;
    for (const auto *key : {"id", "name", "address"})
        if (!o.value(key).isString()) {
            error = "Missing or invalid profile fields.";
            return std::nullopt;
        }
    p.id = o["id"].toString();
    p.name = o["name"].toString();
    p.address = o["address"].toString();
    p.username = o["username"].toString();
    p.group = o["group"].toString("Personal");
    p.favorite = o["favorite"].toBool();
    p.clipboard = o["clipboard"].toBool(true);
    p.audio = o["audio"].toBool(true);
    p.allMonitors = o["allMonitors"].toBool();
    p.fullscreen = o["fullscreen"].toBool();
    p.compatibility = o["compatibility"].toBool();
    p.graphics = o["graphics"].toString("auto");
    const auto layout = o["keyboardLayout"].toDouble(0x409);
    if (layout < 1 || layout > 0xffffffffULL || layout != static_cast<quint32>(layout)) {
        error = "Invalid keyboard layout.";
        return std::nullopt;
    }
    p.keyboardLayout = static_cast<quint32>(layout);
    p.lastConnected = QDateTime::fromString(o["lastConnected"].toString(), Qt::ISODateWithMs);
    error = p.validationError();
    if (!error.isEmpty())
        return std::nullopt;
    p.address =
        QString::fromStdString(core::parseEndpoint(p.address.toStdString()).value->display());
    return p;
}
QString Profile::credentialKey() const {
    // Credentials are bound to the profile AND its destination/account.
    const auto ep = core::parseEndpoint(address.toStdString());
    QByteArray data = id.toUtf8();
    data.append('\0');
    if (ep.value) {
        auto canonical = QByteArray::fromStdString(ep.value->display());
        const auto zone = canonical.indexOf('%');
        // DNS names and hex digits are case-insensitive; IPv6 interface names are not.
        data +=
            zone < 0 ? canonical.toLower() : canonical.left(zone).toLower() + canonical.mid(zone);
    } else
        data += address.toUtf8();
    data.append('\0');
    data += username.toUtf8();
    return "profile/" +
           QString::fromLatin1(QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex());
}
ProfileStore::ProfileStore(QString directory, QObject *parent)
    : QObject(parent), directory_(std::move(directory)) {
    if (directory_.isEmpty())
        directory_ = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
}
QString ProfileStore::path() const {
    return QDir(directory_).filePath("computers.json");
}
bool ProfileStore::load(QString &error) {
    error.clear();
    QFile file(path());
    if (!file.exists()) {
        profiles_.clear();
        writable_ = true;
        return true;
    }
    auto fail = [&](const QString &reason) {
        error = reason + "\nYour existing file has not been changed: " + path();
        writable_ = false;
        return false;
    };
    if (!file.open(QIODevice::ReadOnly))
        return fail(file.errorString());
    if (file.size() > 4 * 1024 * 1024)
        return fail("The profile file is larger than the supported 4 MB.");
    QJsonParseError parse;
    const auto doc = QJsonDocument::fromJson(file.readAll(), &parse);
    if (parse.error != QJsonParseError::NoError || !doc.isObject())
        return fail("Could not read the computer library.");
    const auto root = doc.object();
    if (root["version"].toInt() != 1 || !root["computers"].isArray())
        return fail("This library has an unsupported format.");
    const auto array = root["computers"].toArray();
    if (array.size() > 2000)
        return fail("The computer library exceeds the 2,000-profile limit.");
    QVector<Profile> next;
    QSet<QString> ids;
    for (const auto &item : array) {
        if (!item.isObject())
            return fail("A computer profile is invalid.");
        QString why;
        auto p = Profile::fromJson(item.toObject(), why);
        if (!p)
            return fail(why);
        if (ids.contains(p->id))
            return fail("The library contains duplicate identifiers.");
        ids.insert(p->id);
        next.push_back(*p);
    }
    profiles_ = std::move(next);
    writable_ = true;
    emit changed();
    return true;
}
bool ProfileStore::commit(const QVector<Profile> &next, QString &error) {
    error.clear();
    if (!writable_) {
        error = "The existing library could not be loaded. Back it up and repair or rename it "
                "before saving.";
        return false;
    }
    if (next.size() > 2000) {
        error = "The 2,000-computer library limit has been reached.";
        return false;
    }
    QDir dir(directory_);
    if (!dir.mkpath(".")) {
        error = "Could not create the configuration directory.";
        return false;
    }
    if (!QFile::setPermissions(directory_, QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                                               QFileDevice::ExeOwner)) {
        error = "Could not protect the configuration directory.";
        return false;
    }
    QJsonArray array;
    for (const auto &p : next) {
        const auto why = p.validationError();
        if (!why.isEmpty()) {
            error = why;
            return false;
        }
        array.append(p.toJson());
    }
    QSaveFile file(path());
    file.setDirectWriteFallback(false);
    if (!file.open(QIODevice::WriteOnly)) {
        error = file.errorString();
        return false;
    }
    if (!file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner)) {
        error = "Could not protect the profile file.";
        file.cancelWriting();
        return false;
    }
    const auto bytes = QJsonDocument(QJsonObject{{"version", 1}, {"computers", array}})
                           .toJson(QJsonDocument::Indented);
    if (file.write(bytes) != bytes.size()) {
        error = file.errorString();
        file.cancelWriting();
        return false;
    }
    if (!file.commit()) {
        error = file.errorString();
        return false;
    }
    profiles_ = next;
    emit changed();
    return true;
}
bool ProfileStore::upsert(Profile p, QString &error) {
    p.name = p.name.trimmed();
    p.username = p.username.trimmed();
    p.group = p.group.trimmed();
    if (p.group.isEmpty())
        p.group = "Personal";
    error = p.validationError();
    if (!error.isEmpty())
        return false;
    p.address =
        QString::fromStdString(core::parseEndpoint(p.address.toStdString()).value->display());
    auto next = profiles_;
    auto it = std::find_if(next.begin(), next.end(), [&](const auto &q) { return q.id == p.id; });
    if (it == next.end())
        next.push_back(std::move(p));
    else
        *it = std::move(p);
    return commit(next, error);
}
bool ProfileStore::remove(const QString &id, QString &error) {
    auto next = profiles_;
    next.erase(std::remove_if(next.begin(), next.end(), [&](const auto &p) { return p.id == id; }),
               next.end());
    return commit(next, error);
}
std::optional<Profile> ProfileStore::find(const QString &id) const {
    for (const auto &p : profiles_)
        if (p.id == id)
            return p;
    return std::nullopt;
}
bool ProfileStore::recordConnection(const QString &id, QString &error) {
    auto p = find(id);
    if (!p)
        return true;
    p->lastConnected = QDateTime::currentDateTimeUtc();
    return upsert(*p, error);
}
} // namespace velo
