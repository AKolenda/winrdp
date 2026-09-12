// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include <QImage>
#include <QRect>
#include <QOpenGLWidget>
#include <QString>
namespace velo {
// Presentation only. Decoded/GDI pixels remain in host memory; this is not a
// zero-copy decoder. All methods are invoked on the GUI thread.
class GpuCanvas final : public QOpenGLWidget {
    Q_OBJECT
  public:
    explicit GpuCanvas(QWidget *parent = nullptr);
    void present(const QImage &frame, QRect source);
    QString description() const { return description_; }
    static bool probe(QString *description = nullptr);
  signals:
    void unavailable(QString reason);
  protected:
    void initializeGL() override;
    void paintGL() override;
  private:
    QImage frame_;
    QRect source_;
    QString description_ = "OpenGL initializing";
};
}
