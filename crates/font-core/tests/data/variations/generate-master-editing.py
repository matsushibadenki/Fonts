"""MIT: original variable master editing fixtures. FontTools is test tooling only."""
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.fontBuilder import FontBuilder
from fontTools.misc.psCharStrings import T2CharString
from fontTools.cffLib import SubrsIndex
root=Path(__file__).parent
font=TTFont(root/'gvar.ttf')
del font['HVAR']
font['hmtx'].metrics['A']=(font['hmtx'].metrics['A'][0],-17)
fb=FontBuilder(font=font)
fb.setupVerticalMetrics({name:(1100,800-getattr(font['glyf'][name],'yMax',0)) for name in font.getGlyphOrder()})
fb.setupVerticalHeader(ascent=800,descent=-300)
variation=font['gvar'].variations['A'][1]
variation.coordinates[4]=(11,0)
variation.coordinates[5]=(variation.coordinates[5][0]+11,0)
variation.coordinates[6]=(0,31)
variation.coordinates[7]=(0,-17)
font.save(root/'moving-origins.ttf')
font=TTFont(root/'cff2.otf')
top=font['CFF2'].cff.topDictIndex[0]
assert len(top.VarStore.otVarStore.VarData[0].VarRegionIndex)==4
private=top.FDArray[0].Private
globals=font['CFF2'].cff.GlobalSubrs
private.Subrs=SubrsIndex()
private.Subrs.append(T2CharString(program=[30,0,0,40,40,0,'rrcurveto'],private=private,globalSubrs=globals))
globals.append(T2CharString(program=[-20,-20,10,30,20,20,'rrcurveto'],private=private,globalSubrs=globals))
program=[0,20,'hstem','hintmask',b'\x80',20,0,'rmoveto',
60,10,0,0,7,0,1,'blend',0,0,0,1,'blend',120,140,100,80,-60,'rrcurveto',
70,50,30,-20,'vhcurveto',-30,-50,-10,-20,'hhcurveto',
-107,'callsubr',-107,'callgsubr',-80,-40,'rlineto',
20,30,15,20,30,20,10,'hflex',
10,10,20,15,20,20,20,-10,10,'hflex1',
10,5,10,5,10,5,10,-5,10,-5,10,-5,50,'flex',
10,0,20,0,30,0,20,0,10,0,10,'flex1']
top.CharStrings['A']=T2CharString(program=program,private=private,globalSubrs=globals)
font.save(root/'nested-blend-cff2.otf')
top.FontMatrix=[.0012,0,0,.0008,.01,.02]
font.save(root/'invalid-matrix-cff2.otf')
font['head'].unitsPerEm=2048
top.FontMatrix=[1/2048,0,0,1/2048,0,0]
font.save(root/'matrix-cff2.otf')
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables.TupleVariation import TupleVariation
font=TTFont(root/'gvar.ttf')
pen=TTGlyphPen(font.getGlyphSet())
pen.addComponent('A',(1,0,0,1,40,20))
glyph=pen.glyph()
glyph.components[0].flags |= 0x200
font['glyf']['Aacute']=glyph
font['gvar'].variations['Aacute']=[TupleVariation({'wght':(0,1,1)},[(17,9),(0,0),(0,0),(0,0),(0,0)])]
font.save(root/'composite-gvar.ttf')
font=TTFont(root/'nested-blend-cff2.otf')
top=font['CFF2'].cff.topDictIndex[0]
private=top.FDArray[0].Private
globals=font['CFF2'].cff.GlobalSubrs
program=[20,0,'rmoveto',20,-40,0,0,0,1,'blend',10,0,0,0,0,0,0,0,0,15,'flex1']
top.CharStrings['A']=T2CharString(program=program,private=private,globalSubrs=globals)
font.save(root/'variable-flex-cff2.otf')
