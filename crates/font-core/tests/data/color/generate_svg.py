"""MIT: Original SVG-in-OpenType fixtures; FontTools is development-only."""
from pathlib import Path
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.S_V_G_ import SVGDocument
from fontTools.colorLib.builder import buildCPAL

root = Path(__file__).parent
font = TTFont(root / 'colrv1-paints.ttf', recalcTimestamp=False)
del font['COLR']
for name in font['name'].names:
    name.string = name.toUnicode().replace('COLRv1', 'SVG').encode(name.getEncoding())
font['CPAL'] = buildCPAL([[(1, 0, 0, 1), (0, 0, 1, .5)],
                         [(0, 1, 0, 1), (1, 1, 0, 1)]])
svg = '''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
<defs>
 <linearGradient id="linear"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient>
 <radialGradient id="radial"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></radialGradient>
 <clipPath id="clip"><rect x="200" y="-500" width="200" height="250"/></clipPath>
 <rect id="shape" y="-700" width="700" height="700"/>
</defs>
<g id="glyph4"><rect y="-700" width="700" height="700" fill="red"/><rect x="250" y="-700" width="450" height="450" fill="blue" fill-opacity=".5"/></g>
<g id="glyph5"><use xlink:href="#shape" fill="url(#linear)"/></g>
<g id="glyph6"><use xlink:href="#shape" fill="url(#radial)"/></g>
<g id="glyph7" opacity=".5"><use xlink:href="#shape" fill="red"/><rect x="250" y="-700" width="450" height="450" fill="blue"/></g>
<g id="glyph8" clip-path="url(#clip)"><use xlink:href="#shape" fill="red" transform="translate(100 -200) scale(.5)"/></g>
<g id="glyph9" fill="var(--color1, magenta)" fill-opacity=".5"><use xlink:href="#shape"/></g>
<g id="glyph10"><use xlink:href="#shape" style="fill:rgb(0,128,0);stroke:blue;stroke-width:20;stroke-opacity:.5"/></g>
<g id="glyph11"><use xlink:href="#shape" fill="currentColor"/></g>
<g id="glyph12" transform="translate(100 -100)"><use xlink:href="#shape" fill="#ff8000"/></g>
<g id="glyph13"><path d="M0,-700H700V0H0Z M200,-500V-200H500V-500Z" fill="#00ffff" fill-rule="evenodd"/></g>
</svg>'''
for compressed, name in [(False, 'svg-paints.ttf'), (True, 'svg-paints-gzip.ttf')]:
    font['SVG '] = newTable('SVG ')
    font['SVG '].docList = [SVGDocument(svg, 4, 13, compressed)]
    font.save(root / name)
