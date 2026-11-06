"""MIT: Add original variable COLRv1 fixtures to the original variable outlines."""
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.varLib.builder import buildVarRegionList, buildVarData, buildVarStore
root=Path(__file__).parent
for source,target in [('named-gvar.ttf','color-gvar.ttf'),('named-cff2.otf','color-cff2.otf')]:
    font=TTFont(root/source,recalcTimestamp=False)
    font['CPAL']=buildCPAL([[(1,0,0,1)],[(0,1,0,.5)]])
    store=buildVarStore(buildVarRegionList([{'wght':(0,1,1)}],['wght','wdth']),[buildVarData([0],[[8192]],optimize=False)])
    font['COLR']=buildCOLR({'A':{'Format':10,'Glyph':'Aacute','Paint':{'Format':3,'PaletteIndex':0,'Alpha':0.5,'VarIndexBase':0}}},version=1,varStore=store)
    font.save(root/target)
