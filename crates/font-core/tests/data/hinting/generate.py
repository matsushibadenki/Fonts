"""MIT: original geometric hinting fixtures; FontTools is test tooling only."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.misc.psCharStrings import T2CharString
from fontTools.cffLib import SubrsIndex
from fontTools.ttLib import TTFont
root=Path(__file__).parent
fb=FontBuilder(1000,isTTF=False)
fb.setupGlyphOrder(['.notdef','A'])
chars={'.notdef':T2CharString(program=[600,'endchar']), 'A':T2CharString(program=[600,0,40,'hstem',0,60,'vstem','hintmask',b'\xc0',-107,'callsubr','endchar'])}
fb.setupCFF('HintFixture', {'FullName':'Hint Fixture','FamilyName':'Hint Fixture','Weight':'Regular'}, chars, {'BlueValues':[-20,0,680,700], 'OtherBlues':[-220,-200], 'StdHW':40, 'StdVW':60, 'StemSnapH':[40], 'StemSnapV':[60], 'ForceBold':True})
fb.setupHorizontalMetrics({'.notdef':(600,0),'A':(600,0)})
fb.setupHorizontalHeader(ascent=800,descent=-200)
fb.setupCharacterMap({65:'A'})
fb.setupNameTable({'familyName':'Hint Fixture','styleName':'Regular','psName':'HintFixture','fullName':'Hint Fixture'})
fb.setupOS2(sTypoAscender=800,sTypoDescender=-200,usWinAscent=800,usWinDescent=200)
fb.setupPost()
top=fb.font['CFF '].cff.topDictIndex[0]
subrs=SubrsIndex();subrs.append(T2CharString(program=[0,0,'rmoveto',0,700,400,0,0,-700,'rlineto','return'],private=top.Private,globalSubrs=top.GlobalSubrs));top.Private.Subrs=subrs
for c in chars.values():c.private=top.Private;c.globalSubrs=top.GlobalSubrs
fb.save(root/'cff-hints.otf')
# The existing original variable fixture adds hints before its blend-based paths.
f=TTFont(root.parent/'variations/cff2.otf')
top=f['CFF2'].cff.topDictIndex[0]
for name in ['A','Aacute']:
 c=top.CharStrings[name];c.decompile();c.program=[0,40,'hstem','hintmask',b'\x80']+c.program
for fd in top.FDArray:
 fd.Private.BlueValues=[-20,0,680,700];fd.Private.StdHW=40
f.save(root/'cff2-hints.otf')
# A CID-keyed CFF with a non-identity CID (42) and a local subroutine.
f=TTFont(root/'cff-hints.otf')
from fontTools.cffLib import FDArrayIndex, FontDict, FDSelect
cff=f['CFF '].cff; top=cff.topDictIndex[0]
private=top.Private
fd=FontDict();fd.Private=private
array=FDArrayIndex();array.append(fd)
select=FDSelect();select.format=0;select.gidArray=[0,0]
cs=top.CharStrings
cs.charStrings['cid00042']=cs.charStrings.pop('A')
cs.fdArray=array;cs.fdSelect=select
for i in range(len(cs.charStringsIndex)):
 c=cs.charStringsIndex[i];c.private=private;c.fdSelectIndex=0
cs.charStringsIndex.fdArray=array;cs.charStringsIndex.fdSelect=select
cs.charStringsIndex.private=None
top.FDArray=array;top.FDSelect=select;top.ROS=('Adobe','Identity',0);top.CIDCount=43
top.charset=['.notdef','cid00042'];del top.Private
for t in f['cmap'].tables:
 if t.isUnicode():t.cmap={65:'cid00042'}
f.setGlyphOrder(['.notdef','cid00042'])
f.save(root/'cid-hints.otf')
