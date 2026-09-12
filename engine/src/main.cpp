// SPDX-License-Identifier: AGPL-3.0-only
#include "main_window.hpp"
#include "theme.hpp"
#include "window_chrome.hpp"
#include "graphics_options.hpp"
#include <QApplication>
#include <QCommandLineParser>
#include <QDir>
#include <QFile>
#include <QLockFile>
#include <QMessageBox>
#include <QPixmap>
#include <QSettings>
#include <QStandardPaths>
#include <QStringList>
#include <QTemporaryDir>
#include <QTimer>
#include <memory>
#ifndef VELO_VERSION
#define VELO_VERSION "0.4.0"
#endif
int main(int argc, char **argv) {
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    QApplication app(argc, argv);
    QGuiApplication::setApplicationDisplayName("Win RDP");
    // Stable identity preserves profiles, keychain entries and existing settings.
    QCoreApplication::setApplicationName("Velo");
    QCoreApplication::setOrganizationName("io.velordp");
    QCoreApplication::setApplicationVersion(VELO_VERSION);
    QGuiApplication::setDesktopFileName("io.velordp.Velo");
    QCommandLineParser parser;
    parser.setApplicationDescription(
        "A focused native FreeRDP client for Linux. Evaluation build.");
    parser.addHelpOption();
    parser.addOption({"settings", "Open Settings."});
    parser.addOption({"host", "Open this computer's remote-access setup."});
    parser.addVersionOption();
    parser.addOption({"demo", "Show sample computers without permitting network connections."});
    parser.addOption({"connect",
                      "Open the sign-in dialog for this PC (never accepts a password argument).",
                      "address"});
    parser.addOption(
        {"screenshot", "Save a native UI screenshot and exit; requires --demo.", "path"});
    parser.process(app);
    if (parser.isSet("screenshot") && !parser.isSet("demo")) {
        qCritical("--screenshot requires --demo");
        return 2;
    }
    velo::GraphicsOptions::applyDecoderEnvironment();
    velo::Theme::apply(app, QSettings().value("appearance/dark", false).toBool());
    QTemporaryDir demoDirectory;
    QString directory = parser.isSet("demo")
                            ? demoDirectory.path()
                            : QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    if (directory.isEmpty() || !QDir().mkpath(directory)) {
        QMessageBox::critical(nullptr, "Win RDP", "Could not create the configuration directory.");
        return 1;
    }
    QFile::setPermissions(directory,
                          QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
    QLockFile lock(QDir(directory).filePath("library.lock"));
    lock.setStaleLockTime(30000);
    if (!lock.tryLock(100)) {
        QMessageBox::information(nullptr, "Win RDP is already open",
                                 "Another Win RDP instance is using this computer library. Close it "
                                 "before starting a second instance.");
        return 1;
    }
    velo::ProfileStore store(directory);
    QString error;
    const bool loaded = store.load(error);
    if (parser.isSet("demo")) {
        const QStringList names = {"Office PC", "Studio workstation", "Home desktop",
                                   "Windows lab"},
                          addresses = {"192.168.1.24", "studio.local", "192.168.1.80", "10.8.0.12"},
                          users = {"OFFICE\\alex", "alex", "HOME\\alex", "LAB\\administrator"},
                          groups = {"Work", "Work", "Personal", "Lab"};
        for (int i = 0; i < names.size(); ++i) {
            auto p = velo::Profile::create();
            p.name = names[i];
            p.address = addresses[i];
            p.username = users[i];
            p.group = groups[i];
            p.favorite = i < 2;
            p.lastConnected = QDateTime::currentDateTimeUtc().addSecs(-3600 * (i + 1));
            if (!store.upsert(p, error)) {
                qCritical("Could not create demo profiles.");
                return 1;
            }
        }
    }
    velo::MainWindow window(&store, parser.isSet("demo"));
    if (!parser.isSet("demo")) {
        const auto geometry = QSettings().value("window/geometry").toByteArray();
        if (!geometry.isEmpty())
            window.restoreGeometry(geometry);
    }
    QObject::connect(&app, &QCoreApplication::aboutToQuit, &window, [&] {
        if (!parser.isSet("demo"))
            QSettings().setValue("window/geometry", window.saveGeometry());
    });
    window.show();
    // Restore old geometry only inside a real current output.
    if (auto *chrome = velo::WindowChrome::get(&window)) chrome->ensureVisible();
    if (parser.isSet("settings")) window.openSettings();
    if (parser.isSet("host")) QTimer::singleShot(0, &window, &velo::MainWindow::openHost);
    if (!loaded)
        QTimer::singleShot(0, &window, [&] {
            QMessageBox::warning(&window, "Your library was preserved", error);
        });
    if (parser.isSet("connect")) {
        auto p = velo::Profile::create();
        p.address = parser.value("connect");
        p.name = p.address;
        QTimer::singleShot(0, &window, [&window, p] {
            const auto error = p.validationError();
            if (!error.isEmpty())
                QMessageBox::warning(&window, "Check the address", error);
            else
                window.connectTo(p);
        });
    }
    if (parser.isSet("screenshot"))
        QTimer::singleShot(400, &window, [&] {
            const bool ok = window.grab().save(parser.value("screenshot"));
            app.exit(ok ? 0 : 1);
        });
    return app.exec();
}
