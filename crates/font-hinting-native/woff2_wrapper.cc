// Fonts ownership and semantic-verification boundary around the MIT WOFF2 reference codec.
#include <woff2/encode.h>
#include <woff2/decode.h>
#include "font.h"
#include "glyph.h"
#include "table_tags.h"
#include <cstdlib>
#include <cstring>
#include <vector>
#include <string>
#include <exception>
static constexpr size_t limit=256u*1024u*1024u;
static bool admit(const woff2::Font& font){
 const int n=woff2::NumGlyphs(font);if(n<1||n>65535)return false;size_t total=0;
 for(int i=0;i<n;i++){const uint8_t* data;size_t size;if(!woff2::GetGlyphData(font,i,&data,&size))return false;if(!size)continue;if(size<10)return false;
  int contours=static_cast<int16_t>((data[0]<<8)|data[1]);if(contours < -1)return false;
  if(contours>0){size_t end=10+2*(static_cast<size_t>(contours)-1);if(end+2>size)return false;total+=1+((data[end]<<8)|data[end+1]);if(total>16000000)return false;}
 }return true;
}
extern "C" int fonts_woff2_encode(const uint8_t* data,size_t len,int quality,uint8_t** output,size_t* size){
 try {
  if(!data||!output||!size||len>limit||quality<1||quality>11)return 1;
  woff2::Font font;if(!woff2::ReadFont(data,len,&font)||!admit(font))return 1;
  const size_t cap=woff2::MaxWOFF2CompressedSize(data,len);if(!cap||cap>limit)return 1;
  std::vector<uint8_t> bytes(cap);size_t n=cap;woff2::WOFF2Params params;params.brotli_quality=quality;
  if(!woff2::ConvertTTFToWOFF2(data,len,bytes.data(),&n,params)||!n||n>cap)return 1;
  *output=static_cast<uint8_t*>(malloc(n));if(!*output)return 1;memcpy(*output,bytes.data(),n);*size=n;return 0;
 }catch(...){return 1;}
}
extern "C" int fonts_woff2_decode(const uint8_t* data,size_t len,uint8_t** output,size_t* size){
 try {
  if(!data||!output||!size||len>limit)return 1;
  std::string bytes;woff2::WOFF2StringOut out(&bytes);out.SetMaxSize(limit);
  if(!woff2::ConvertWOFF2ToTTF(data,len,&out)||!out.Size()||out.Size()>limit)return 1;
  *output=static_cast<uint8_t*>(malloc(out.Size()));if(!*output)return 1;memcpy(*output,bytes.data(),out.Size());*size=out.Size();return 0;
 }catch(...){return 1;}
}
static bool empty(const uint8_t* p,size_t n){if(n==0)return true;if(n<10||p[0]||p[1])return false;for(size_t i=2;i<n;i++)if(p[i])return false;return true;}
static bool glyph_equal(const uint8_t* a,size_t an,const uint8_t* b,size_t bn){
 if(empty(a,an)||empty(b,bn))return empty(a,an)&&empty(b,bn);
 woff2::Glyph x,y;if(!woff2::ReadGlyph(a,an,&x)||!woff2::ReadGlyph(b,bn,&y))return false;
 if(x.x_min!=y.x_min||x.x_max!=y.x_max||x.y_min!=y.y_min||x.y_max!=y.y_max||
  x.instructions_size!=y.instructions_size||x.overlap_simple_flag_set!=y.overlap_simple_flag_set||x.composite_data_size!=y.composite_data_size||x.contours.size()!=y.contours.size())return false;
 if(x.instructions_size&&memcmp(x.instructions_data,y.instructions_data,x.instructions_size))return false;
 if(x.composite_data_size&&(x.have_instructions!=y.have_instructions||memcmp(x.composite_data,y.composite_data,x.composite_data_size)))return false;
 for(size_t c=0;c<x.contours.size();c++){
  if(x.contours[c].size()!=y.contours[c].size())return false;
  for(size_t p=0;p<x.contours[c].size();p++){const auto &u=x.contours[c][p],&v=y.contours[c][p];if(u.x!=v.x||u.y!=v.y||u.on_curve!=v.on_curve)return false;}
 }return true;
}
extern "C" int fonts_woff2_equivalent(const uint8_t* a,size_t an,const uint8_t* b,size_t bn){
 try {
  if(an>limit||bn>limit)return 0;woff2::Font x,y;
  if(!woff2::ReadFont(a,an,&x)||!woff2::ReadFont(b,bn,&y)||x.flavor!=y.flavor||x.tables.size()!=y.tables.size()||!admit(x)||!admit(y))return 0;
  for(auto &entry:x.tables){auto j=y.tables.find(entry.first);if(j==y.tables.end())return 0;const auto &u=entry.second,&v=j->second;
   if(entry.first==woff2::kGlyfTableTag||entry.first==woff2::kLocaTableTag)continue;
   if(entry.first==woff2::kHeadTableTag){
    if(u.length!=v.length||u.length<54)return 0;
    for(size_t i=0;i<u.length;i++){if((i>=8&&i<12)||(i>=50&&i<52))continue;uint8_t p=u.data[i],q=v.data[i];if(i==16){p|=8;q|=8;}if(p!=q)return 0;}
   }else if(u.length!=v.length||(u.length&&memcmp(u.data,v.data,u.length)))return 0;
  }
  const int n=woff2::NumGlyphs(x);if(n!=woff2::NumGlyphs(y)||n<1)return 0;
  for(int i=0;i<n;i++){const uint8_t *p,*q;size_t pn,qn;if(!woff2::GetGlyphData(x,i,&p,&pn)||!woff2::GetGlyphData(y,i,&q,&qn)||!glyph_equal(p,pn,q,qn))return 0;}
  return 1;
 }catch(...){return 0;}
}
