#include "hb.h"
#include "hb-ot.h"
#include <cstdint>
struct fonts_shape_glyph {uint32_t glyph,cluster;int32_t x_advance,y_advance,x_offset,y_offset;};
extern "C" void *fonts_shape_create(const uint8_t *data,uint32_t length) {
 if(!data||!length||length>256u*1024u*1024u)return nullptr;
 hb_blob_t *blob=hb_blob_create(reinterpret_cast<const char*>(data),length,HB_MEMORY_MODE_READONLY,nullptr,nullptr);
 hb_face_t *face=hb_face_create(blob,0);hb_blob_destroy(blob);
 unsigned n=hb_face_get_glyph_count(face),upm=hb_face_get_upem(face);
 if(!n||n>65535||upm<16||upm>16384){hb_face_destroy(face);return nullptr;}
 hb_font_t *font=hb_font_create(face);hb_face_destroy(face);hb_ot_font_set_funcs(font);hb_font_set_scale(font,upm,upm);hb_font_make_immutable(font);return font;
}
extern "C" void fonts_shape_destroy(void *font){if(font)hb_font_destroy(static_cast<hb_font_t*>(font));}
extern "C" uint32_t fonts_unicode_script(uint32_t c){return static_cast<uint32_t>(hb_unicode_script(hb_unicode_funcs_get_default(),c));}
extern "C" int fonts_unicode_extender(uint32_t c) {
 auto gc=hb_unicode_general_category(hb_unicode_funcs_get_default(),c);
 return gc==HB_UNICODE_GENERAL_CATEGORY_SPACING_MARK||gc==HB_UNICODE_GENERAL_CATEGORY_ENCLOSING_MARK||gc==HB_UNICODE_GENERAL_CATEGORY_NON_SPACING_MARK||c==0x200c||c==0x200d||(c>=0xfe00&&c<=0xfe0f)||(c>=0xe0100&&c<=0xe01ef);
}
extern "C" int fonts_shape_run(void *handle,const char *text,uint32_t text_length,const char *language,int vertical,int kerning,int rtl,uint32_t script,fonts_shape_glyph *out,uint32_t capacity,uint32_t *count) {
 if(!handle||!text||!language||!out||!count||text_length>16384u||capacity>16384u)return 1;
 auto font=static_cast<hb_font_t*>(handle);hb_buffer_t *buffer=hb_buffer_create();hb_buffer_set_cluster_level(buffer,HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES);
 hb_buffer_set_flags(buffer,HB_BUFFER_FLAG_REMOVE_DEFAULT_IGNORABLES);hb_buffer_add_utf8(buffer,text,text_length,0,text_length);if(script)hb_buffer_set_script(buffer,static_cast<hb_script_t>(script));hb_buffer_set_language(buffer,hb_language_from_string(language,-1));hb_buffer_guess_segment_properties(buffer);
 if(rtl>=0)hb_buffer_set_direction(buffer,rtl?HB_DIRECTION_RTL:HB_DIRECTION_LTR);if(vertical)hb_buffer_set_direction(buffer,HB_DIRECTION_TTB);
 hb_feature_t features[2];unsigned feature_count=0;
 if(!kerning){hb_feature_from_string("kern=0",-1,&features[feature_count++]);hb_feature_from_string("vkrn=0",-1,&features[feature_count++]);}
 hb_shape(font,buffer,features,feature_count);
 unsigned length_out=0;const hb_glyph_info_t *info=hb_buffer_get_glyph_infos(buffer,&length_out);const hb_glyph_position_t *pos=hb_buffer_get_glyph_positions(buffer,nullptr);int result=0;
 if(!hb_buffer_allocation_successful(buffer)||length_out>capacity||(!info&&length_out)||(!pos&&length_out))result=3;
 else {for(unsigned i=0;i<length_out;i++)out[i]={info[i].codepoint,info[i].cluster,pos[i].x_advance,pos[i].y_advance,pos[i].x_offset,pos[i].y_offset};*count=length_out;}
 hb_buffer_destroy(buffer);return result;
}
