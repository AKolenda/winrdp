// SPDX-License-Identifier: AGPL-3.0-only
#include "session_window.hpp"
#include "theme.hpp"
#include "window_chrome.hpp"
#include "graphics_options.hpp"
#include <QSizeGrip>
#include <QFile>
#include <QStringList>
#include <QKeySequence>
#include <QDir>
#include <QShortcut>
#include <QApplication>
#include <QCheckBox>
#include <QClipboard>
#include <QCloseEvent>
#include <QDialogButtonBox>
#include <QGuiApplication>
#include <QLabel>
#include <QMenu>
#include <QMessageBox>
#include <QMimeData>
#include <QPlainTextEdit>
#include <QProgressBar>
#include <QPushButton>
#include <QScreen>
#include <QStackedWidget>
#include <QStatusBar>
#include <QVBoxLayout>
#include <QWindow>
#include <algorithm>
namespace velo {
SessionWindow::SessionWindow(Profile profile, QString password, bool remember,
                             CredentialStore *secrets, QWidget *parent)
    : QMainWindow(parent), profile_(std::move(profile)), secrets_(secrets), remember_(remember) {
    setAttribute(Qt::WA_DeleteOnClose);
    setWindowTitle(profile_.name + " — Win RDP");
    setWindowIcon(icon("logo", Theme::current().accent));
    resize(1280, 840);
    setMinimumSize(640, 420);
    new WindowChrome(this,40);
    caption_ = WindowChrome::titleBar(this, profile_.name + " — Win RDP");
    setMenuWidget(caption_);
    statusBar()->setSizeGripEnabled(false);
    grip_ = new QSizeGrip(this); grip_->setObjectName("sessionResizeGrip"); grip_->setFixedSize(22,22);
    statusBar()->addPermanentWidget(grip_);
    auto *restore = new QShortcut(QKeySequence("Ctrl+Alt+Home"),this);
    connect(restore,&QShortcut::activated,this,[this] { if (auto *c = WindowChrome::get(window())) c->recover(); });
    if (remember_)
        pendingSecret_ = password;
    auto *root = new QWidget;
    auto *outer = new QVBoxLayout(root);
    outer->setContentsMargins(0, 0, 0, 0);
    outer->setSpacing(0);
    auto *toolbar = new QFrame;
    toolbar->setObjectName("toolbar");
    auto *bar = new QHBoxLayout(toolbar);
    bar->setContentsMargins(16, 10, 16, 10);
    auto *home = new QPushButton(icon("home", Theme::current().muted), "Computers");
    home->setProperty("quiet", true);
    home->setToolTip("Release input and return to your computer library");
    bar->addWidget(home);
    bar->addSpacing(14);
    bar->addWidget(label(profile_.name, "subheading"));
    status_ = label("Connecting", "muted");
    bar->addWidget(status_);
    bar->addStretch();
    auto *keys = new QPushButton(icon("keyboard", Theme::current().muted), "Keyboard");
    keys->setProperty("quiet", true);
    auto *menu = new QMenu(keys);
    auto *capture = menu->addAction("Capture keyboard (X11)");
    auto *release = menu->addAction("Release keyboard    Ctrl+Alt+Esc");
    menu->addSeparator();
    auto *cad = menu->addAction("Send Ctrl+Alt+Delete");
    auto *altTab = menu->addAction("Send Alt+Tab");
    auto *win = menu->addAction("Send Windows key");
    keys->setMenu(menu);
    bar->addWidget(keys);
    auto *full = new QPushButton(icon("fullscreen", Theme::current().muted), "Full screen");
    full->setProperty("quiet", true);
    full->setToolTip("Toggle full screen on this monitor · Ctrl+Alt+Enter. Recover window: Ctrl+Alt+Home.");
    bar->addWidget(full);
    tabButton_ = new QPushButton("Return to tab");
    tabButton_->setObjectName("sessionPopout");
    tabButton_->setProperty("quiet", true);
    tabButton_->setVisible(!profile_.allMonitors);
    bar->addWidget(tabButton_);
    connect(tabButton_, &QPushButton::clicked, this, [this] {
        if (view_) view_->releaseCapture();
        if (tabbed_) emit detachRequested(false);
        else emit attachRequested();
    });
    auto *info = new QPushButton(icon("info", Theme::current().muted), "Details");
    info->setProperty("quiet", true);
    bar->addWidget(info);
    disconnect_ = new QPushButton("Disconnect");
    bar->addWidget(disconnect_);
    outer->addWidget(toolbar);
    notice_ = label({}, "muted");
    notice_->setWordWrap(true);
    notice_->setContentsMargins(18, 8, 18, 8);
    notice_->hide();
    outer->addWidget(notice_);
    pages_ = new QStackedWidget;
    outer->addWidget(pages_, 1);
    setCentralWidget(root);
    auto *state = new QWidget;
    auto *stateLayout = new QVBoxLayout(state);
    stateLayout->setAlignment(Qt::AlignCenter);
    stateLayout->setContentsMargins(64, 48, 64, 48);
    auto *glyph = label({});
    glyph->setPixmap(icon("monitor", Theme::current().accent).pixmap(64, 64));
    glyph->setAlignment(Qt::AlignCenter);
    stateLayout->addWidget(glyph);
    stage_ = label("Connecting to " + profile_.name, "heading");
    stage_->setAlignment(Qt::AlignCenter);
    stage_->setWordWrap(true);
    stateLayout->addWidget(stage_);
    detail_ = label(profile_.address, "muted");
    detail_->setAlignment(Qt::AlignCenter);
    detail_->setWordWrap(true);
    stateLayout->addWidget(detail_);
    auto *progress = new QProgressBar;
    progress->setRange(0, 0);
    progress->setMaximumWidth(260);
    progress->setMaximumHeight(4);
    stateLayout->addWidget(progress, 0, Qt::AlignHCenter);
    retry_ = new QPushButton("Reconnect");
    retry_->setProperty("primary", true);
    retry_->hide();
    stateLayout->addWidget(retry_, 0, Qt::AlignHCenter);
    diagnostic_ = new QPlainTextEdit;
    diagnostic_->setReadOnly(true);
    diagnostic_->setMaximumHeight(150);
    diagnostic_->hide();
    stateLayout->addWidget(diagnostic_);
    pages_->addWidget(state);
    statusBar()->showMessage("Ctrl+Alt+Enter: full screen   ·   Ctrl+Alt+Esc: release input   ·   "
                             "Closing disconnects; it does not sign out.");
    connect(home, &QPushButton::clicked, this, [this] {
        if (view_)
            view_->releaseCapture();
        if (tabbed_) emit libraryRequested();
        else { showMinimized(); emit libraryRequested(); }
        for (auto w : companions_)
            if (w)
                w->showMinimized();
    });
    connect(full, &QPushButton::clicked, this, &SessionWindow::toggleFullscreen);
    connect(info, &QPushButton::clicked, this, &SessionWindow::showDiagnostics);
    connect(disconnect_, &QPushButton::clicked, this, [this] {
        if (session_ && session_->isRunning()) {
            disconnect_->setEnabled(false);
            status_->setText("Disconnecting");
            session_->stop();
        } else
            close();
    });
    connect(retry_, &QPushButton::clicked, this, [this] {
        if (session_ && session_->isRunning())
            return;
        emit reconnectRequested(profile_);
        allowClose_ = true;
        close();
    });
    connect(capture, &QAction::triggered, this, [this] {
        if (view_)
            view_->captureKeyboard();
    });
    connect(release, &QAction::triggered, this, [this] {
        if (view_)
            view_->releaseCapture();
    });
    connect(cad, &QAction::triggered, this, [this] {
        if (view_)
            view_->sendChord({0x1d, 0x38, 0x153});
    });
    connect(altTab, &QAction::triggered, this, [this] {
        if (view_)
            view_->sendChord({0x38, 0x0f});
    });
    connect(win, &QAction::triggered, this, [this] {
        if (view_)
            view_->sendChord({0x15b});
    });
    connect(qApp, &QApplication::focusChanged, this, [this](QWidget *, QWidget *) {
        QTimer::singleShot(0, this, &SessionWindow::syncClipboardFocus);
    });
    connect(qApp, &QGuiApplication::applicationStateChanged, this,
            [this](Qt::ApplicationState) { syncClipboardFocus(); });
    connect(QApplication::clipboard(), &QClipboard::dataChanged, this, [this] {
        if (!profile_.clipboard || !session_ || applyingClipboard_ || !activeClipboard_)
            return;
        const auto *mime = QApplication::clipboard()->mimeData();
        session_->setLocalClipboard(mime && mime->hasText() ? mime->text() : QString{});
    });
    connect(qApp, &QGuiApplication::screenRemoved, this, [this](QScreen *) {
        if (multiActive_) {
            closeCompanions();
            if (view_)
                view_->setSourceRect({});
            setNotice("A monitor was removed. Showing the entire desktop here. Reconnect to "
                      "negotiate the new layout.");
            showNormal();
        }
    });
    connect(&frameTimer_, &QTimer::timeout, this, [this] {
        if (!session_)
            return;
        auto frame = session_->takeFrame();
        if (!frame)
            return;
        if (multiActive_ && !multiMatched_ &&
            frame->size() == QSize(layout_.bounds.width, layout_.bounds.height)) {
            multiMatched_ = true;
            makeCompanions();
        }
        if (multiActive_ && multiMatched_ &&
            frame->size() != QSize(layout_.bounds.width, layout_.bounds.height)) {
            closeCompanions();
            view_->setSourceRect({});
            setNotice("Windows changed the desktop layout. The entire desktop is shown here; "
                      "reconnect to use all monitors.");
        }
        for (auto v : views_)
            if (v)
                v->setFrame(*frame);
    });
    QTimer::singleShot(0, this, [this, password = std::move(password), progress]() mutable {
        begin(std::move(password));
        if (session_) {
            connect(session_, &RdpSession::connected, progress, &QWidget::hide);
            connect(session_, &RdpSession::ended, progress, &QWidget::hide);
        } else
            progress->hide();
    });
}
SessionWindow::~SessionWindow() {
    frameTimer_.stop();
    closeCompanions();
    if (session_) {
        session_->stop();
        session_->wait();
    }
    pendingSecret_.fill(QChar(0));
}
bool SessionWindow::running() const {
    return session_ && session_->isRunning();
}
void SessionWindow::bringForward() {
    if (tabbed_) { emit activateRequested(); return; }
    if (isMinimized()) showNormal();
    raise();
    activateWindow();
    for (auto window : companions_)
        if (window)
            window->showFullScreen();
}
void SessionWindow::begin(QString password) {
    std::vector<core::Monitor> monitors;
    const auto all = QGuiApplication::screens();
    if (profile_.allMonitors && all.size() > 1) {
        for (auto *screen : all) {
            screens_.push_back(screen);
            const auto r = screen->geometry();
            const auto physical = screen->physicalSize();
            monitors.push_back({{r.x(), r.y(), r.width(), r.height()},
                                screen == QGuiApplication::primaryScreen(),
                                int(physical.width()),
                                int(physical.height())});
        }
        multiActive_ = true;
    } else {
        const auto size =
            core::safeDesktopSize(std::max(800, width()), std::max(600, height() - 100));
        monitors.push_back({{0, 0, size.first, size.second}, true, 310, 174});
    }
    layout_ = core::normalizeMonitors(std::move(monitors));
    if (!layout_) {
        stage_->setText("This monitor arrangement is not supported yet");
        detail_->setText(QString::fromStdString(layout_.error) +
                         " Edit this PC and turn off All monitors, then reconnect.");
        pendingSecret_.fill(QChar(0));
        pendingSecret_.clear();
        password.fill(QChar(0));
        retry_->show();
        disconnect_->setText("Close");
        return;
    }
    session_ = new RdpSession(profile_, std::move(password), layout_, this);
    view_ = new RdpView(session_);
    view_->setDynamicResize(!multiActive_);
    pages_->addWidget(view_);
    views_.push_back(view_);
    connect(view_, &RdpView::toggleFullscreenRequested, this, &SessionWindow::toggleFullscreen);
    connect(view_, &RdpView::recoverWindowRequested, this, [this] { if (auto *c = WindowChrome::get(window())) c->recover(); });
    connect(session_, &RdpSession::stageChanged, this, [this](const QString &state) {
        status_->setText(state);
        stage_->setText(state);
    });
    connect(session_, &RdpSession::connected, this, [this] {
        pages_->setCurrentWidget(view_);
        view_->setFocus();
        view_->setDynamicResize(!multiActive_);
        emit successfulConnection(profile_.id);
        syncClipboardFocus();
        if (remember_ && !pendingSecret_.isEmpty())
            secrets_->write(profile_.credentialKey(), pendingSecret_, this, [this](QString error) {
                if (!error.isEmpty())
                    setNotice("Connected, but the password was not saved: " + error);
            });
        pendingSecret_.fill(QChar(0));
        pendingSecret_.clear();
        if (profile_.fullscreen || multiActive_) {
            if (auto *c = WindowChrome::get(window())) c->enterFullscreen();
        }
        if (multiActive_)
            setNotice("All-monitor evaluation mode: logical monitor sizes, 100% remote scale. "
                      "Ctrl+Alt+Enter returns to one window.");
    });
    connect(session_, &RdpSession::featureChanged, this,
            [this](QString feature, QString state) { features_[feature] = state; });
    connect(session_, &RdpSession::warning, this, &SessionWindow::setNotice);
    connect(session_, &RdpSession::cursorChanged, this,
            [this](QImage image, QPoint hotspot, bool hidden) {
                for (auto v : views_)
                    if (v)
                        v->setRemoteCursor(image, hotspot, hidden);
            });
    connect(session_, &RdpSession::certificateRequested, this, &SessionWindow::showCertificate);
    connect(session_, &RdpSession::clipboardReceived, this, [this](const QString &text) {
        syncClipboardFocus();
        if (!activeClipboard_)
            return;
        applyingClipboard_ = true;
        QApplication::clipboard()->setText(text);
        applyingClipboard_ = false;
    });
    connect(session_, &RdpSession::ended, this,
            [this](QString message, QString diagnostic, bool user) {
                frameTimer_.stop();
                closeCompanions();
                view_->releaseCapture();
                view_->setFrame({});
                pendingSecret_.fill(QChar(0));
                pendingSecret_.clear();
                pages_->setCurrentIndex(0);
                stage_->setText(user ? "Disconnected" : "Session ended");
                status_->setText("Disconnected");
                detail_->setText(message);
                diagnostic_->setPlainText(diagnostic);
                disconnect_->setText("Close");
                disconnect_->setEnabled(true);
                retry_->setEnabled(false);
                retry_->show();
                if (isVisible() && window()->isFullScreen())
                    if (auto *c = WindowChrome::get(window())) c->leaveFullscreen();
            });
    connect(session_, &QThread::finished, this, [this] {
        retry_->setEnabled(true);
        if (closePending_) {
            allowClose_ = true;
            close();
        }
    });
    session_->start();
    frameTimer_.start(16);
}
void SessionWindow::makeCompanions() {
    if (!multiActive_ || !companions_.isEmpty())
        return;
    for (std::size_t i = 0; i < layout_.monitors.size(); ++i) {
        const auto &m = layout_.monitors[i];
        const QRect source(m.rect.x - layout_.bounds.x, m.rect.y - layout_.bounds.y, m.rect.width,
                           m.rect.height);
        if (m.primary) {
            view_->setSourceRect(source);
            if (i < std::size_t(screens_.size()) && screens_[int(i)]) {
                winId();
                windowHandle()->setScreen(screens_[int(i)]);
                setGeometry(screens_[int(i)]->geometry());
            }
            continue;
        }
        if (i >= std::size_t(screens_.size()) || !screens_[int(i)])
            continue;
        auto *window = new QWidget(nullptr, Qt::Window);
        window->setWindowTitle(profile_.name + " — Win RDP monitor");
        window->setAttribute(Qt::WA_DeleteOnClose);
        auto *l = new QVBoxLayout(window);
        l->setContentsMargins(0, 0, 0, 0);
        l->setSpacing(0);
        auto *hint = new QPushButton("Return to one window   ·   Ctrl+Alt+Enter");
        hint->setProperty("quiet", true);
        l->addWidget(hint);
        auto *v = new RdpView(session_, window);
        v->setSourceRect(source);
        v->setDynamicResize(false);
        l->addWidget(v, 1);
        views_.push_back(v);
        companions_.push_back(window);
        connect(hint, &QPushButton::clicked, this, &SessionWindow::toggleFullscreen);
        connect(v, &RdpView::toggleFullscreenRequested, this, &SessionWindow::toggleFullscreen);
        connect(v, &RdpView::recoverWindowRequested, this, [this] {
            if (multiActive_) toggleFullscreen();
            if (auto *c = WindowChrome::get(this)) c->recover();
        });
        // Closing an auxiliary window does not terminate its shared network session.
        connect(window, &QObject::destroyed, this,
                [this] { QTimer::singleShot(0, this, &SessionWindow::syncClipboardFocus); });
        window->winId();
        window->windowHandle()->setScreen(screens_[int(i)]);
        window->setGeometry(screens_[int(i)]->geometry());
        window->showFullScreen();
    }
}
void SessionWindow::closeCompanions() {
    const auto windows = companions_;
    companions_.clear();
    multiActive_ = false;
    for (auto w : windows)
        if (w) {
            w->close();
        }
    views_.clear();
    if (view_)
        views_.push_back(view_);
}
void SessionWindow::toggleFullscreen() {
    if (view_)
        view_->releaseCapture();
    // Fullscreen the existing toplevel: reparenting a live view can move it to
    // a different output and destroys window placement on Wayland/XWayland.
    if (tabbed_) { if (auto *c = WindowChrome::get(window())) c->toggleFullscreen(); return; }
    if (multiActive_) {
        closeCompanions();
        view_->setSourceRect({});
        view_->setDynamicResize(false);
        setNotice("Showing the full multi-monitor desktop in one window. Reconnect to enter "
                  "all-monitor mode again.");
        if (auto *c = WindowChrome::get(this)) c->leaveFullscreen();
    } else if (auto *c = WindowChrome::get(this)) c->toggleFullscreen();
}
void SessionWindow::syncClipboardFocus() {
    if (!session_ || !profile_.clipboard)
        return;
    auto *active = QApplication::activeWindow();
    bool belongs = tabbed_ ? (isVisible() && active == window()) : active == this;
    for (auto w : companions_)
        if (w && active == w)
            belongs = true;
    belongs = belongs && QGuiApplication::applicationState() == Qt::ApplicationActive;
    if (belongs != activeClipboard_) {
        activeClipboard_ = belongs;
        session_->setClipboardActive(belongs);
        if (belongs) {
            const auto *mime = QApplication::clipboard()->mimeData();
            session_->setLocalClipboard(mime && mime->hasText() ? mime->text() : QString{});
        }
    }
}
void SessionWindow::setNotice(const QString &message) {
    notice_->setText(message);
    notice_->setVisible(!message.isEmpty());
}
void SessionWindow::showCertificate(quint64 id, QString host, QString details, bool changed) {
    if (!session_ || !session_->isRunning())
        return;
    auto *dialog = new QDialog(this);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    dialog->setWindowTitle("Verify computer identity");
    dialog->resize(560, 500);
    dialog->setWindowModality(Qt::WindowModal);
    auto *l = new QVBoxLayout(dialog);
    l->setContentsMargins(26, 24, 26, 24);
    l->setSpacing(14);
    l->addWidget(
        label(changed ? "This computer's identity changed" : "Verify this computer", "heading"));
    auto *explanation =
        label("Before trusting " + host +
                  ", compare the fingerprint below with the certificate on that Windows PC or with "
                  "your administrator. A LAN or VPN does not replace identity verification.",
              "muted");
    explanation->setWordWrap(true);
    l->addWidget(explanation);
    auto *text = new QPlainTextEdit;
    text->setReadOnly(true);
    text->setPlainText(details);
    l->addWidget(text, 1);
    auto *checked = new QCheckBox("I verified this fingerprint through a trusted source");
    l->addWidget(checked);
    auto *buttons = new QDialogButtonBox;
    auto *reject = buttons->addButton("Cancel", QDialogButtonBox::RejectRole);
    auto *once = buttons->addButton("Trust once", QDialogButtonBox::ActionRole);
    auto *remember = buttons->addButton("Trust and remember", QDialogButtonBox::ActionRole);
    once->setEnabled(false);
    remember->setEnabled(false);
    reject->setDefault(true);
    l->addWidget(buttons);
    connect(checked, &QCheckBox::toggled, once, &QPushButton::setEnabled);
    connect(checked, &QCheckBox::toggled, remember, &QPushButton::setEnabled);
    connect(once, &QPushButton::clicked, dialog, [dialog] { dialog->done(2); });
    connect(remember, &QPushButton::clicked, dialog, [dialog] { dialog->done(1); });
    connect(reject, &QPushButton::clicked, dialog, &QDialog::reject);
    connect(dialog, &QDialog::finished, this, [this, id](int decision) {
        if (session_)
            session_->answerCertificate(id, decision);
    });
    connect(session_, &QThread::finished, dialog, &QDialog::reject);
    dialog->open();
}
void SessionWindow::showDiagnostics() {
    auto *dialog = new QDialog(this);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    dialog->setWindowTitle("Session details");
    dialog->resize(640, 460);
    auto *l = new QVBoxLayout(dialog);
    l->addWidget(label("Session details", "heading"));
    QString text = "FreeRDP " + RdpSession::backendVersion() +
                   "\nNative platform: " + QGuiApplication::platformName() +
                   "\nPresentation: " + (view_ ? view_->rendererDescription() : "not initialized") +
                   "\nH.264 decoder requested at startup: " + GraphicsOptions::decoderRequest() +
                   "\nDecoder activity: not inferred from GPU presence. See runtime.log for backend selection."
                   "\nPixel pipeline: FreeRDP GDI → owned CPU image → presentation (not zero-copy)\n";
    const QString logPath = QDir(qEnvironmentVariable("XDG_STATE_HOME", QDir::homePath()+"/.local/state"))
        .filePath("velordp/runtime.log");
    QFile runtimeLog(logPath);
    QStringList observations;
    if (runtimeLog.open(QIODevice::ReadOnly)) {
        if (runtimeLog.size() > 256*1024) runtimeLog.seek(runtimeLog.size()-256*1024);
        const auto lines = QString::fromUtf8(runtimeLog.readAll()).split('\n');
        for (const auto &line : lines)
            if (line.contains("WinRDP H264:")) observations.append(line.left(400));
    }
    text += "\nApp-wide decoder observations (may include other tabs):\n";
    text += observations.isEmpty() ? "No decoder events observed in the current log.\n" : observations.mid(std::max(0,int(observations.size())-12)).join('\n')+"\n";
    text += "Native log: " + logPath + "\n\n";
    for (auto i = features_.cbegin(); i != features_.cend(); ++i)
        text += i.key() + ": " + i.value() + "\n";
    text += "\n" + diagnostic_->toPlainText() + "\n\nBuild configuration:\n" +
            RdpSession::backendBuild();
    auto *display = new QPlainTextEdit;
    display->setPlainText(text);
    display->setReadOnly(true);
    l->addWidget(display);
    auto *b = new QDialogButtonBox(QDialogButtonBox::Close);
    connect(b, &QDialogButtonBox::rejected, dialog, &QDialog::reject);
    l->addWidget(b);
    dialog->open();
}
void SessionWindow::closeEvent(QCloseEvent *event) {
    if (allowClose_ || !running()) {
        closeCompanions();
        emit sessionClosing();
        event->accept();
        return;
    }
    event->ignore();
    if (closePending_)
        return;
    if (session_->isConnected() &&
        QMessageBox::question(
            this, "Disconnect?", "Disconnect from this PC? Your Windows apps will stay open.",
            QMessageBox::Yes | QMessageBox::No, QMessageBox::No) != QMessageBox::Yes)
        return;
    closePending_ = true;
    disconnect_->setEnabled(false);
    status_->setText("Disconnecting");
    session_->stop();
}
void SessionWindow::setTabMode(bool tabbed) {
    tabbed_ = tabbed;
    if (caption_) caption_->setVisible(!tabbed);
    if (grip_) grip_->setVisible(!tabbed);
    // The library toplevel supplies the tabbed window's resize grip.
    if (tabButton_) tabButton_->setText(tabbed ? "Pop out" : "Return to tab");
    refreshFocus();
}
void SessionWindow::refreshFocus() {
    if (view_ && !isVisible()) view_->releaseCapture();
    syncClipboardFocus();
    QTimer::singleShot(0, this, &SessionWindow::syncClipboardFocus);
}
void SessionWindow::requestShutdown() {
    if (view_) view_->releaseCapture();
    if (running()) {
        closePending_ = true;
        if (session_) session_->stop();
    } else {
        allowClose_ = true;
        close();
    }
}
} // namespace velo
