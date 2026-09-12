// SPDX-License-Identifier: AGPL-3.0-only
#include "main_window.hpp"
#include "session_window.hpp"
#include "theme.hpp"
#include "window_chrome.hpp"
#include "graphics_options.hpp"
#include <QSizeGrip>
#include <QScreen>
#include <QWindow>
#include <QApplication>
#include <QCloseEvent>
#include <QTabBar>
#include <QInputDialog>
#include <QFrame>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFormLayout>
#include <QGridLayout>
#include <QGroupBox>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPushButton>
#include <QResizeEvent>
#include <QScrollArea>
#include <QSet>
#include <QSettings>
#include <QShortcut>
#include <QSignalBlocker>
#include <QTabWidget>
#include <QTimer>
#include <QVBoxLayout>
#include <QTreeWidget>
#include <QHeaderView>
#include <QStackedWidget>
#include <QProcess>
#include <QFileInfo>
#include <QCoreApplication>
#include <QDir>
#include <QStatusBar>
#include <QStyle>
#include <algorithm>
namespace velo {
ProfileDialog::ProfileDialog(Profile p, bool adding, QWidget *parent)
    : QDialog(parent), original_(std::move(p)) {
    setWindowTitle(adding ? "Add a PC" : "Edit PC");
    setObjectName("profileDialog");
    setMinimumWidth(530);
    auto *outer = new QVBoxLayout(this);
    outer->setContentsMargins(28, 26, 28, 24);
    outer->setSpacing(17);
    outer->addWidget(label(adding ? "New connection" : "Connection settings", "heading"));
    auto *tabs = new QTabWidget;
    outer->addWidget(tabs);
    auto *general = new QWidget;
    auto *form = new QFormLayout(general);
    form->setContentsMargins(20, 22, 20, 22);
    form->setSpacing(14);
    name_ = new QLineEdit(original_.name);
    name_->setObjectName("profileName");
    name_->setMaxLength(100);
    name_->setPlaceholderText("Office PC");
    address_ = new QLineEdit(original_.address);
    address_->setObjectName("profileAddress");
    address_->setMaxLength(320);
    address_->setPlaceholderText("192.168.1.20 or office-pc");
    username_ = new QLineEdit(original_.username);
    username_->setObjectName("profileUsername");
    username_->setMaxLength(256);
    username_->setPlaceholderText("Optional — asked when connecting");
    group_ = new QLineEdit(original_.group);
    group_->setMaxLength(80);
    group_->setPlaceholderText("Personal");
    form->addRow("PC name", name_);
    form->addRow("Address", address_);
    form->addRow("Username", username_);
    form->addRow("Group", group_);
    auto *addressHint = label("Custom port: pc-name:3390   ·   IPv6: [::1]:3389", "muted");
    form->addRow({}, addressHint);
    tabs->addTab(general, "Connection");
    auto *display = new QWidget;
    auto *dl = new QVBoxLayout(display);
    dl->setContentsMargins(20, 18, 20, 20);
    dl->setSpacing(10);
    fullscreen_ = new QCheckBox("Start in full screen");
    fullscreen_->setChecked(original_.fullscreen);
    dl->addWidget(fullscreen_);
    monitors_ = new QCheckBox("Use all my monitors — evaluation");
    monitors_->setChecked(original_.allMonitors);
    dl->addWidget(monitors_);
    auto *multiNote = label(
        "All monitors share one Windows session. This first build uses logical monitor sizes at "
        "100% remote scale. Mixed-DPI, fractional scaling, and hot-plug changes need validation.",
        "muted");
    multiNote->setWordWrap(true);
    dl->addWidget(multiNote);
    dl->addWidget(label("Remote graphics", "subheading"));
    graphics_ = new QComboBox;
    graphics_->addItem("Automatic", "auto");
    graphics_->addItem("H.264 / AVC420 · motion", "avc420");
    graphics_->addItem("H.264 / AVC444 · text clarity", "avc444");
    graphics_->setCurrentIndex(std::max(0,graphics_->findData(original_.graphics)));
    dl->addWidget(graphics_);
    compatibility_ = new QCheckBox("Compatibility graphics");
    compatibility_->setChecked(original_.compatibility);
    dl->addWidget(compatibility_);
    auto *graphicsNote = label("Try this if the desktop is black or corrupted. It disables the "
                               "modern graphics channel; it does not weaken authentication.",
                               "muted");
    graphicsNote->setWordWrap(true);
    dl->addWidget(graphicsNote);
    dl->addStretch();
    tabs->addTab(display, "Display");
    auto *sharing = new QWidget;
    auto *sl = new QVBoxLayout(sharing);
    sl->setContentsMargins(20, 18, 20, 20);
    sl->setSpacing(12);
    clipboard_ = new QCheckBox("Share text clipboard");
    clipboard_->setChecked(original_.clipboard);
    sl->addWidget(clipboard_);
    auto *clipNote = label("Only while this session is active. Files and images are not supported. "
                           "Anything copied locally may become available to the remote PC.",
                           "muted");
    clipNote->setWordWrap(true);
    sl->addWidget(clipNote);
    audio_ = new QCheckBox("Play Windows audio on this computer");
    audio_->setChecked(original_.audio);
    sl->addWidget(audio_);
    keyboard_ = new QComboBox;
    keyboard_->addItem("English (US)", quint32(0x409));
    keyboard_->addItem("English (UK)", quint32(0x809));
    keyboard_->addItem("Canadian Multilingual Standard", quint32(0x11009));
    keyboard_->addItem("French (Canada)", quint32(0x1009));
    keyboard_->addItem("French", quint32(0x40c));
    keyboard_->addItem("German", quint32(0x407));
    keyboard_->addItem("Spanish", quint32(0x40a));
    int index = keyboard_->findData(original_.keyboardLayout);
    if (index < 0) {
        keyboard_->addItem("Custom: 0x" + QString::number(original_.keyboardLayout, 16),
                           original_.keyboardLayout);
        index = keyboard_->count() - 1;
    }
    keyboard_->setCurrentIndex(index);
    sl->addWidget(label("Windows keyboard layout", "subheading"));
    sl->addWidget(keyboard_);
    sl->addStretch();
    tabs->addTab(sharing, "Sharing & input");
    error_ = label({}, "error");
    error_->setObjectName("profileError");
    error_->setWordWrap(true);
    error_->hide();
    outer->addWidget(error_);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Cancel | QDialogButtonBox::Save);
    buttons->button(QDialogButtonBox::Save)->setObjectName("saveProfile");
    buttons->button(QDialogButtonBox::Save)->setProperty("primary", true);
    buttons->button(QDialogButtonBox::Save)->setText(adding ? "Add PC" : "Save changes");
    outer->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] {
        auto p = profile();
        auto error = p.validationError();
        if (!error.isEmpty()) {
            error_->setText(error);
            error_->show();
            return;
        }
        accept();
    });
    name_->setFocus();
}
Profile ProfileDialog::profile() const {
    auto p = original_;
    p.name = name_->text().trimmed();
    p.address = address_->text().trimmed();
    p.username = username_->text().trimmed();
    p.group = group_->text().trimmed();
    p.clipboard = clipboard_->isChecked();
    p.audio = audio_->isChecked();
    p.allMonitors = monitors_->isChecked();
    p.fullscreen = fullscreen_->isChecked();
    p.compatibility = compatibility_->isChecked();
    p.graphics = graphics_->currentData().toString();
    p.keyboardLayout = keyboard_->currentData().toUInt();
    return p;
}
MainWindow::MainWindow(ProfileStore *store, bool demo, QWidget *parent)
    : QMainWindow(parent), store_(store), secrets_(new CredentialStore(this)), demo_(demo) {
    setWindowTitle("Win RDP");
    setWindowIcon(icon("logo", Theme::current().accent));
    setObjectName("mainWindow");
    resize(1240, 820);
    setMinimumSize(800, 520);
    new WindowChrome(this);

    tabs_ = new QTabWidget(this);
    tabs_->setObjectName("sessionTabs");
    tabs_->setDocumentMode(true);
    tabs_->setTabsClosable(true);
    tabs_->setMovable(false); // The library is always tab zero.
    setCentralWidget(tabs_);
    library_ = new QWidget;
    auto *body = new QHBoxLayout(library_);
    body->setContentsMargins(0, 0, 0, 0);
    body->setSpacing(0);
    tabs_->addTab(library_, icon("monitor"), "Computers");
    tabs_->tabBar()->setTabButton(0, QTabBar::RightSide, nullptr);
    tabs_->tabBar()->setTabButton(0, QTabBar::LeftSide, nullptr);
    auto *newTab = new QPushButton(icon("plus"), {});
    newTab->setObjectName("newSession");
    newTab->setToolTip("Connect to a computer");
    newTab->setAccessibleName("New remote session");
    auto *caption = new QWidget;
    auto *captionLayout = new QHBoxLayout(caption);
    captionLayout->setContentsMargins(0,0,0,0); captionLayout->setSpacing(0);
    newTab->setFixedSize(38,36); newTab->setProperty("quiet",true);
    captionLayout->addWidget(newTab);
    captionLayout->addWidget(WindowChrome::controls(this, caption));
    tabs_->setCornerWidget(caption, Qt::TopRightCorner);
    auto *logo = label({}); logo->setPixmap(icon("logo",Theme::current().accent).pixmap(22,22));
    logo->setFixedSize(40,40); logo->setAlignment(Qt::AlignCenter);
    logo->setProperty("windowDragArea",true);
    tabs_->setCornerWidget(logo, Qt::TopLeftCorner);
    statusBar()->setSizeGripEnabled(false);
    auto *grip = new QSizeGrip(this); grip->setObjectName("windowResizeGrip"); grip->setFixedSize(20,20);
    statusBar()->addPermanentWidget(grip);
    connect(newTab, &QPushButton::clicked, this, &MainWindow::quickConnect);
    connect(tabs_, &QTabWidget::tabCloseRequested, this, [this](int index) {
        if (index > 0) {
            if (auto *session = qobject_cast<SessionWindow *>(tabs_->widget(index)))
                session->close();
        }
    });
    connect(tabs_, &QTabWidget::currentChanged, this, [this](int) {
        for (auto session : sessions_)
            if (session) session->refreshFocus();
    });

    auto *sidebar = new QFrame;
    sidebar->setObjectName("sidebar");
    sidebar->setFixedWidth(204);
    auto *nav = new QVBoxLayout(sidebar);
    nav->setContentsMargins(12, 22, 12, 14);
    nav->setSpacing(5);
    auto *brand = new QHBoxLayout;
    auto *brandIcon = label({});
    brandIcon->setPixmap(icon("logo", Theme::current().accent).pixmap(30, 30));
    brand->addWidget(brandIcon);
    brand->addWidget(label("Win RDP", "subheading"));
    brand->addStretch();
    nav->addLayout(brand);
    nav->addSpacing(16);
    const QStringList titles = {"Computers", "Favourites", "Recent"};
    const QStringList icons = {"monitor", "star", "clock"};
    for (int i = 0; i < titles.size(); ++i) {
        auto *button = new QPushButton(icon(icons[i]), titles[i]);
        button->setProperty("nav", true);
        button->setCheckable(true);
        button->setObjectName("nav" + QString::number(i));
        navigationButtons_.push_back(button);
        nav->addWidget(button);
        connect(button, &QPushButton::clicked, this, [this, i] {
            tabs_->setCurrentWidget(library_);
            navigation_ = i;
            pages_->setCurrentIndex(0);
            for (int j = 0; j < navigationButtons_.size(); ++j)
                navigationButtons_[j]->setChecked(j == i);
            rebuild();
        });
    }
    auto *host = new QPushButton(icon("home"), "This PC");
    host->setObjectName("hostSettings");
    host->setProperty("nav", true);
    nav->addSpacing(12);
    nav->addWidget(host);
    connect(host, &QPushButton::clicked, this, &MainWindow::openHost);
    nav->addStretch();
    auto *settings = new QPushButton(icon("settings"), "Settings");
    settings->setObjectName("openSettings");
    settings->setProperty("nav", true);
    nav->addWidget(settings);
    connect(settings, &QPushButton::clicked, this, &MainWindow::showSettings);
    auto *appearance = new QPushButton("Light / dark");
    appearance->setObjectName("toggleAppearance");
    appearance->setProperty("nav", true);
    nav->addWidget(appearance);
    connect(appearance, &QPushButton::clicked, this, [this] {
        const bool dark = Theme::current().background.lightness() >= 128;
        QSettings().setValue("appearance/dark", dark);
        Theme::apply(*qApp, dark);
        if (auto *box = findChild<QCheckBox *>("darkAppearance")) {
            const QSignalBlocker block(box);
            box->setChecked(dark);
        }
        rebuild();
    });
    nav->addSpacing(12);
    nav->addWidget(label(QStringLiteral("Win RDP 0.4.0"), "muted"));
    body->addWidget(sidebar);
    auto *right = new QWidget;
    auto *rightLayout = new QVBoxLayout(right);
    rightLayout->setContentsMargins(0, 0, 0, 0);
    rightLayout->setSpacing(0);
    banner_ = label({}, "error");
    banner_->setContentsMargins(26, 10, 26, 10);
    banner_->setWordWrap(true);
    banner_->hide();
    rightLayout->addWidget(banner_);
    if (demo_) {
        auto *notice = label("Interface demo — connections disabled", "muted");
        notice->setObjectName("demoNotice");
        notice->setContentsMargins(26, 8, 26, 8);
        rightLayout->addWidget(notice);
    }
    pages_ = new QStackedWidget;
    rightLayout->addWidget(pages_, 1);
    body->addWidget(right, 1);
    auto *libraryPage = new QWidget;
    auto *page = new QVBoxLayout(libraryPage);
    page->setContentsMargins(28, 22, 28, 16);
    page->setSpacing(18);
    auto *commands = new QHBoxLayout;
    heading_ = label("Computers", "title");
    commands->addWidget(heading_);
    commands->addStretch();
    auto *searchButton = new QPushButton(icon("search"), {});
    searchButton->setToolTip("Find a saved computer · Ctrl+F");
    searchButton->setAccessibleName("Find computer");
    commands->addWidget(searchButton);
    auto *add = new QPushButton(icon("plus"), "New");
    add->setObjectName("addComputer");
    commands->addWidget(add);
    connect(add, &QPushButton::clicked, this, [this] { edit(defaultProfile(), true); });
    auto selected = [this]() -> std::optional<Profile> {
        const auto *item = tree_->currentItem();
        return item ? store_->find(item->data(0, Qt::UserRole).toString()) : std::nullopt;
    };
    auto *editButton = new QPushButton("Edit");
    editButton->setObjectName("editComputer");
    commands->addWidget(editButton);
    connect(editButton, &QPushButton::clicked, this, [this, selected] {
        if (auto p = selected()) edit(*p, false);
    });
    auto *favorite = new QPushButton(icon("star"), "Favourite");
    commands->addWidget(favorite);
    connect(favorite, &QPushButton::clicked, this, [this, selected] {
        if (auto p = selected()) {
            p->favorite = !p->favorite;
            QString error;
            if (!store_->upsert(*p, error)) report(error);
        }
    });
    auto *more = new QPushButton(QStringLiteral("···"));
    more->setAccessibleName("More computer actions");
    auto *moreMenu = new QMenu(more);
    auto *remove = moreMenu->addAction("Remove computer");
    auto *quick = moreMenu->addAction("Quick connect…");
    more->setMenu(moreMenu);
    commands->addWidget(more);
    connect(remove, &QAction::triggered, this, [this, selected] {
        if (auto p = selected()) removeProfile(*p);
    });
    connect(quick, &QAction::triggered, this, &MainWindow::quickConnect);
    page->addLayout(commands);
    search_ = new QLineEdit;
    search_->setObjectName("searchComputers");
    search_->setPlaceholderText("Find a saved computer");
    search_->setClearButtonEnabled(true);
    search_->hide();
    page->addWidget(search_);
    auto showSearch = [this] {
        tabs_->setCurrentWidget(library_);
        pages_->setCurrentIndex(0);
        search_->show(); search_->setFocus(); search_->selectAll();
    };
    connect(searchButton, &QPushButton::clicked, this, showSearch);
    auto *filter = new QHBoxLayout;
    count_ = label({}, "muted");
    filter->addWidget(count_);
    filter->addStretch();
    groups_ = new QComboBox;
    groups_->setObjectName("groupFilter");
    groups_->setMinimumWidth(130);
    filter->addWidget(groups_);
    page->addLayout(filter);
    tree_ = new QTreeWidget;
    tree_->setObjectName("computerList");
    tree_->setRootIsDecorated(false);
    tree_->setUniformRowHeights(true);
    tree_->setSelectionMode(QAbstractItemView::SingleSelection);
    tree_->setEditTriggers(QAbstractItemView::NoEditTriggers);
    tree_->setAllColumnsShowFocus(true);
    tree_->setHeaderLabels({"Name", "Address", "Username", "Display", "Group"});
    tree_->header()->setStretchLastSection(false);
    tree_->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    for (int i = 1; i < 5; ++i) tree_->header()->setSectionResizeMode(i, QHeaderView::Interactive);
    tree_->setColumnWidth(1,200); tree_->setColumnWidth(2,160);
    tree_->setColumnWidth(3,125); tree_->setColumnWidth(4,120);
    tree_->setContextMenuPolicy(Qt::CustomContextMenu);
    page->addWidget(tree_, 1);
    empty_ = label("No saved computers. Choose New to add one.", "muted");
    empty_->setAlignment(Qt::AlignCenter);
    page->addWidget(empty_);
    auto *bottom = new QHBoxLayout;
    selection_ = label("Select a computer", "muted");
    bottom->addWidget(selection_, 1);
    auto *connectButton = new QPushButton(icon("monitor", Qt::white), "Connect");
    connectButton->setObjectName("connectComputer");
    connectButton->setProperty("primary", true);
    connectButton->setMinimumWidth(128);
    bottom->addWidget(connectButton);
    page->addLayout(bottom);
    pages_->addWidget(libraryPage);
    pages_->addWidget(makeSettingsPage());
    pages_->addWidget(makeHostPage());
    auto connectSelected = [this, selected] { if (auto p = selected()) connectTo(*p); };
    connect(connectButton, &QPushButton::clicked, this, connectSelected);
    connect(tree_, &QTreeWidget::itemActivated, this,
            [connectSelected](QTreeWidgetItem *, int) { connectSelected(); });
    connect(tree_, &QTreeWidget::itemSelectionChanged, this,
            [this, selected, editButton, favorite, connectButton] {
        const auto p = selected();
        editButton->setEnabled(p.has_value());
        favorite->setEnabled(p.has_value());
        connectButton->setEnabled(p.has_value());
        selection_->setText(p ? p->name + "  ·  " + p->address : "Select a computer");
    });
    connect(tree_, &QWidget::customContextMenuRequested, this, [this, selected](QPoint pos) {
        auto *item = tree_->itemAt(pos);
        if (!item) return;
        tree_->setCurrentItem(item);
        auto p = selected();
        if (!p) return;
        QMenu menu(this);
        auto *connectAction = menu.addAction("Connect");
        auto *editAction = menu.addAction("Edit connection");
        auto *forgetAction = menu.addAction("Forget password");
        menu.addSeparator();
        auto *removeAction = menu.addAction("Remove");
        const auto *chosen = menu.exec(tree_->viewport()->mapToGlobal(pos));
        if (chosen == connectAction) connectTo(*p);
        else if (chosen == editAction) edit(*p, false);
        else if (chosen == forgetAction) forget(*p);
        else if (chosen == removeAction) removeProfile(*p);
    });
    connect(search_, &QLineEdit::textChanged, this, [this] {
        pages_->setCurrentIndex(0); rebuild();
    });
    connect(groups_, &QComboBox::currentTextChanged, this, &MainWindow::rebuild);
    connect(store_, &ProfileStore::changed, this, [this] { refreshGroups(); rebuild(); });
    auto *findShortcut = new QShortcut(QKeySequence::Find, library_);
    findShortcut->setContext(Qt::WidgetWithChildrenShortcut);
    connect(findShortcut, &QShortcut::activated, this, showSearch);
    auto *newShortcut = new QShortcut(QKeySequence("Ctrl+N"), library_);
    newShortcut->setContext(Qt::WidgetWithChildrenShortcut);
    connect(newShortcut, &QShortcut::activated, this, [this] { edit(defaultProfile(), true); });
    refreshGroups();
    rebuild();
    if (QSettings().value("startup/settings", false).toBool()) openSettings();
    else navigationButtons_.front()->setChecked(true);
}
Profile MainWindow::defaultProfile() const {
    auto p = Profile::create();
    const auto mode = QSettings().value("connection/display", "window").toString();
    p.fullscreen = mode != "window";
    p.allMonitors = mode == "monitors";
    p.clipboard = QSettings().value("connection/clipboard", true).toBool();
    p.audio = QSettings().value("connection/audio", true).toBool();
    return p;
}
void MainWindow::report(const QString &error) {
    banner_->setText(error);
    banner_->setVisible(!error.isEmpty());
}
void MainWindow::refreshGroups() {
    const auto old = groups_->currentText();
    QSignalBlocker block(groups_);
    groups_->clear();
    groups_->addItem("All groups");
    QSet<QString> groups;
    for (const auto &p : store_->profiles())
        groups.insert(p.group);
    auto names = groups.values();
    std::sort(names.begin(), names.end());
    groups_->addItems(names);
    const auto i = groups_->findText(old);
    if (i >= 0)
        groups_->setCurrentIndex(i);
}
void MainWindow::rebuild() {
    const auto selectedId = tree_->currentItem()
        ? tree_->currentItem()->data(0, Qt::UserRole).toString() : QString{};
    QSignalBlocker blocked(tree_);
    tree_->clear();
    auto profiles = store_->profiles();
    if (navigation_ == 2)
        std::stable_sort(profiles.begin(), profiles.end(), [](const auto &a, const auto &b) {
            return a.lastConnected > b.lastConnected;
        });
    const auto query = search_->text().trimmed();
    visibleCount_ = 0;
    for (const auto &p : profiles) {
        if (navigation_ == 1 && !p.favorite) continue;
        if (navigation_ == 2 && !p.lastConnected.isValid()) continue;
        if (groups_->currentIndex() > 0 && p.group != groups_->currentText()) continue;
        if (!query.isEmpty() && !(p.name + " " + p.address + " " + p.username + " " + p.group)
                                     .contains(query, Qt::CaseInsensitive)) continue;
        auto *item = new QTreeWidgetItem(tree_,
            {p.name, p.address, p.username.isEmpty() ? "Ask when connecting" : p.username,
             p.allMonitors ? "All monitors" : p.fullscreen ? "Full screen" : "Window", p.group});
        item->setIcon(0, icon(p.favorite ? "star-filled" : "monitor", Theme::current().accent));
        item->setData(0, Qt::UserRole, p.id);
        item->setSizeHint(0, QSize(0, 46));
        for (int col = 0; col < 5; ++col) item->setToolTip(col, item->text(col));
        if (p.id == selectedId) tree_->setCurrentItem(item);
        ++visibleCount_;
    }
    count_->setText(QString::number(visibleCount_) + (visibleCount_ == 1 ? " connection" : " connections"));
    empty_->setVisible(!visibleCount_);
    if (!tree_->currentItem() && tree_->topLevelItemCount() > 0)
        tree_->setCurrentItem(tree_->topLevelItem(0));
    if (selection_) {
        const auto *current = tree_->currentItem();
        selection_->setText(current ? current->text(0) + "  ·  " + current->text(1)
                                    : QStringLiteral("Select a computer"));
    }
    if (auto *button = findChild<QPushButton *>("connectComputer"))
        button->setEnabled(tree_->currentItem() != nullptr);
    if (auto *button = findChild<QPushButton *>("editComputer"))
        button->setEnabled(tree_->currentItem() != nullptr);
    heading_->setText(navigation_ == 1 ? "Favourites" : navigation_ == 2 ? "Recent" : "Computers");
}
void MainWindow::edit(Profile profile, bool adding) {
    ProfileDialog dialog(profile, adding, this);
    if (dialog.exec() != QDialog::Accepted)
        return;
    const auto updated = dialog.profile();
    QString error;
    if (!store_->upsert(updated, error)) {
        report(error);
        return;
    }
    report({});
    if (!adding && updated.credentialKey() != profile.credentialKey())
        forget(profile);
}
void MainWindow::forget(const Profile &profile) {
    secrets_->forget(profile.credentialKey(), this, [this](QString error) { report(error); });
}
void MainWindow::removeProfile(Profile profile) {
    if (QMessageBox::question(
            this, "Remove this PC?",
            "Remove this saved computer? This does not affect the Windows PC or close its apps.",
            QMessageBox::Yes | QMessageBox::No, QMessageBox::No) != QMessageBox::Yes)
        return;
    QString error;
    if (!store_->remove(profile.id, error)) {
        report(error);
        return;
    }
    forget(profile);
}
void MainWindow::connectTo(Profile profile) {
    report({});
    if (demo_) {
        QMessageBox::information(this, "Design demo",
                                 "This mode uses sample computers and never opens a network "
                                 "connection. Launch Win RDP without --demo to use real RDP.");
        return;
    }
    if (auto window = sessions_.value(profile.id)) {
        if (window->running()) {
            window->bringForward();
            return;
        }
        window->close();
    }
    LoginDialog login(profile, secrets_, this);
    if (!store_->find(profile.id))
        login.disableRemember();
    if (login.exec() != QDialog::Accepted)
        return;
    const auto oldKey = profile.credentialKey();
    profile.username = login.username();
    if (store_->find(profile.id)) {
        QString error;
        if (!store_->upsert(profile, error)) {
            report(error);
            return;
        }
        if (oldKey != profile.credentialKey())
            secrets_->forget(oldKey, this, [this](QString error) {
                if (!error.isEmpty())
                    report(error);
            });
    }
    auto *window = new SessionWindow(profile, login.password(), login.remember(), secrets_);
    sessions_[profile.id] = window;
    connect(window, &SessionWindow::successfulConnection, this, [this](QString id) {
        QString error;
        if (!store_->recordConnection(id, error))
            report(error);
    });
    connect(window, &SessionWindow::reconnectRequested, this,
            [this](Profile p) { QTimer::singleShot(0, this, [this, p] { connectTo(p); }); });
    connect(window, &SessionWindow::activateRequested, this, [this, window] {
        const int index = tabs_->indexOf(window);
        if (index >= 0) tabs_->setCurrentIndex(index);
        showNormal(); raise(); activateWindow();
    });
    connect(window, &SessionWindow::libraryRequested, this, [this] {
        tabs_->setCurrentWidget(library_); showNormal(); raise(); activateWindow();
    });
    connect(window, &SessionWindow::detachRequested, this, [this, window](bool fullscreen) {
        detachSession(window, fullscreen);
    });
    connect(window, &SessionWindow::attachRequested, this, [this, window] {
        attachSession(window);
    });
    connect(window, &SessionWindow::sessionClosing, this, [this, window] {
        const int index = tabs_->indexOf(window);
        if (index > 0) tabs_->removeTab(index);
    });
    connect(window, &QObject::destroyed, this, [this, id = profile.id] {
        if (!sessions_.value(id)) sessions_.remove(id);
        if (closing_) QTimer::singleShot(0, this, &MainWindow::finishClosing);
    });
    if (profile.allMonitors) window->show();
    else attachSession(window);
}
void MainWindow::resizeEvent(QResizeEvent *event) {
    QMainWindow::resizeEvent(event);
}
bool MainWindow::eventFilter(QObject *object, QEvent *event) {
    if (event->type() == QEvent::MouseButtonDblClick) {
        auto *mouse = static_cast<QMouseEvent *>(event);
        if (mouse->button() == Qt::LeftButton) {
            auto p = store_->find(object->property("profileId").toString());
            if (p) {
                connectTo(*p);
                return true;
            }
        }
    }
    return QMainWindow::eventFilter(object, event);
}
void MainWindow::showShortcuts() {
    QMessageBox::information(
        this, "Keyboard shortcuts",
        "Computer library\nCtrl+N — Add a PC\nCtrl+F — Search\n\nInside a session\nCtrl+Alt+Enter "
        "— Toggle full screen\nCtrl+Alt+Home — Recover window\nCtrl+Alt+Esc — Release keyboard and held keys\n\nUse the Keyboard "
        "menu to send Ctrl+Alt+Delete, Alt+Tab, or the Windows key. Desktop-reserved shortcuts may "
        "remain local on Wayland. Keyboard capture is available on X11.\n\nClick inside the "
        "desktop to send input. Focus changes release held keys and mouse buttons.");
}
void MainWindow::showSettings() { openSettings(); }
void MainWindow::openSettings() {
    tabs_->setCurrentWidget(library_);
    pages_->setCurrentIndex(1);
    for (auto *button : navigationButtons_) button->setChecked(false);
}
void MainWindow::openHost() {
    tabs_->setCurrentWidget(library_);
    pages_->setCurrentIndex(2);
    for (auto *button : navigationButtons_) button->setChecked(false);
}
void MainWindow::launchSetup(const QString &page) {
    if (demo_) {
        QMessageBox::information(this, "Interface demo", "Host changes are disabled in demo mode.");
        return;
    }
    const QString installed = page == "host" ? "/usr/lib/velordp/setup/host_window.py" : "/usr/lib/velordp/setup/velo_setup.py";
    const QString source = QDir(QCoreApplication::applicationDirPath()).filePath(page == "host" ? "../setup/host_window.py" : "../setup/velo_setup.py");
    const QString script = QFileInfo::exists(installed) ? installed : source;
    if (!QFileInfo::exists(script) ||
        !QProcess::startDetached("/usr/bin/python3", {script, "--page", page})) {
        QMessageBox::warning(this, "Setup unavailable",
            "Install the Win RDP .deb to manage this PC and components graphically. "
            "The RDP client itself does not need administrator access.");
    }
}
QWidget *MainWindow::makeSettingsPage() {
    auto *scroll = new QScrollArea;
    scroll->setWidgetResizable(true);
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    layout->setContentsMargins(32, 26, 32, 24);
    layout->setSpacing(18);
    layout->addWidget(label("Settings", "title"));
    auto section = [layout](const QString &title) {
        auto *frame = new QFrame;
        frame->setObjectName("panel");
        auto *inner = new QVBoxLayout(frame);
        inner->setContentsMargins(20, 16, 20, 16);
        inner->setSpacing(12);
        inner->addWidget(label(title, "subheading"));
        layout->addWidget(frame);
        return inner;
    };
    auto *connection = section("New connections");
    auto *form = new QFormLayout;
    auto *display = new QComboBox;
    display->setObjectName("defaultDisplay");
    display->addItem("Window", "window");
    display->addItem("Full screen", "full");
    display->addItem("All monitors (experimental)", "monitors");
    display->setCurrentIndex(std::max(0, display->findData(QSettings().value("connection/display", "window"))));
    form->addRow("Open desktop in", display);
    connection->addLayout(form);
    connect(display, &QComboBox::currentIndexChanged, page, [display] {
        QSettings().setValue("connection/display", display->currentData());
    });
    auto checkbox = [page](QVBoxLayout *l, const QString &text, const QString &key, bool fallback) {
        auto *box = new QCheckBox(text);
        box->setChecked(QSettings().value(key, fallback).toBool());
        l->addWidget(box);
        QObject::connect(box, &QCheckBox::toggled, page, [key](bool checked) {
            QSettings().setValue(key, checked);
        });
        return box;
    };
    checkbox(connection, "Share text clipboard", "connection/clipboard", true);
    checkbox(connection, "Play remote audio on this computer", "connection/audio", true);
    auto *graphics = section("Graphics");
    auto *graphicsForm = new QFormLayout;
    auto *renderer = new QComboBox;
    renderer->setObjectName("graphicsRenderer");
    renderer->addItem("Automatic · OpenGL presentation", "auto");
    renderer->addItem("Software · compatibility", "software");
    renderer->setCurrentIndex(std::max(0,renderer->findData(QSettings().value("graphics/renderer","auto"))));
    graphicsForm->addRow("Presentation",renderer);
    connect(renderer,&QComboBox::currentIndexChanged,page,[renderer] { QSettings().setValue("graphics/renderer",renderer->currentData()); });
    auto *decoder = new QComboBox;
    decoder->setObjectName("graphicsDecoder");
    decoder->addItem("Software · recommended for this evaluation", "software");
    decoder->addItem("Automatic hardware · experimental", "auto");
    decoder->addItem("NVIDIA NVDEC / CUDA · experimental", "cuda");
    decoder->addItem("AMD / Intel VA-API · experimental", "vaapi");
    decoder->setCurrentIndex(std::max(0,decoder->findData(QSettings().value("graphics/decoder","software"))));
    graphicsForm->addRow("H.264 decoding",decoder);
    connect(decoder,&QComboBox::currentIndexChanged,page,[decoder] { QSettings().setValue("graphics/decoder",decoder->currentData()); });
    graphics->addLayout(graphicsForm);
    auto *graphicsNote = label("Presentation applies to new sessions. Restart Win RDP after changing the decoder. Hardware decoding requires the Win RDP private backend and working GPU drivers; system drivers are never installed automatically. Session Details reports presentation and requested decoding separately.","muted");
    graphicsNote->setWordWrap(true); graphics->addWidget(graphicsNote);
    auto *appearance = section("Application");
    checkbox(appearance, "Open Settings at startup", "startup/settings", false);
    auto *dark = checkbox(appearance, "Dark appearance", "appearance/dark", false);
    dark->setObjectName("darkAppearance");
    connect(dark, &QCheckBox::toggled, this, [this](bool enabled) {
        Theme::apply(*qApp, enabled); rebuild();
    });
    auto *host = section("This PC");
    auto *hostText = label("Allow Windows Remote Desktop to control this desktop.", "muted");
    hostText->setWordWrap(true);
    host->addWidget(hostText);
    auto *configure = new QPushButton("Set up remote access");
    host->addWidget(configure, 0, Qt::AlignLeft);
    connect(configure, &QPushButton::clicked, this, &MainWindow::openHost);
    auto *components = section("Components");
    components->addWidget(label("FreeRDP " + RdpSession::backendVersion() + "   ·   Qt " + qVersion(), "muted"));
    auto *manage = new QPushButton("Manage components");
    components->addWidget(manage, 0, Qt::AlignLeft);
    connect(manage, &QPushButton::clicked, this, [this] { launchSetup("components"); });
    auto *security = label(CredentialStore::available()
        ? "Saved passwords use the desktop keyring."
        : "Keyring support is unavailable; passwords are not saved.", "muted");
    security->setWordWrap(true);
    layout->addWidget(security);
    layout->addStretch();
    scroll->setWidget(page);
    return scroll;
}

