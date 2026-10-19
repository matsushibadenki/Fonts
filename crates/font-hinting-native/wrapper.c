/* Fonts-owned bridge. Native algorithms retain their upstream licenses. */
#include <stdlib.h>
#include <string.h>
#include "hb.h"
#include "hb-subset.h"
#include "ttfautohint.h"
int fonts_cff_dehint(const unsigned char *data, unsigned int length, unsigned int index,
                     unsigned char **out, size_t *out_length) {
  hb_blob_t *blob = hb_blob_create((const char *)data, length, HB_MEMORY_MODE_READONLY, NULL, NULL);
  hb_face_t *face = hb_face_create(blob, index);
  hb_subset_input_t *input = hb_subset_input_create_or_fail();
  int result = 1;
  hb_face_t *subset = NULL;
  hb_blob_t *table = NULL;
  if (!input || !hb_face_get_glyph_count(face)) goto done;
  hb_set_add_range(hb_subset_input_glyph_set(input), 0, hb_face_get_glyph_count(face)-1);
  hb_subset_input_set_flags(input, HB_SUBSET_FLAGS_NO_HINTING | HB_SUBSET_FLAGS_RETAIN_GIDS |
     HB_SUBSET_FLAGS_NOTDEF_OUTLINE | HB_SUBSET_FLAGS_GLYPH_NAMES);
  subset = hb_subset_or_fail(face, input);
  if (!subset) goto done;
  table = hb_face_reference_table(subset, HB_TAG('C','F','F',' '));
  if (!hb_blob_get_length(table)) {
    hb_blob_destroy(table);
    table = hb_face_reference_table(subset, HB_TAG('C','F','F','2'));
  }
  unsigned int size;
  const char *payload = hb_blob_get_data(table, &size);
  if (!size || size > 256u*1024u*1024u) goto done;
  *out = (unsigned char *)malloc(size);
  if (!*out) goto done;
  memcpy(*out, payload, size); *out_length = size; result = 0;
done:
  if (table) hb_blob_destroy(table);
  if (subset) hb_face_destroy(subset);
  if (input) hb_subset_input_destroy(input);
  hb_face_destroy(face); hb_blob_destroy(blob);
  return result;
}
int fonts_autohint(const unsigned char *data, size_t length, unsigned char **out, size_t *out_length,
                   int minimum, int maximum) {
  char *buffer = NULL;
  size_t size = 0;
  int status = TTF_autohint("in-buffer, in-buffer-len, out-buffer, out-buffer-len, hinting-range-min, hinting-range-max, hinting-limit, symbol, epoch",
      (const char *)data, length, &buffer, &size, minimum, maximum, 200, 1, (unsigned long long)0);
  if (status) { free(buffer); return status; }
  *out = (unsigned char *)buffer; *out_length = size;
  return 0;
}
void fonts_hinting_free(void *pointer) { free(pointer); }
