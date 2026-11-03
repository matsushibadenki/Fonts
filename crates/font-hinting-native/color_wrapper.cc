// MIT. Bounded COLR rasterization using the bundled HarfBuzz paint engine.
#include "hb.h"
#include "hb-ot.h"
#include "hb-raster.h"
#include <memory>

extern "C" int fonts_color_render(const unsigned char *data, unsigned length,
                                  unsigned index, unsigned glyph, unsigned size,
                                  unsigned palette, unsigned width, unsigned height,
                                  int x, int baseline, unsigned char *rgba) {
  if (!data || !rgba || !length || length > 256u * 1024u * 1024u ||
      !width || width > 2048 || !height || height > 1024 || size < 12 || size > 192)
    return 1;
  std::unique_ptr<hb_blob_t, decltype(&hb_blob_destroy)> blob(
      hb_blob_create_or_fail(reinterpret_cast<const char *>(data), length,
                             HB_MEMORY_MODE_READONLY, nullptr, nullptr), hb_blob_destroy);
  if (!blob) return 1;
  std::unique_ptr<hb_face_t, decltype(&hb_face_destroy)> face(
      hb_face_create_or_fail(blob.get(), index), hb_face_destroy);
  if (!face || glyph >= hb_face_get_glyph_count(face.get()) ||
      palette >= hb_ot_color_palette_get_count(face.get())) return 1;
  std::unique_ptr<hb_font_t, decltype(&hb_font_destroy)> font(
      hb_font_create(face.get()), hb_font_destroy);
  hb_ot_font_set_funcs(font.get());
  const unsigned upem = hb_face_get_upem(face.get());
  if (!upem) return 1;
  hb_font_set_scale(font.get(), upem, upem);
  std::unique_ptr<hb_raster_paint_t, decltype(&hb_raster_paint_destroy)> paint(
      hb_raster_paint_create_or_fail(), hb_raster_paint_destroy);
  if (!paint) return 1;
  // Fixed extents bound intermediate layers too; never allocate from font bounds.
  hb_raster_extents_t extents{0, 0, width, height, width * 4};
  hb_raster_paint_set_extents(paint.get(), &extents);
  const float scale = static_cast<float>(size) / upem;
  hb_raster_paint_set_transform(paint.get(), scale, 0, 0, -scale, x, baseline);
  hb_raster_paint_set_palette(paint.get(), palette);
  hb_raster_paint_set_foreground(paint.get(), HB_COLOR(0, 0, 0, 255));
  if (!hb_raster_paint_glyph_or_fail(paint.get(), font.get(), glyph)) return 1;
  std::unique_ptr<hb_raster_image_t, decltype(&hb_raster_image_destroy)> image(
      hb_raster_paint_render(paint.get()), hb_raster_image_destroy);
  if (!image || hb_raster_image_get_format(image.get()) != HB_RASTER_FORMAT_BGRA32)
    return 1;
  hb_raster_image_get_extents(image.get(), &extents);
  if (extents.x_origin || extents.y_origin || extents.width != width ||
      extents.height != height || extents.stride < width * 4) return 1;
  const auto *buffer = hb_raster_image_get_buffer(image.get());
  if (!buffer) return 1;
  // HarfBuzz produces premultiplied BGRA; ImageAsset/wgpu use straight RGBA.
  for (unsigned row = 0; row < height; ++row) {
    for (unsigned col = 0; col < width; ++col) {
      const auto *p = buffer + row * extents.stride + col * 4;
      auto *q = rgba + (row * width + col) * 4;
      for (unsigned c = 0; c < 3; ++c) {
        const unsigned v = p[3] ? (p[2 - c] * 255u + p[3] / 2) / p[3] : 0;
        q[c] = v > 255 ? 255 : v;
      }
      q[3] = p[3];
    }
  }
  return 0;
}