void MainWindow::quickConnect() {
    bool accepted = false;
    const QString address = QInputDialog::getText(this, "Connect to a computer", "IP address or computer name",
                                                 QLineEdit::Normal, {}, &accepted).trimmed();
    if (!accepted || address.isEmpty()) return;
    auto p = defaultProfile();
    p.address = address;
    p.name = address;
    const auto error = p.validationError();
    if (!error.isEmpty()) { report(error); tabs_->setCurrentWidget(library_); return; }
    connectTo(p);
}
void MainWindow::attachSession(SessionWindow *session) {
    if (!session) return;
    if (session->usesAllMonitors()) { session->bringForward(); return; }
    session->hide();
    if (session->isFullScreen()) session->showNormal();
    session->hide();
    session->setParent(tabs_, Qt::Widget);
    session->setTabMode(true);
    int index = tabs_->indexOf(session);
    if (index < 0) {
        index = tabs_->addTab(session, icon("monitor"), session->computerName());
        auto *closeTab = new CaptionButton(CaptionButton::Close,session,tabs_);
        closeTab->setFixedSize(24,24); closeTab->setToolTip("Disconnect session");
        closeTab->setAccessibleName("Disconnect " + session->computerName());
        tabs_->tabBar()->setTabButton(index,QTabBar::RightSide,closeTab);
    }
    tabs_->setCurrentIndex(index);
    session->show();
    session->refreshFocus();
}
void MainWindow::detachSession(SessionWindow *session, bool fullscreen) {
    if (!session) return;
    QScreen *output = windowHandle() ? windowHandle()->screen() : screen();
    const int index = tabs_->indexOf(session);
    session->hide();
    if (index > 0) tabs_->removeTab(index);
    session->setParent(nullptr, Qt::Window | Qt::FramelessWindowHint);
    session->setTabMode(false);
    auto *chrome = WindowChrome::get(session);
    if (chrome) chrome->placeOn(output, size());
    session->show();
    if (fullscreen && chrome) chrome->enterFullscreen(output);
    session->raise(); session->activateWindow();
    session->refreshFocus();
}
void MainWindow::closeEvent(QCloseEvent *event) {
    bool running = false;
    for (auto session : sessions_)
        if (session && session->running()) running = true;
    if (running && !closing_ && QMessageBox::question(this, "Close Win RDP?",
            "Disconnect all remote sessions and close Win RDP? Your remote applications stay open.",
            QMessageBox::Yes | QMessageBox::No, QMessageBox::No) != QMessageBox::Yes) {
        event->ignore(); return;
    }
    closing_ = true;
    const auto all = sessions_.values();
    for (auto session : all) if (session) session->requestShutdown();
    if (running) {
        event->ignore();
        QTimer::singleShot(100, this, &MainWindow::finishClosing);
    } else event->accept();
}
void MainWindow::finishClosing() {
    if (!closing_) return;
    for (auto session : sessions_) {
        if (session && session->running()) {
            QTimer::singleShot(100, this, &MainWindow::finishClosing);
            return;
        }
    }
    close();
}
QWidget *MainWindow::makeHostPage() {
    auto *scroll = new QScrollArea;
    scroll->setWidgetResizable(true);
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    layout->setContentsMargins(32, 26, 32, 24);
    layout->setSpacing(18);
    layout->addWidget(label("This PC", "heading"));
    auto card = [layout](const QString &title, const QString &description) {
        auto *frame = new QFrame;
        frame->setObjectName("panel");
        auto *contents = new QVBoxLayout(frame);
        contents->setContentsMargins(22, 18, 22, 18);
        contents->setSpacing(12);
        contents->addWidget(label(title, "subheading"));
        auto *text = label(description, "muted");
        text->setWordWrap(true);
        contents->addWidget(text);
        layout->addWidget(frame);
        return contents;
    };
    auto *share = card("Remote access", "Connect from Windows using this PC’s address and remote-access credentials. "
                      "Incoming access is configured separately from outgoing sessions.");
    auto *configure = new QPushButton("Configure this PC");
    configure->setProperty("primary", true);
    configure->setObjectName("configureHost");
    share->addWidget(configure, 0, Qt::AlignLeft);
    connect(configure, &QPushButton::clicked, this, [this] { launchSetup("host"); });
    card("When someone is signed in", "Desktop sharing shows the current desktop. Locking or signing out ends this sharing mode.");
    auto *login = card("When nobody is signed in", "Remote login requires a supported GNOME system service. "
                     "Use the Remote Login section in System Settings. This is not seamless takeover of a locally open desktop.");
    auto *system = new QPushButton("Open system remote-login settings");
    login->addWidget(system, 0, Qt::AlignLeft);
    connect(system, &QPushButton::clicked, this, [this] {
        if (demo_) { report("System changes are disabled in the interface demo."); return; }
        if (!QProcess::startDetached("gnome-control-center", {"system", "remote-desktop"}))
            report("System Settings could not be opened. Remote Login may not be available on this OS version.");
    });
    card("Local network / VPN", "Allow devices on your trusted LAN or VPN. A username and password are still required. "
                              "Win RDP does not configure your router or expose RDP to the internet.");
    layout->addStretch();
    scroll->setWidget(page);
    return scroll;
}
} // namespace velo
