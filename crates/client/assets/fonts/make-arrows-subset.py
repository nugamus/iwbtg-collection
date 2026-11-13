"""Rebuilds NotoSansMath-subset.ttf: the basic arrows (U+2190..U+2199, ← ↑ → ↓ ↔ ↕ ↖ ↗ ↘ ↙), which
Barlow, Noto Sans Symbols 2 and Noto Sans lack. Run from a folder holding Noto Sans Math as
dl/NotoSansMath-Regular.ttf (github.com/google/fonts, ofl/notosansmath); needs fonttools."""
from fontTools.ttLib import TTFont
from fontTools import subset

f = TTFont('dl/NotoSansMath-Regular.ttf')
chars = range(0x2190, 0x219A)
opts = subset.Options()
opts.layout_features = []
opts.name_IDs = ['*']
opts.notdef_outline = True
opts.drop_tables += ['MATH']
s = subset.Subsetter(opts)
s.populate(unicodes=chars)
s.subset(f)
f.save('NotoSansMath-subset.ttf')
print(len(f.getBestCmap()), 'characters')
