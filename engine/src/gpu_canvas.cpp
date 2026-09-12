// SPDX-License-Identifier: AGPL-3.0-only
#include "gpu_canvas.hpp"
#include <QGuiApplication>
#include <QOffscreenSurface>
#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <QPainter>
#include <QTimer>
namespace velo {
namespace {
QString renderer(QOpenGLContext *c) {
    if (!c) return {};
    auto *f = c->functions(); f->initializeOpenGLFunctions();
    const auto *r = f->glGetString(GL_RENDERER);
    return r ? QString::fromLatin1(reinterpret_cast<const char *>(r)) : QString{};
}
bool isSoftware(const QString &r) {
    return r.isEmpty() || r.contains("llvmpipe",Qt::CaseInsensitive) || r.contains("softpipe",Qt::CaseInsensitive)
        || r.contains("software",Qt::CaseInsensitive);
}
}
bool GpuCanvas::probe(QString *description) {
    // Headless tests intentionally exercise the CPU renderer, not a fake GPU.
    const auto platform = QGuiApplication::platformName();
    if (platform == "offscreen" || platform == "minimal") return false;
    QOpenGLContext context;
    if (!context.create()) return false;
    QOffscreenSurface surface; surface.setFormat(context.format()); surface.create();
    if (!surface.isValid() || !context.makeCurrent(&surface)) return false;
    const auto r = renderer(&context);
    if (description) *description = r;
    context.doneCurrent();
    return !isSoftware(r);
}
GpuCanvas::GpuCanvas(QWidget *parent) : QOpenGLWidget(parent) {
    setObjectName("gpuPresentation");
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setFocusPolicy(Qt::NoFocus);
    setUpdateBehavior(QOpenGLWidget::NoPartialUpdate);
    // Context creation can fail after a successful probe (driver/device change).
    QTimer::singleShot(1200,this,[this] {
        if (isVisible() && !isValid()) emit unavailable("OpenGL context unavailable; using software presentation");
    });
}
void GpuCanvas::initializeGL() {
    const auto r = renderer(context());
    description_ = "OpenGL presentation · " + r;
    if (isSoftware(r)) QTimer::singleShot(0,this,[this] { emit unavailable("Hardware OpenGL unavailable; using software presentation"); });
}
void GpuCanvas::present(const QImage &frame, QRect source) { frame_ = frame; source_ = source; update(); }
void GpuCanvas::paintGL() {
    // QPainter uses Qt's OpenGL paint engine on this surface (texture upload,
    // image scaling, composition). Preserve identical source/pointer mapping.
    QPainter p(this);
    p.fillRect(rect(),QColor("#101723"));
    if (frame_.isNull()) return;
    const QRect src = source_.isEmpty() ? frame_.rect() : source_.intersected(frame_.rect());
    if (src.isEmpty()) return;
    QSize size = src.size(); size.scale(this->size(),Qt::KeepAspectRatio);
    QRect dst(QPoint((width()-size.width())/2,(height()-size.height())/2),size);
    p.setRenderHint(QPainter::SmoothPixmapTransform,size != src.size());
    p.drawImage(dst,frame_,src);
}
}
