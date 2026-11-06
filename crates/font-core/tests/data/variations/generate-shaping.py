"""MIT: Original ligature, IVS and variable kerning for the original variable fixtures."""
from pathlib import Path
from fontTools.ttLib import TTFont, newTable
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.ttLib.tables import otTables
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
from fontTools.varLib.builder import buildVarRegionList, buildVarData, buildVarStore
root=Path(__file__).parent
for source,target in [('named-gvar.ttf','shaping-gvar.ttf'),('named-cff2.otf','shaping-cff2.otf')]:
    font=TTFont(root/source,recalcTimestamp=False)
    addOpenTypeFeaturesFromString(font,'feature liga {sub A space A by Aacute;} liga; feature kern {pos A A -80;} kern;')
    font['GDEF']=newTable('GDEF');g=otTables.GDEF();g.Version=0x10003
    g.GlyphClassDef=g.AttachList=g.LigCaretList=g.MarkAttachClassDef=g.MarkGlyphSetsDef=None
    g.VarStore=buildVarStore(buildVarRegionList([{'wght':(0,1,1)}],['wght','wdth']),[buildVarData([0],[[-40]],optimize=False)])
    font['GDEF'].table=g
    pair=font['GPOS'].table.LookupList.Lookup[0].SubTable[0]
    device=otTables.Device();device.StartSize=0;device.EndSize=0;device.DeltaFormat=0x8000
    pair.ValueFormat1|=0x40;pair.PairSet[0].PairValueRecord[0].Value1.XAdvDevice=device
    uvs=CmapSubtable.newSubtable(14);uvs.platformID=0;uvs.platEncID=5;uvs.language=0;uvs.cmap={};uvs.uvsDict={0xE0100:[(65,'Aacute')]}
    font['cmap'].tables.append(uvs)
    font.save(root/target)
