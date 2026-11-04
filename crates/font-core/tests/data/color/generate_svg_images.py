"""MIT: Original embedded-raster SVG fixture. FontTools/Pillow are development-only."""
from pathlib import Path
from io import BytesIO
import base64
from PIL import Image
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.S_V_G_ import SVGDocument

root = Path(__file__).parent
font = TTFont(root / 'svg-paints.ttf', recalcTimestamp=False)
def uri(fmt):
    image = Image.new('RGBA', (2, 2))
    image.putdata([(240, 30, 20, 128), (20, 60, 240, 255), (20, 200, 60, 255), (240, 180, 20, 0)])
    out = BytesIO()
    (image.convert('RGB') if fmt == 'JPEG' else image).save(out, fmt)
    return 'data:image/' + fmt.lower() + ';base64,' + base64.b64encode(out.getvalue()).decode()
svg = f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
<defs><image id="raster" xlink:href="{uri('PNG')}" y="-700" width="700" height="700"/><clipPath id="clip"><rect y="-700" width="350" height="700"/></clipPath></defs>
<g id="glyph4"><use xlink:href="#raster"/></g>
<g id="glyph5" opacity=".5" clip-path="url(#clip)"><use xlink:href="#raster" transform="translate(50 -50)"/></g>
<g id="glyph6"><image href="{uri('JPEG')}" y="-700" width="700" height="700" preserveAspectRatio="none"/></g>
</svg>'''
font['SVG '] = newTable('SVG ')
font['SVG '].docList = [SVGDocument(svg, 4, 6, True)]
font.save(root / 'svg-images.ttf')
