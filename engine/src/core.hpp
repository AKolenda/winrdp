// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include <algorithm>
#include <cstdint>
#include <mutex>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace velo::core {
struct Endpoint {
    std::string host;
    std::uint16_t port = 3389;
    std::string display() const;
};
struct EndpointResult {
    std::optional<Endpoint> value;
    std::string error;
};
EndpointResult parseEndpoint(std::string_view input);

struct Rect {
    int x = 0, y = 0, width = 0, height = 0;
};
struct Monitor {
    Rect rect;
    bool primary = false;
    int physicalWidth = 0, physicalHeight = 0;
};
struct Layout {
    std::vector<Monitor> monitors;
    Rect bounds;
    std::string error;
    explicit operator bool() const {
        return error.empty() && !monitors.empty();
    }
};
// Uses a single coordinate space: no independently scaled monitor origins.
Layout normalizeMonitors(std::vector<Monitor> monitors);
std::pair<int, int> safeDesktopSize(int width, int height);
struct MappedPoint {
    int x = 0, y = 0;
    bool inside = false;
};
MappedPoint mapPointer(int viewWidth, int viewHeight, Rect source, double x, double y);

// Both Qt's xcb and Wayland native keycodes use the XKB evdev+8 convention.
// The adapter only uses this mapping on these known platforms.
std::optional<std::uint32_t> evdevToScanCode(std::uint32_t evdev);
std::vector<std::uint8_t> encodeClipboard(std::u16string_view text);
std::optional<std::u16string> decodeClipboard(const std::uint8_t *bytes, std::size_t size);
inline constexpr std::size_t MaxClipboardBytes = 8 * 1024 * 1024;

// A one-slot mailbox. Producers replace stale frames, never queue a history.
// T must own its data (e.g. an independently copied QImage).
template <typename T> class LatestMailbox {
  public:
    void put(T value) {
        std::lock_guard<std::mutex> guard(mutex_);
        value_ = std::move(value);
        ++generation_;
    }
    std::optional<T> take() {
        std::lock_guard<std::mutex> guard(mutex_);
        auto result = std::move(value_);
        value_.reset();
        return result;
    }
    std::uint64_t generation() const {
        std::lock_guard<std::mutex> guard(mutex_);
        return generation_;
    }

  private:
    mutable std::mutex mutex_;
    std::optional<T> value_;
    std::uint64_t generation_ = 0;
};
} // namespace velo::core
