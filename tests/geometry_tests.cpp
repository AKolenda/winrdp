// SPDX-License-Identifier: AGPL-3.0-only
#include "window_geometry.hpp"
#include <stdexcept>
#define CHECK(expr) do { if (!(expr)) throw std::runtime_error(#expr); } while(false)
#include <iostream>
#include <vector>
using velo::geometry::Rect;
int main() {
    const std::vector<Rect> screens={{0,0,3440,1440},{-3440,0,3440,1440},
        {3440,0,3440,1440},{0,-1440,3440,1440},{-5120,-2880,5120,1440},
        {0,0,2752,1152},{0,0,800,600},{0,0,640,360},{1280,0,5120,1440}};
    const std::vector<Rect> saved={{50,50,1240,820},{6000,3000,4000,2500},
        {-9000,-9000,1280,840},{-3340,100,1000,700},{0,0,-2,0}};
    int cases=0;
    for (auto s:screens) {
        for (auto r:saved) {
            const auto v=velo::geometry::clamp(r,s);
            CHECK(v.x>=s.x && v.y>=s.y);
            CHECK(v.width>0 && v.height>0);
            CHECK(v.x+v.width<=s.x+s.width && v.y+v.height<=s.y+s.height);
            const auto again=velo::geometry::clamp(v,s);
            CHECK(v.x==again.x && v.y==again.y && v.width==again.width && v.height==again.height);
            ++cases;
        }
        const auto c=velo::geometry::centered(s);
        CHECK(c.x>=s.x && c.y>=s.y && c.width<=s.width && c.height<=s.height);
        ++cases;
    }
    const auto negative=velo::geometry::centered({-3440,0,3440,1440},1240,820);
    CHECK(negative.x==-2340 && negative.y==310);
    std::cout<<cases+1<<" geometry scenarios passed (mathematical placement, not a window-manager test).\n";
}
