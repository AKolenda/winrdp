// SPDX-License-Identifier: AGPL-3.0-only
#include "theme.hpp"
#include <QApplication>
#include <QFontDatabase>
#include <QGraphicsDropShadowEffect>
#include <QLabel>
#include <QPainter>
#include <QPainterPath>
#include <QPalette>
#include <QPixmap>
#include <QPolygonF>
#include <cmath>
namespace velo {
static bool darkMode = false;
Theme Theme::current() {
    if (darkMode)
        return {"#202020", "#2b2b2b", "#202020", "#f3f3f3", "#b4b4b4",
                "#3d3d3d", "#60cdff", "#353535", "#35434b"};
    return {"#f3f3f3", "#ffffff", "#f3f3f3", "#1b1b1b", "#616161",
            "#e5e5e5", "#0067c0", "#f5f5f5", "#e4eff9"};
}
void Theme::apply(QApplication &app, bool dark) {
    darkMode = dark;
    app.setStyle("Fusion");
    const auto t = current();
    QPalette p;
    p.setColor(QPalette::Window, t.background);
    p.setColor(QPalette::WindowText, t.text);
    p.setColor(QPalette::Base, t.panel);
    p.setColor(QPalette::AlternateBase, t.hover);
    p.setColor(QPalette::Text, t.text);
    p.setColor(QPalette::Button, t.panel);
    p.setColor(QPalette::ButtonText, t.text);
    p.setColor(QPalette::Highlight, t.accent);
    p.setColor(QPalette::HighlightedText, dark ? QColor("#111b30") : QColor("white"));
    p.setColor(QPalette::PlaceholderText, t.muted);
    p.setColor(QPalette::ToolTipBase, t.panel);
    p.setColor(QPalette::ToolTipText, t.text);
    p.setColor(QPalette::Disabled, QPalette::Text, t.muted);
    p.setColor(QPalette::Disabled, QPalette::ButtonText, t.muted);
    app.setPalette(p);
    QFont f;
    const auto families = QFontDatabase::families();
    for (const auto &family : {QString("Segoe UI Variable"), QString("Segoe UI"), QString("Noto Sans"), QString("DejaVu Sans")})
        if (families.contains(family)) {
            f.setFamily(family);
            break;
        }
    f.setPointSize(10);
    app.setFont(f);
    auto css = QString(R"(
        QWidget { color:%1; }
        QMainWindow, QDialog { background:%2; }
        QLabel { background:transparent; }
        QLabel[role="title"] { font-size:26px; font-weight:600; }
        QLabel[role="heading"] { font-size:20px; font-weight:600; }
        QLabel[role="subheading"] { font-size:14px; font-weight:600; }
        QLabel[role="muted"] { color:%3; }
        QLabel[role="eyebrow"] { color:%3; font-size:10px; font-weight:650; }
        QLabel[role="error"] { color:%10; }
        QFrame#sidebar { background:%4; border-right:1px solid %5; }
        QFrame#panel, QFrame#computerCard { background:%6; border:1px solid %5; border-radius:8px; }
        QFrame#computerCard:hover { border-color:%7; }
        QFrame#toolbar { background:%6; border-bottom:1px solid %5; }
        QPushButton, QToolButton { background:%6; border:1px solid %5; border-radius:5px; padding:7px 13px; min-height:18px; }
        QPushButton:hover, QToolButton:hover { background:%8; border-color:%7; }
        QPushButton:pressed, QToolButton:pressed { background:%9; }
        QPushButton:focus, QToolButton:focus { border:2px solid %7; padding:6px 12px; }
        QPushButton:disabled, QToolButton:disabled { color:%3; background:%2; }
        QPushButton[primary="true"] { background:%7; color:%11; border:1px solid %7; font-weight:600; }
        QPushButton[primary="true"]:hover { background:%12; }
        QPushButton[quiet="true"], QToolButton[quiet="true"] { background:transparent; border:1px solid transparent; }
        QPushButton[quiet="true"]:hover, QToolButton[quiet="true"]:hover { background:%8; }
        QPushButton[nav="true"] { text-align:left; background:transparent; border:1px solid transparent; border-radius:8px; padding:9px 12px; }
        QPushButton[nav="true"]:hover { background:%8; }
        QPushButton[nav="true"]:checked { background:%9; color:%7; font-weight:600; }
        QLineEdit, QComboBox { background:%6; border:1px solid %5; border-radius:7px; padding:10px 12px; min-height:18px; selection-background-color:%7; }
        QLineEdit:focus, QComboBox:focus { border:2px solid %7; padding:9px 11px; }
        QComboBox::drop-down { width:24px; border:0; }
        QComboBox QAbstractItemView { background:%6; color:%1; selection-background-color:%9; }
        QCheckBox { spacing:9px; padding:5px 0; }
        QCheckBox::indicator { width:18px; height:18px; }
        QScrollArea { background:transparent; border:none; }
        QScrollBar:vertical { background:transparent; width:9px; margin:2px; }
        QScrollBar::handle:vertical { background:%5; border-radius:3px; min-height:30px; }
        QScrollBar::add-line:vertical,QScrollBar::sub-line:vertical { height:0; }
        QScrollBar::add-page:vertical,QScrollBar::sub-page:vertical { background:none; }
        QMenu { background:%6; border:1px solid %5; border-radius:8px; padding:5px; }
        QMenu::item { padding:8px 24px; border-radius:5px; }
        QMenu::item:selected { background:%9; }
        QMenu::separator { background:%5; height:1px; margin:4px 10px; }
        QTabWidget#sessionTabs::pane { border:0; background:%6; }
        QTabWidget#sessionTabs > QTabBar::tab { min-width:125px; padding:10px 18px; border:0; border-top-left-radius:8px; border-top-right-radius:8px; }
        QTabWidget#sessionTabs > QTabBar::tab:selected { background:%6; color:%1; }
        QTabWidget::pane { border:1px solid %5; border-radius:8px; background:%6; }
        QTabBar::tab { background:transparent; border-bottom:2px solid transparent; padding:12px 18px; }
        QTabBar::tab:selected { color:%7; border-bottom-color:%7; }
        QTreeWidget { background:%6; border:1px solid %5; border-radius:5px; outline:none; }
        QTreeWidget::item { padding:4px 10px; border:0; }
        QTreeWidget::item:hover { background:%8; }
        QTreeWidget::item:selected { background:%9; color:%1; }
        QHeaderView::section { background:%6; color:%3; padding:9px 10px; border:0; border-bottom:1px solid %5; text-align:left; }
        QFrame#windowTitleBar { background:%4; border-bottom:1px solid %5; }
        QStatusBar { background:%4; border-top:1px solid %5; min-height:22px; font-size:11px; color:%3; }
        QStatusBar::item { border:none; }
        QTabBar::close-button { width:16px; height:16px; border-radius:3px; }
        QTabBar::close-button:hover { background:%8; }
        QPushButton#newSession { border:none; padding:0; min-height:0; }
        QToolTip { background:%6; color:%1; border:1px solid %5; padding:6px; }
    )")
                   .arg(t.text.name())
                   .arg(t.background.name())
                   .arg(t.muted.name())
                   .arg(t.sidebar.name())
                   .arg(t.border.name())
                   .arg(t.panel.name())
                   .arg(t.accent.name())
                   .arg(t.hover.name())
                   .arg(t.selected.name())
                   .arg(dark ? "#ffb4a9" : "#b42335")
                   .arg(dark ? "#112144" : "#ffffff")
                   .arg(dark ? "#a4bfff" : "#1d4ed8");
    app.setStyleSheet(css);
}
QIcon icon(const QString &name, const QColor &requested) {
    const auto color = requested.isValid() ? requested : Theme::current().muted;
    QPixmap pix(64, 64);
    pix.fill(Qt::transparent);
    pix.setDevicePixelRatio(2);
    QPainter p(&pix);
    p.setRenderHint(QPainter::Antialiasing);
    p.scale(32.0 / 24.0, 32.0 / 24.0);
    p.setPen(QPen(color, 1.65, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    p.setBrush(Qt::NoBrush);
    if (name == "monitor" || name == "computers") {
        p.drawRoundedRect(QRectF(3, 4, 18, 13), 2, 2);
        p.drawLine(QPointF(12, 17), QPointF(12, 21));
        p.drawLine(QPointF(8, 21), QPointF(16, 21));
    } else if (name == "plus") {
        p.drawLine(12, 5, 12, 19);
        p.drawLine(5, 12, 19, 12);
    } else if (name == "search") {
        p.drawEllipse(QRectF(4, 4, 12, 12));
        p.drawLine(14, 14, 20, 20);
    } else if (name == "arrow") {
        p.drawLine(4, 12, 20, 12);
        p.drawLine(14, 6, 20, 12);
        p.drawLine(14, 18, 20, 12);
    } else if (name == "star" || name == "star-filled") {
        QPainterPath path;
        for (int i = 0; i < 10; ++i) {
            const double a = -1.57079632679 + i * 0.31415926536 * 2;
            const double r = (i % 2) ? 4 : 9;
            const QPointF q(12 + std::cos(a) * r, 12 + std::sin(a) * r);
            if (!i)
                path.moveTo(q);
            else
                path.lineTo(q);
        }
        path.closeSubpath();
        if (name == "star-filled")
            p.setBrush(color);
        p.drawPath(path);
    } else if (name == "clock") {
        p.drawEllipse(QRectF(3, 3, 18, 18));
        p.drawLine(12, 6, 12, 12);
        p.drawLine(12, 12, 16, 14);
    } else if (name == "more") {
        p.setBrush(color);
        p.setPen(Qt::NoPen);
        for (int x : {5, 12, 19})
            p.drawEllipse(QPointF(x, 12), 1.5, 1.5);
    } else if (name == "lock") {
        p.drawRoundedRect(QRectF(5, 10, 14, 11), 2, 2);
        p.drawArc(QRectF(8, 3, 8, 13), 0, 180 * 16);
        p.drawLine(12, 14, 12, 17);
    } else if (name == "fullscreen") {
        for (const QPointF q : {QPointF(4, 4), QPointF(20, 4), QPointF(4, 20), QPointF(20, 20)}) {
            p.drawLine(q, q + QPointF(q.x() < 12 ? 5 : -5, 0));
            p.drawLine(q, q + QPointF(0, q.y() < 12 ? 5 : -5));
        }
    } else if (name == "close") {
        p.drawLine(6, 6, 18, 18);
        p.drawLine(6, 18, 18, 6);
    } else if (name == "keyboard") {
        p.drawRoundedRect(QRectF(2, 6, 20, 13), 2, 2);
        for (int y : {10, 13})
            for (int x : {6, 10, 14, 18})
                p.drawPoint(x, y);
        p.drawLine(7, 16, 17, 16);
    } else if (name == "sound") {
        QPolygonF q;
        q << QPointF(3, 9) << QPointF(7, 9) << QPointF(12, 5) << QPointF(12, 19) << QPointF(7, 15)
          << QPointF(3, 15);
        p.drawPolygon(q);
        p.drawArc(QRectF(11, 7, 8, 10), -65 * 16, 130 * 16);
        p.drawArc(QRectF(11, 3, 13, 18), -65 * 16, 130 * 16);
    } else if (name == "settings") {
        p.drawEllipse(QRectF(8, 8, 8, 8));
        for (int i = 0; i < 8; ++i) {
            p.save();
            p.translate(12, 12);
            p.rotate(i * 45);
            p.drawLine(0, 7, 0, 10);
            p.restore();
        }
    } else if (name == "check") {
        p.drawLine(5, 12, 10, 17);
        p.drawLine(10, 17, 20, 6);
    } else if (name == "home") {
        p.drawLine(3, 11, 12, 3);
        p.drawLine(12, 3, 21, 11);
        p.drawRect(QRectF(6, 10, 12, 11));
    } else if (name == "logo") {
        p.drawRoundedRect(QRectF(3, 4, 18, 13), 2, 2);
        p.drawLine(QPointF(12, 17), QPointF(12, 21));
        p.drawLine(QPointF(8, 21), QPointF(16, 21));
        p.drawLine(7, 8, 16, 8);
        p.drawLine(14, 6, 16, 8);
        p.drawLine(14, 10, 16, 8);
        p.drawLine(17, 13, 8, 13);
        p.drawLine(10, 11, 8, 13);
        p.drawLine(10, 15, 8, 13);
    } else if (name == "windows") {
        p.setPen(Qt::NoPen);
        p.setBrush(color);
        for (int x : {3, 13})
            for (int y : {3, 13})
                p.drawRect(x, y, 8, 8);
    } else {
        p.drawEllipse(QRectF(3, 3, 18, 18));
        p.drawLine(12, 10, 12, 17);
        p.drawPoint(12, 7);
    }
    return QIcon(pix);
}
QLabel *label(const QString &text, const char *role, QWidget *parent) {
    auto *l = new QLabel(text, parent);
    l->setProperty("role", role);
    l->setTextFormat(Qt::PlainText);
    return l;
}
void shadow(QWidget *w, int blur, int alpha) {
    auto *s = new QGraphicsDropShadowEffect(w);
    s->setBlurRadius(blur);
    s->setOffset(0, 4);
    s->setColor(QColor(20, 40, 80, alpha));
    w->setGraphicsEffect(s);
}
} // namespace velo
