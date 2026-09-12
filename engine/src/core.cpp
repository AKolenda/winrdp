// SPDX-License-Identifier: AGPL-3.0-only
#include "core.hpp"
#include <arpa/inet.h>
#include <charconv>
#include <cmath>
#include <limits>

namespace velo::core {
namespace {
bool space(unsigned char c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\r';
}
bool alnum(unsigned char c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9');
}
bool ipv6(std::string host) {
    const auto zone = host.find('%');
    if (zone != std::string::npos) {
        const auto suffix = host.substr(zone + 1);
        if (suffix.empty() || suffix.size() > 64 ||
            !std::all_of(suffix.begin(), suffix.end(), [](unsigned char c) {
                return alnum(c) || c == '_' || c == '-' || c == '.';
            }))
            return false;
        host.resize(zone);
    }
    in6_addr addr{};
    return inet_pton(AF_INET6, host.c_str(), &addr) == 1;
}
bool hostname(const std::string &host) {
    if (host.empty() || host.size() > 253)
        return false;
    if (std::all_of(host.begin(), host.end(),
                    [](char c) { return (c >= '0' && c <= '9') || c == '.'; }) &&
        host.find('.') != std::string::npos) {
        in_addr addr{};
        return inet_pton(AF_INET, host.c_str(), &addr) == 1;
    }
    std::size_t start = 0;
    while (start < host.size()) {
        auto end = host.find('.', start);
        if (end == std::string::npos)
            end = host.size();
        const auto n = end - start;
        if (!n || n > 63 || !alnum(host[start]) || !alnum(host[end - 1]))
            return false;
        for (auto i = start; i < end; ++i)
            if (!alnum(host[i]) && host[i] != '-')
                return false;
        start = end + 1;
    }
    return true;
}
long long right(const Rect &r) {
    return static_cast<long long>(r.x) + r.width;
}
long long bottom(const Rect &r) {
    return static_cast<long long>(r.y) + r.height;
}
bool overlaps(const Rect &a, const Rect &b) {
    return std::max<long long>(a.x, b.x) < std::min(right(a), right(b)) &&
           std::max<long long>(a.y, b.y) < std::min(bottom(a), bottom(b));
}
bool adjacent(const Rect &a, const Rect &b) {
    return ((right(a) == b.x || right(b) == a.x) &&
            std::max<long long>(a.y, b.y) < std::min(bottom(a), bottom(b))) ||
           ((bottom(a) == b.y || bottom(b) == a.y) &&
            std::max<long long>(a.x, b.x) < std::min(right(a), right(b)));
}
} // namespace
std::string Endpoint::display() const {
    auto s = host.find(':') != std::string::npos ? "[" + host + "]" : host;
    if (port != 3389)
        s += ":" + std::to_string(port);
    return s;
}
EndpointResult parseEndpoint(std::string_view input) {
    while (!input.empty() && space(input.front()))
        input.remove_prefix(1);
    while (!input.empty() && space(input.back()))
        input.remove_suffix(1);
    auto fail = [](std::string message) {
        return EndpointResult{std::nullopt, std::move(message)};
    };
    if (input.empty())
        return fail("Enter a computer name or IP address.");
    if (input.size() > 320)
        return fail("That address is too long.");
    for (unsigned char c : input)
        if (c <= 32 || c == 127 || c >= 128)
            return fail("Use a hostname or IP address without spaces. Use punycode for "
                        "international hostnames.");
    if (input.find_first_of("/@\\?#") != std::string::npos)
        return fail("Enter just the computer address, not a URL or a username.");
    Endpoint ep;
    std::string_view port;
    bool explicitPort = false;
    if (input.front() == '[') {
        const auto end = input.find(']');
        if (end == std::string_view::npos)
            return fail("Close the IPv6 address with ].");
        ep.host = input.substr(1, end - 1);
        if (!ipv6(ep.host))
            return fail("That IPv6 address is not valid.");
        if (end + 1 < input.size()) {
            if (input[end + 1] != ':')
                return fail("Use [IPv6 address]:port.");
            port = input.substr(end + 2);
            explicitPort = true;
        }
    } else {
        const auto colonCount = std::count(input.begin(), input.end(), ':');
        if (colonCount > 1) {
            ep.host = input;
            if (!ipv6(ep.host))
                return fail(
                    "That IPv6 address is not valid. Use [address]:port for a custom port.");
        } else {
            const auto split = input.find(':');
            ep.host = input.substr(0, split);
            if (split != std::string_view::npos) {
                port = input.substr(split + 1);
                explicitPort = true;
            }
            if (!hostname(ep.host))
                return fail("That computer name or IPv4 address is not valid.");
        }
    }
    if (explicitPort) {
        unsigned n = 0;
        if (port.empty())
            return fail("Enter a port number after the colon.");
        const auto result = std::from_chars(port.data(), port.data() + port.size(), n);
        if (result.ec != std::errc{} || result.ptr != port.data() + port.size() || n < 1 ||
            n > 65535)
            return fail("The port must be between 1 and 65535.");
        ep.port = static_cast<std::uint16_t>(n);
    }
    return {ep, {}};
}
std::pair<int, int> safeDesktopSize(int width, int height) {
    width = std::clamp(width, 200, 8192);
    height = std::clamp(height, 200, 8192);
    // Display Control requires an even width. Keep the last column local if odd.
    return {width & ~1, height};
}
Layout normalizeMonitors(std::vector<Monitor> monitors) {
    Layout result;
    auto fail = [&](std::string text) {
        result.error = std::move(text);
        return result;
    };
    if (monitors.empty() || monitors.size() > 16)
        return fail("Use between 1 and 16 monitors.");
    if (std::count_if(monitors.begin(), monitors.end(),
                      [](const Monitor &m) { return m.primary; }) != 1)
        return fail("Exactly one monitor must be primary.");
    for (const auto &m : monitors) {
        if (m.rect.width < 200 || m.rect.width > 8192 || m.rect.height < 200 ||
            m.rect.height > 8192 || (m.rect.width & 1))
            return fail("Monitor sizes must be 200–8192 pixels with an even width. Adjust the "
                        "display layout or use one monitor.");
    }
    auto primary =
        std::find_if(monitors.begin(), monitors.end(), [](const Monitor &m) { return m.primary; });
    const long long dx = primary->rect.x, dy = primary->rect.y;
    for (auto &m : monitors) {
        const auto x = static_cast<long long>(m.rect.x) - dx,
                   y = static_cast<long long>(m.rect.y) - dy;
        if (x < -32768 || x > 32767 || y < -32768 || y > 32767)
            return fail("The monitor layout exceeds RDP coordinate limits.");
        m.rect.x = static_cast<int>(x);
        m.rect.y = static_cast<int>(y);
    }
    for (std::size_t i = 0; i < monitors.size(); ++i)
        for (std::size_t j = i + 1; j < monitors.size(); ++j)
            if (overlaps(monitors[i].rect, monitors[j].rect))
                return fail("Mirrored or overlapping displays are not supported. Use an extended "
                            "desktop or one monitor.");
    std::vector<bool> reached(monitors.size());
    reached[0] = true;
    for (std::size_t pass = 0; pass < monitors.size(); ++pass)
        for (std::size_t i = 0; i < monitors.size(); ++i)
            for (std::size_t j = 0; j < monitors.size(); ++j)
                if (reached[i] && adjacent(monitors[i].rect, monitors[j].rect))
                    reached[j] = true;
    if (std::find(reached.begin(), reached.end(), false) != reached.end())
        return fail("Displays must touch along an edge. Remove gaps in Linux Display Settings, or "
                    "use one monitor.");
    long long l = 0, t = 0, r = 0, b = 0;
    for (const auto &m : monitors) {
        l = std::min<long long>(l, m.rect.x);
        t = std::min<long long>(t, m.rect.y);
        r = std::max(r, right(m.rect));
        b = std::max(b, bottom(m.rect));
    }
    if (r - l > 16384 || b - t > 16384 || (r - l) * (b - t) > 64LL * 1024 * 1024)
        return fail("That desktop is too large for this evaluation build (16,384 per edge; 64 "
                    "megapixels total).");
    result.bounds = {static_cast<int>(l), static_cast<int>(t), static_cast<int>(r - l),
                     static_cast<int>(b - t)};
    result.monitors = std::move(monitors);
    return result;
}
MappedPoint mapPointer(int vw, int vh, Rect src, double x, double y) {
    if (vw <= 0 || vh <= 0 || src.width <= 0 || src.height <= 0 || !std::isfinite(x) ||
        !std::isfinite(y))
        return {};
    const double scale = std::min(double(vw) / src.width, double(vh) / src.height);
    const double left = (vw - src.width * scale) / 2.0, top = (vh - src.height * scale) / 2.0;
    const double rx = (x - left) / scale, ry = (y - top) / scale;
    const bool inside = rx >= 0 && ry >= 0 && rx < src.width && ry < src.height;
    // Clamp before integer conversion: very large mouse coordinates must not overflow.
    return {src.x + static_cast<int>(std::clamp(rx, 0.0, double(src.width - 1))),
            src.y + static_cast<int>(std::clamp(ry, 0.0, double(src.height - 1))), inside};
}
std::optional<std::uint32_t> evdevToScanCode(std::uint32_t k) {
    if (k >= 1 && k <= 83)
        return k;
    if (k == 86)
        return 0x56; // ISO 102nd key
    if (k == 87 || k == 88)
        return k; // F11/F12
    switch (k) {
    case 96:
        return 0x11c;
    case 97:
        return 0x11d;
    case 98:
        return 0x135;
    case 99:
        return 0x137;
    case 100:
        return 0x138;
    case 102:
        return 0x147;
    case 103:
        return 0x148;
    case 104:
        return 0x149;
    case 105:
        return 0x14b;
    case 106:
        return 0x14d;
    case 107:
        return 0x14f;
    case 108:
        return 0x150;
    case 109:
        return 0x151;
    case 110:
        return 0x152;
    case 111:
        return 0x153;
    case 113:
        return 0x120;
    case 114:
        return 0x12e;
    case 115:
        return 0x130;
    case 117:
        return 0x59;
    case 125:
        return 0x15b;
    case 126:
        return 0x15c;
    case 127:
        return 0x15d;
    default:
        return std::nullopt;
    }
}
std::vector<std::uint8_t> encodeClipboard(std::u16string_view text) {
    std::vector<std::uint8_t> out;
    if (text.size() > (MaxClipboardBytes / 2) - 1)
        return out;
    out.reserve(text.size() * 2 + 2);
    auto put = [&](char16_t c) {
        out.push_back(c & 0xff);
        out.push_back((c >> 8) & 0xff);
    };
    char16_t prev = 0;
    for (char16_t c : text) {
        if (c == 0)
            break; // CF_UNICODETEXT terminator, never advertise data past NUL.
        if (c == u'\n' && prev != u'\r')
            put(u'\r');
        put(c);
        prev = c;
        if (out.size() > MaxClipboardBytes - 2)
            return {};
    }
    put(0);
    return out;
}
std::optional<std::u16string> decodeClipboard(const std::uint8_t *bytes, std::size_t size) {
    if (!bytes || size < 2 || size > MaxClipboardBytes || (size & 1))
        return std::nullopt;
    std::u16string result;
    result.reserve(size / 2);
    bool terminated = false;
    for (std::size_t i = 0; i < size; i += 2) {
        char16_t c = static_cast<char16_t>(bytes[i] | (unsigned(bytes[i + 1]) << 8));
        if (c == 0) {
            terminated = true;
            break;
        }
        if (c == u'\r' && i + 3 < size && bytes[i + 2] == '\n' && bytes[i + 3] == 0)
            continue;
        result.push_back(c);
    }
    if (!terminated)
        return std::nullopt;
    // Reject unpaired UTF-16 surrogates instead of injecting malformed text into Qt.
    for (std::size_t i = 0; i < result.size(); ++i) {
        if (result[i] >= 0xd800 && result[i] <= 0xdbff) {
            if (++i >= result.size() || result[i] < 0xdc00 || result[i] > 0xdfff)
                return std::nullopt;
        } else if (result[i] >= 0xdc00 && result[i] <= 0xdfff)
            return std::nullopt;
    }
    return result;
}
} // namespace velo::core
