// SPDX-License-Identifier: AGPL-3.0-only
#include "graphics_options.hpp"
#include <QSettings>
#include <QStringList>
namespace velo {
void GraphicsOptions::applyDecoderEnvironment() {
    if (!qEnvironmentVariableIsSet("WINRDP_HWDECODER")) {
        QString mode = QSettings().value("graphics/decoder", "software").toString();
        if (!QStringList{"software","auto","cuda","vaapi"}.contains(mode)) mode = "software";
        qputenv("WINRDP_HWDECODER", mode.toLatin1());
    }
}
QString GraphicsOptions::decoderRequest() {
    return qEnvironmentVariable("WINRDP_HWDECODER", "software");
}
bool GraphicsOptions::useOpenGL() {
    const QString request = qEnvironmentVariable("WINRDP_RENDERER", QSettings().value("graphics/renderer","auto").toString());
    return request != "software";
}
}
