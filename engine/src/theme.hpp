// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include <QColor>
#include <QIcon>
#include <QString>
class QApplication;
class QLabel;
class QWidget;
namespace velo {
struct Theme {
    QColor background, panel, sidebar, text, muted, border, accent, hover, selected;
    static Theme current();
    static void apply(QApplication &app, bool dark);
};
QIcon icon(const QString &name, const QColor &color = {});
QLabel *label(const QString &text, const char *role = "", QWidget *parent = nullptr);
void shadow(QWidget *widget, int blur = 24, int alpha = 16);
} // namespace velo
