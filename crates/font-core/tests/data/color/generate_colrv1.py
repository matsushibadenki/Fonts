"""MIT: Original COLRv1 paint fixtures. FontTools is development tooling only."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.colorLib.builder import buildCOLR, buildCPAL

root = Path(__file__).parent
fb = FontBuilder(1000, isTTF=True)
order = ['.notdef', 'space', 'red', 'blue'] + list('ABCDEFGHIJ')
fb.setupGlyphOrder(order)
glyphs = {}
for name in order:
    pen = TTGlyphPen(None)
    if name in ('red', 'blue'):
        x = y = 0 if name == 'red' else 250
        pen.moveTo((x, y))
        pen.lineTo((700, y))
        pen.lineTo((700, 700))
        pen.lineTo((x, 700))
        pen.closePath()
    glyphs[name] = pen.glyph()
fb.setupGlyf(glyphs)
fb.setupHorizontalMetrics({g: (800, 0) for g in order})
fb.setupHorizontalHeader(ascent=800, descent=-200)
fb.setupCharacterMap({32: 'space', 75: 'blue', **{ord(g): g for g in 'ABCDEFGHIJ'}})
fb.setupNameTable({'familyName': 'COLRv1 Paint Fixture', 'styleName': 'Regular',
                  'uniqueFontIdentifier': 'COLRv1PaintFixture',
                  'fullName': 'COLRv1 Paint Fixture', 'psName': 'COLRv1PaintFixture'})
fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
fb.setupPost()

def solid(index, alpha=1):
    return {'Format': 2, 'PaletteIndex': index, 'Alpha': alpha}

def glyph(paint, name='red'):
    return {'Format': 10, 'Glyph': name, 'Paint': paint}

def line(extend=0, alpha=1):
    return {'Extend': extend, 'ColorStop': [
        {'StopOffset': 0, 'PaletteIndex': 0, 'Alpha': alpha},
        {'StopOffset': 1, 'PaletteIndex': 1, 'Alpha': alpha}]}

def linear(extend=0, end=700, alpha=1):
    return {'Format': 4, 'ColorLine': line(extend, alpha),
            'x0': 0, 'y0': 0, 'x1': end, 'y1': 0, 'x2': 0, 'y2': 700}

paints = {
    'A': glyph(linear()),
    'B': glyph({'Format': 6, 'ColorLine': line(), 'x0': 350, 'y0': 350,
                'r0': 0, 'x1': 350, 'y1': 350, 'r1': 350}),
    'C': glyph({'Format': 8, 'ColorLine': line(), 'centerX': 350,
                'centerY': 350, 'startAngle': 0, 'endAngle': 360}),
    'D': glyph(linear(1, 200)),
    'E': glyph(linear(2, 200)),
    'F': {'Format': 12, 'Paint': glyph(solid(0)),
          'Transform': {'xx': .5, 'yx': 0, 'xy': 0, 'yy': .5, 'dx': 100, 'dy': 200}},
    'G': {'Format': 32, 'CompositeMode': 3,
          'SourcePaint': glyph(solid(1, .5), 'blue'), 'BackdropPaint': glyph(solid(0))},
    'H': glyph(solid(0xffff)),
    'I': glyph(linear(alpha=.5)),
    'J': {'Format': 1, 'Layers': [
        {'Format': 11, 'Glyph': 'A'}, glyph(solid(1, .5), 'blue')]},
}
fb.font['COLR'] = buildCOLR(paints, version=1, glyphMap=fb.font.getReverseGlyphMap(),
                           clipBoxes={'F': (200, 250, 400, 500)})
fb.font['CPAL'] = buildCPAL([
    [(1, 0, 0, 1), (0, 0, 1, 1)],
    [(0, 1, 0, 1), (1, 1, 0, 1)]])
fb.font.recalcTimestamp = False
fb.font['head'].created = fb.font['head'].modified = 3800000000
fb.save(root / 'colrv1-paints.ttf')
