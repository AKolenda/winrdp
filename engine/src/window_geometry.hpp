// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include <algorithm>
namespace velo::geometry {
struct Rect { int x, y, width, height; };
// Window placement is in Qt logical coordinates. Negative origins are valid.
// Never use the union of all outputs for a single-monitor fullscreen window.
inline Rect clamp(Rect r, Rect screen) {
    screen.width = std::max(1, screen.width);
    screen.height = std::max(1, screen.height);
    r.width = std::clamp(r.width, 1, screen.width);
    r.height = std::clamp(r.height, 1, screen.height);
    r.x = std::clamp(r.x, screen.x, screen.x + screen.width - r.width);
    r.y = std::clamp(r.y, screen.y, screen.y + screen.height - r.height);
    return r;
}
inline Rect centered(Rect screen, int width = 1240, int height = 820) {
    width = std::clamp(width, 1, std::max(1, screen.width));
    height = std::clamp(height, 1, std::max(1, screen.height));
    return {screen.x + (screen.width-width)/2, screen.y+(screen.height-height)/2, width, height};
}
} // namespace velo::geometry
