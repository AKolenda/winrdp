// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include <QString>
namespace velo {
struct GraphicsOptions {
    // Called once on the GUI thread, before any FreeRDP workers exist. These
    // environment variables are process-wide: never mutate them per session.
    static void applyDecoderEnvironment();
    static QString decoderRequest();
    static bool useOpenGL();
};
}
