// SPDX-License-Identifier: AGPL-3.0-only
#include "core.hpp"
#include <cstdlib>
#include <iostream>
#include <limits>
#include <thread>
using namespace velo::core;
static int checks = 0;
#define CHECK(x)                                                                                   \
    do {                                                                                           \
        ++checks;                                                                                  \
        if (!(x)) {                                                                                \
            std::cerr << "FAIL " << __FILE__ << ':' << __LINE__ << "  " #x "\n";                   \
            std::exit(1);                                                                          \
        }                                                                                          \
    } while (false)
int main() {
    for (const auto *s : {"desktop", "MY-PC", "work.example", "192.168.1.10", "192.168.1.10:3390",
                          "[::1]", "::1", "[fe80::1%eth0]:3390", " example.com. "})
        CHECK(parseEndpoint(s).value.has_value());
    for (const auto *s : {"",           "  ",           "host:",
                          "host:0",     "host:65536",   "host:+33",
                          "host:3x",    "host:-1",      "host:99999999999999999999",
                          "host name",  "https://host", "/p:secret",
                          "user@host",  "[xyz]",        "[::1]x",
                          "[::1]:",     "[::1]:1:2",    "[::1",
                          "-host",      "a..b",         "999.1.1.1",
                          "host;touch", "::gg",         "fe80::1%",
                          "::1%%abc",   "host\nname"})
        CHECK(!parseEndpoint(s).value);
    auto e = parseEndpoint("[2001:db8::1]:3391");
    CHECK(e.value->host == "2001:db8::1");
    CHECK(e.value->port == 3391);
    CHECK(e.value->display() == "[2001:db8::1]:3391");
    CHECK(parseEndpoint("pc:3389").value->display() == "pc");
    CHECK((safeDesktopSize(1, 1) == std::make_pair(200, 200)));
    CHECK((safeDesktopSize(1921, 1081) == std::make_pair(1920, 1081)));
    CHECK((safeDesktopSize(99999, 99999) == std::make_pair(8192, 8192)));
    std::vector<Monitor> two = {{{-1920, 0, 1920, 1080}, false}, {{0, 0, 2560, 1440}, true}};
    auto l = normalizeMonitors(two);
    CHECK(bool(l));
    CHECK(l.bounds.x == -1920);
    CHECK(l.bounds.width == 4480);
    CHECK(l.bounds.height == 1440);
    auto offset =
        normalizeMonitors({{{100, 200, 1920, 1080}, true}, {{2020, 200, 1920, 1080}, false}});
    CHECK(bool(offset));
    CHECK(offset.monitors[0].rect.x == 0);
    CHECK(offset.monitors[1].rect.x == 1920);
    CHECK(!normalizeMonitors({}));
    CHECK(!normalizeMonitors({{{0, 0, 1920, 1080}, false}}));
    CHECK(!normalizeMonitors({{{0, 0, 1920, 1080}, true}, {{0, 0, 1920, 1080}, true}}));
    CHECK(!normalizeMonitors({{{0, 0, 1920, 1080}, true}, {{1000, 0, 1920, 1080}, false}}));
    CHECK(!normalizeMonitors({{{0, 0, 1920, 1080}, true}, {{2000, 0, 1920, 1080}, false}}));
    CHECK(!normalizeMonitors({{{0, 0, 1920, 1080}, true}, {{1920, 1080, 1920, 1080}, false}}));
    CHECK(!normalizeMonitors({{{0, 0, 1921, 1080}, true}}));
    CHECK(!normalizeMonitors({{{0, 0, 199, 1080}, true}}));
    CHECK(!normalizeMonitors(
        {{{0, 0, 1920, 1080}, true}, {{std::numeric_limits<int>::min(), 0, 1920, 1080}, false}}));
    auto p = mapPointer(1000, 1000, {0, 0, 1920, 1080}, 500, 500);
    CHECK(p.inside);
    CHECK(p.x == 960);
    CHECK(p.y == 540);
    p = mapPointer(1000, 1000, {0, 0, 1920, 1080}, 0, 0);
    CHECK(!p.inside);
    CHECK(p.y == 0);
    p = mapPointer(1920, 1080, {1920, 0, 1920, 1080}, 1919, 1079);
    CHECK(p.inside);
    CHECK(p.x == 3839);
    CHECK(p.y == 1079);
    CHECK(!mapPointer(0, 0, {0, 0, 10, 10}, 2, 3).inside);
    CHECK(!mapPointer(100, 100, {0, 0, 10, 10}, std::numeric_limits<double>::infinity(), 3).inside);
    for (std::uint32_t k = 1; k <= 83; ++k)
        CHECK(evdevToScanCode(k) == k);
    CHECK(evdevToScanCode(97) == 0x11d);
    CHECK(evdevToScanCode(111) == 0x153);
    CHECK(evdevToScanCode(125) == 0x15b);
    CHECK(!evdevToScanCode(9999));
    for (const auto &text : {std::u16string(u""), std::u16string(u"hello"),
                             std::u16string(u"café → 🚀"), std::u16string(u"first\nsecond")}) {
        auto bytes = encodeClipboard(text);
        auto decoded = decodeClipboard(bytes.data(), bytes.size());
        CHECK(decoded);
        CHECK(*decoded == text);
    }
    auto crlf = encodeClipboard(u"a\r\nb");
    CHECK(decodeClipboard(crlf.data(), crlf.size()) == std::u16string(u"a\nb"));
    std::uint8_t odd[]{1, 2, 3};
    CHECK(!decodeClipboard(odd, 3));
    CHECK(!decodeClipboard(nullptr, 0));
    std::uint8_t missingNul[]{65, 0};
    CHECK(!decodeClipboard(missingNul, 2));
    std::uint8_t badSurrogate[]{0, 0xd8, 0, 0};
    CHECK(!decodeClipboard(badSurrogate, 4));
    std::u16string huge(MaxClipboardBytes / 2, u'x');
    CHECK(encodeClipboard(huge).empty());
    LatestMailbox<int> box;
    CHECK(!box.take());
    box.put(1);
    box.put(2);
    CHECK(box.take() == 2);
    CHECK(!box.take());
    CHECK(box.generation() == 2);
    std::thread producer([&] {
        for (int i = 0; i < 10000; ++i)
            box.put(i);
    });
    for (int i = 0; i < 10000; ++i)
        (void)box.take();
    producer.join();
    CHECK(box.generation() == 10002);
    std::cout << checks << " checks passed.\n";
}
