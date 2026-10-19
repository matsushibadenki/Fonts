/* Fonts-owned bridge to Adobe's Apache-2.0 psautohint library. */
#include <stdlib.h>
#include <string.h>
#include "psautohint.h"
static void discard_report(char *message, int level) { (void)message; (void)level; }
int fonts_ps_autohint(const char *bez, const char *info, unsigned char **out, size_t *length) {
  AC_initCallGlobals();
  AC_SetReportCB(discard_report);
  ACBuffer *buffer = ACBufferNew(1024);
  if (!buffer) return 1;
  /* Preserve geometry; allow hint replacement, but disable coordinate rounding. */
  int status = AutoHintString(bez, info, buffer, 0, 1, 0);
  if (!status) {
    char *data; size_t size;
    ACBufferRead(buffer, &data, &size);
    if (!size || size > 16u*1024u*1024u) status = 1;
    else {
      *out = (unsigned char *)malloc(size);
      if (!*out) status = 1;
      else { memcpy(*out,data,size); *length=size; }
    }
  }
  ACBufferFree(buffer);
  return status;
}
