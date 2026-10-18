"""MIT: Original geometric color font fixtures. FontTools is test tooling only."""
from pathlib import Path
import struct,zlib
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.colorLib.builder import buildCOLR,buildCPAL
from fontTools.ttLib import newTable
from fontTools.ttLib.tables.sbixStrike import Strike
from fontTools.ttLib.tables.sbixGlyph import Glyph
from fontTools.ttLib.tables.S_V_G_ import SVGDocument
root=Path(__file__).parent
fb=FontBuilder(1000,isTTF=True);order=['.notdef','A','red','blue','space'];fb.setupGlyphOrder(order)
glyphs={}
for g in order:
 p=TTGlyphPen(None)
 if g in ['red','blue']:
  x,y,w,h=(0,0,700,700) if g=='red' else (250,250,450,450)
  p.moveTo((x,y));p.lineTo((x+w,y));p.lineTo((x+w,y+h));p.lineTo((x,y+h));p.closePath()
 glyphs[g]=p.glyph()
fb.setupGlyf(glyphs);fb.setupHorizontalMetrics({g:(800,0) for g in order});fb.setupHorizontalHeader(ascent=800,descent=-200);fb.setupCharacterMap({65:'A',32:'space'})
fb.setupNameTable({'familyName':'Color Fixture','styleName':'Regular','uniqueFontIdentifier':'ColorFixture','fullName':'Color Fixture','psName':'ColorFixture'})
fb.setupOS2(sTypoAscender=800,sTypoDescender=-200,usWinAscent=800,usWinDescent=200);fb.setupPost()
f=fb.font
f['CPAL']=buildCPAL([[(1,0,0,1),(0,0,1,.5)],[(0,1,0,1),(1,1,0,1)]])
f['COLR']=buildCOLR({'A':[('red',0),('blue',1)]},version=0);f.save(root/'colrv0.ttf')
f['COLR']=buildCOLR({'A':{'Format':10,'Glyph':'red','Paint':{'Format':2,'PaletteIndex':0,'Alpha':1.0}}},version=1);f.save(root/'colrv1.ttf')
del f['COLR'];del f['CPAL']
f['SVG ']=newTable('SVG ');f['SVG '].docList=[SVGDocument('<svg xmlns="http://www.w3.org/2000/svg"><rect width="700" height="700" fill="red"/></svg>',1,1)];f.save(root/'svg.ttf');del f['SVG ']
def chunk(tag,data):return struct.pack('>I',len(data))+tag+data+struct.pack('>I',zlib.crc32(tag+data)&0xffffffff)
raw=b''.join(b'\0'+b''.join(bytes((255,0,0,255) if x<64 else (0,0,255,128)) for x in range(128)) for y in range(128))
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',128,128,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b'')
f['sbix']=newTable('sbix');s=Strike(ppem=128,resolution=72);s.glyphs={'A':Glyph(glyphName='A',originOffsetX=0,originOffsetY=0,graphicType='png ',imageData=png)};f['sbix'].strikes={128:s};f.save(root/'sbix.ttf')
# A PNG CBDT strike, index format 1 / image format 17.
from fontTools.ttLib.tables.DefaultTable import DefaultTable
from fontTools.ttLib import TTFont,TTCollection
del f['sbix']
block=struct.pack('>BBbbB',128,128,0,127,128)+struct.pack('>I',len(png))+png
cbdt=struct.pack('>I',0x30000)+block
size=struct.pack('>IIII',56,24,1,0)+b'\0'*24+struct.pack('>HHBBBB',1,1,128,128,32,1)
cblc=struct.pack('>II',0x30000,1)+size+struct.pack('>HHI',1,1,8)+struct.pack('>HHIII',1,17,4,0,len(block))
for tag,data in [('CBDT',cbdt),('CBLC',cblc)]:
 t=DefaultTable(tag);t.data=data;f[tag]=t
f.save(root/'cbdt.ttf')
c=TTCollection();c.fonts=[TTFont(root/'colrv0.ttf'),TTFont(root/'sbix.ttf')];c.save(root/'collection.ttc')
from fontTools.pens.t2CharStringPen import T2CharStringPen
fb2=FontBuilder(1000,isTTF=False);fb2.setupGlyphOrder(order)
charstrings={}
for g in order:
 p=T2CharStringPen(800,None)
 if g in ['red','blue']:
  x,y,w,h=(0,0,700,700) if g=='red' else (250,250,450,450)
  p.moveTo((x,y));p.lineTo((x+w,y));p.lineTo((x+w,y+h));p.lineTo((x,y+h));p.closePath()
 charstrings[g]=p.getCharString()
fb2.setupCFF('ColorFixtureCFF',{'FullName':'Color Fixture CFF','FamilyName':'Color Fixture CFF','Weight':'Regular'},charstrings,{})
fb2.setupHorizontalMetrics({g:(800,0) for g in order});fb2.setupHorizontalHeader(ascent=800,descent=-200);fb2.setupCharacterMap({65:'A',32:'space'})
fb2.setupNameTable({'familyName':'Color Fixture CFF','styleName':'Regular','uniqueFontIdentifier':'ColorFixtureCFF','fullName':'Color Fixture CFF','psName':'ColorFixtureCFF'});fb2.setupOS2(sTypoAscender=800,sTypoDescender=-200,usWinAscent=800,usWinDescent=200);fb2.setupPost()
fb2.font['COLR']=buildCOLR({'A':[('red',0),('blue',1)]},version=0);fb2.font['CPAL']=buildCPAL([[(1,0,0,1),(0,0,1,.5)]]);fb2.save(root/'colrv0.otf')
