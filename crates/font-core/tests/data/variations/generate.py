"""MIT: original geometric fixtures, generated with FontTools (test tooling only)."""
from pathlib import Path
from tempfile import TemporaryDirectory
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.designspaceLib import DesignSpaceDocument, AxisDescriptor, SourceDescriptor
from fontTools.varLib import build
root=Path(__file__).parent
with TemporaryDirectory() as tmp:
 for cff in [False,True]:
  ds=DesignSpaceDocument()
  axis=AxisDescriptor();axis.name='Weight';axis.tag='wght';axis.minimum=100;axis.default=400;axis.maximum=900;ds.addAxis(axis)
  axis=AxisDescriptor();axis.name='Width';axis.tag='wdth';axis.minimum=75;axis.default=100;axis.maximum=125;ds.addAxis(axis)
  for weight,width,stretch in [(100,200,100),(400,400,100),(900,700,100),(400,300,75),(400,500,125)]:
   fb=FontBuilder(1000,isTTF=not cff);order=['.notdef','A','space','Aacute'];fb.setupGlyphOrder(order)
   pens={}
   for g in order:
    pen=T2CharStringPen(1000,None) if cff else TTGlyphPen(None)
    if g in ['A','Aacute']:
     pen.moveTo((20,0));pen.lineTo((width,0));pen.lineTo((width,700));pen.lineTo((20,700));pen.closePath()
    pens[g]=pen.getCharString() if cff else pen.glyph()
   if cff: fb.setupCFF('VariableFixture',{'FullName':'Variable Fixture','FamilyName':'Variable Fixture','Weight':'Regular'},pens,{})
   else: fb.setupGlyf(pens)
   fb.setupHorizontalMetrics({g:(width+100,20 if g in ['A','Aacute'] else 0) for g in order})
   fb.setupHorizontalHeader(ascent=800+(weight-400)//5,descent=-200)
   fb.setupCharacterMap({65:'A',32:'space',193:'Aacute'})
   fb.setupNameTable({'familyName':'Variable Fixture','styleName':str(weight),'uniqueFontIdentifier':f'Fixture{weight}','fullName':f'Variable Fixture {weight}','psName':f'VariableFixture-{weight}'})
   fb.setupOS2(sTypoAscender=800+(weight-400)//5,sTypoDescender=-200,usWinAscent=1000,usWinDescent=200)
   fb.setupPost();path=Path(tmp)/f'{weight}-{stretch}.otf';fb.save(path)
   src=SourceDescriptor();src.path=str(path);src.name=f'{weight}-{stretch}';src.location={'Weight':weight,'Width':stretch};src.copyInfo=weight==400 and stretch==100;src.copyLib=src.copyInfo;src.copyFeatures=src.copyInfo;ds.addSource(src)
  ds.write(Path(tmp)/'source.designspace')
  font,_,_=build(ds)
  from fontTools.ttLib import newTable
  font['avar']=newTable('avar');font['avar'].segments={'wght':{-1:-1,0:0,0.5:0.75,1:1},'wdth':{-1:-1,0:0,1:1}}
  font.save(root/('cff2.otf' if cff else 'gvar.ttf'))

# Named-instance fixtures keep the original variable outlines unchanged.
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._f_v_a_r import NamedInstance
for source, target in [('gvar.ttf','named-gvar.ttf'),('cff2.otf','named-cff2.otf')]:
 font=TTFont(root/source)
 font['fvar'].instances=[]
 for n,(en,ja,zh,weight,width) in enumerate([
  ('Regular','標準','常规',400,100),('Bold','太字','粗体',900,100),('Condensed','長体','窄体',400,75)]):
  name_id=500+n
  for language,value in [(0x409,en),(0x411,ja),(0x804,zh)]:
   font['name'].setName(value,name_id,3,1,language)
  ps_id=510+n
  font['name'].setName('VariableFixture-'+en,ps_id,3,1,0x409)
  instance=NamedInstance();instance.subfamilyNameID=name_id;instance.flags=0
  instance.coordinates={'wght':weight,'wdth':width};instance.postscriptNameID=ps_id if n<2 else 0xffff
  font['fvar'].instances.append(instance)
 font.save(root/target)
