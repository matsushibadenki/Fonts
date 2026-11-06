#include <hb.h>
#include <hb-ot.h>
#include <hb-subset.h>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <initializer_list>
struct FontsAxisRange { uint32_t tag; float minimum, maximum, def; };
extern "C" int fonts_variation_subset(const uint8_t *data, unsigned length,
    unsigned index, const FontsAxisRange *ranges, unsigned count,
    uint8_t **out, unsigned *out_length) {
  if (!data || !length || length > 256u*1024u*1024u || !out || !out_length ||
      count > 64 || (count && !ranges)) return 1;
  *out = nullptr; *out_length = 0;
  std::unique_ptr<hb_blob_t, decltype(&hb_blob_destroy)> blob(
      hb_blob_create(reinterpret_cast<const char *>(data), length, HB_MEMORY_MODE_READONLY, nullptr, nullptr), hb_blob_destroy);
  if (!blob || !hb_blob_get_length(blob.get())) return 1;
  std::unique_ptr<hb_face_t, decltype(&hb_face_destroy)> face(hb_face_create(blob.get(), index), hb_face_destroy);
  if (!face || !hb_face_get_glyph_count(face.get()) || hb_face_get_glyph_count(face.get()) > 65535 || !hb_ot_var_has_data(face.get())) return 1;
  std::unique_ptr<hb_subset_input_t, decltype(&hb_subset_input_destroy)> input(hb_subset_input_create_or_fail(), hb_subset_input_destroy);
  if (!input) return 1;
  hb_set_add_range(hb_subset_input_glyph_set(input.get()), 0, hb_face_get_glyph_count(face.get())-1);
  hb_set_clear(hb_subset_input_unicode_set(input.get()));
  hb_set_invert(hb_subset_input_unicode_set(input.get()));
  hb_set_clear(hb_subset_input_set(input.get(), HB_SUBSET_SETS_NAME_ID));
  hb_set_invert(hb_subset_input_set(input.get(), HB_SUBSET_SETS_NAME_ID));
  hb_set_clear(hb_subset_input_set(input.get(), HB_SUBSET_SETS_NAME_LANG_ID));
  hb_set_invert(hb_subset_input_set(input.get(), HB_SUBSET_SETS_NAME_LANG_ID));
  hb_subset_input_set_flags(input.get(), HB_SUBSET_FLAGS_RETAIN_GIDS | HB_SUBSET_FLAGS_GLYPH_NAMES |
      HB_SUBSET_FLAGS_NOTDEF_OUTLINE | HB_SUBSET_FLAGS_NAME_LEGACY |
      HB_SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED | HB_SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES |
      HB_SUBSET_FLAGS_DOWNGRADE_CFF2);
  hb_set_clear(hb_subset_input_set(input.get(), HB_SUBSET_SETS_DROP_TABLE_TAG));
  hb_set_clear(hb_subset_input_set(input.get(), HB_SUBSET_SETS_LAYOUT_FEATURE_TAG));
  hb_set_invert(hb_subset_input_set(input.get(), HB_SUBSET_SETS_LAYOUT_FEATURE_TAG));
  // These engines encode positioning/variation references that the OT subsetter cannot rebase.
  for (hb_tag_t tag : {HB_TAG('J','S','T','F'), HB_TAG('k','e','r','x'), HB_TAG('S','i','l','f')}) {
    std::unique_ptr<hb_blob_t, decltype(&hb_blob_destroy)> table(hb_face_reference_table(face.get(), tag), hb_blob_destroy);
    if (hb_blob_get_length(table.get())) return 4;
  }
  hb_set_add(hb_subset_input_set(input.get(), HB_SUBSET_SETS_DROP_TABLE_TAG), HB_TAG('D','S','I','G'));
  for (unsigned i=0; i<count; ++i) {
    const auto &r=ranges[i]; hb_ot_var_axis_info_t axis{};
    if (!std::isfinite(r.minimum) || !std::isfinite(r.maximum) || !std::isfinite(r.def) ||
        r.minimum > r.def || r.def > r.maximum ||
        !hb_ot_var_find_axis_info(face.get(), r.tag, &axis) ||
        r.minimum < axis.min_value || r.maximum > axis.max_value) return 2;
    for (unsigned j=0; j<i; ++j) if (ranges[j].tag==r.tag) return 2;
    if (!hb_subset_input_set_axis_range(input.get(), face.get(), r.tag, r.minimum, r.maximum, r.def)) return 2;
  }
  std::unique_ptr<hb_face_t, decltype(&hb_face_destroy)> result(hb_subset_or_fail(face.get(), input.get()), hb_face_destroy);
  if (!result || hb_face_get_glyph_count(result.get()) != hb_face_get_glyph_count(face.get())) return 3;
  std::unique_ptr<hb_blob_t, decltype(&hb_blob_destroy)> output(hb_face_reference_blob(result.get()), hb_blob_destroy);
  unsigned n=0; const char *p=hb_blob_get_data(output.get(), &n);
  if (!p || !n || n>256u*1024u*1024u) return 3;
  auto *copy=static_cast<uint8_t *>(std::malloc(n));
  if (!copy) return 3;
  std::memcpy(copy,p,n); *out=copy; *out_length=n; return 0;
}
