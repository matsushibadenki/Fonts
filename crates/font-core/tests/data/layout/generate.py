"""Original MIT geometric shaping fixture. FontTools is development tooling only."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
root=Path(__file__).parent
order=['.notdef','space','A','B','f','i','fi','acutecomb','ideograph','ideograph.alt','comma','comma.vert','paren','paren.vert','longmark','alef','bet']
fb=FontBuilder(1000,isTTF=True);fb.setupGlyphOrder(order)
glyphs={}
for index,name in enumerate(order):
 p=TTGlyphPen(None)
 if name not in ['.notdef','space']:
  if name=='acutecomb':points=[(0,0),(100,180),(160,180),(60,0)]
  elif name in ['comma.vert','paren.vert']:points=[(650,650),(850,650),(850,850),(650,850)]
  elif name=='longmark':points=[(100,300),(900,300),(900,360),(100,360)]
  elif name=='ideograph.alt':points=[(100,0),(800,0),(800,150),(250,150),(250,700),(100,700)]
  else:points=[(20,0),(300+index*10,0),(300+index*10,700),(20,700)]
  p.moveTo(points[0]);[p.lineTo(v) for v in points[1:]];p.closePath()
 glyphs[name]=p.glyph()
fb.setupGlyf(glyphs)
fb.setupHorizontalMetrics({name:(1000 if name in ['ideograph','ideograph.alt','comma','comma.vert','paren','paren.vert','longmark','alef','bet'] else (0 if name=='acutecomb' else 600),0) for name in order})
fb.setupHorizontalHeader(ascent=800,descent=-200)
fb.setupCharacterMap({32:'space',65:'A',66:'B',102:'f',105:'i',0x301:'acutecomb',0x5B57:'ideograph',0x3001:'comma',0xFF08:'paren',0x30FC:'longmark',0x5D0:'alef',0x5D1:'bet'})
fb.setupNameTable({'familyName':'Fonts Layout Fixture','styleName':'Regular','uniqueFontIdentifier':'FontsLayoutFixture-Regular','fullName':'Fonts Layout Fixture Regular','psName':'FontsLayoutFixture-Regular','version':'Version 1.0','copyright':'Original geometric test data, MIT.'})
fb.setupOS2(sTypoAscender=800,sTypoDescender=-200,usWinAscent=900,usWinDescent=200)
fb.setupPost();fb.setupMaxp();fb.setupVerticalMetrics({name:(1000,100) for name in order});fb.setupVerticalHeader(ascent=900,descent=-100)
addOpenTypeFeaturesFromString(fb.font,'''languagesystem DFLT dflt;
languagesystem latn dflt;
languagesystem kana dflt;
languagesystem hebr dflt;
markClass acutecomb <anchor 50 0> @TOP;
feature liga {sub f i by fi;} liga;
feature vert {sub comma by comma.vert;sub paren by paren.vert;} vert;
feature vrt2 {sub comma by comma.vert;sub paren by paren.vert;} vrt2;
feature kern {pos A B -80;} kern;
feature mark {pos base A <anchor 320 700> mark @TOP;} mark;
table GDEF {LigatureCaretByIndex fi 1;} GDEF;
''')
for lookup in fb.font['GPOS'].table.LookupList.Lookup:
 if lookup.LookupType==4:
  anchor=lookup.SubTable[0].BaseArray.BaseRecord[0].BaseAnchor[0];anchor.Format=2;anchor.AnchorPoint=2
uvs=CmapSubtable.newSubtable(14);uvs.platformID=0;uvs.platEncID=5;uvs.language=0;uvs.cmap={};uvs.uvsDict={0xE0100:[(0x5B57,'ideograph.alt')],0xFE00:[(0x5B57,None)]};fb.font['cmap'].tables.append(uvs)
fb.font['head'].created=fb.font['head'].modified=3850080000
fb.font.recalcTimestamp=False;fb.font.save(root/'shaping.ttf')
