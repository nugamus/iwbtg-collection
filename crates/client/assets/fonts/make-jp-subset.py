"""Rebuilds NotoSansJP-subset.ttf. Run from a folder holding the Noto Sans JP variable font as dl/NotoSansJP.ttf (github.com/google/fonts, ofl/notosansjp); needs fonttools. Widen the kanji lead-byte range (0x99..0xEA) for JIS level 2."""
import os, sys
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools import subset
sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
import fallback_metrics
f=TTFont('dl/NotoSansJP.ttf')
f=instantiateVariableFont(f,{'wght':500})
chars=set()
# kana, punctuation, fullwidth forms
for a,b in [(0x3000,0x30FF),(0x31F0,0x31FF),(0xFF00,0xFFEF),(0x2010,0x2027),(0x25A0,0x25EF)]:
    chars|=set(range(a,b+1))
# JIS X 0208 level 1 kanji (lead bytes 0x88..0x98)
for lead in range(0x88,0x99):
    for trail in range(0x40,0xFD):
        if trail==0x7F: continue
        try: chars.add(ord(bytes([lead,trail]).decode('shift_jis')))
        except: pass
opts=subset.Options(); opts.layout_features=['vert','palt']; opts.name_IDs=['*']; opts.notdef_outline=True
opts.hinting=False; opts.desubroutinize=True
s=subset.Subsetter(opts); s.populate(unicodes=sorted(chars)); s.subset(f)
n=f['name']
for rec in list(n.names):
    if rec.nameID in (16,17,25): n.removeNames(nameID=rec.nameID)
for pid,eid,lid in [(3,1,0x409),(1,0,0)]:
    n.setName('Noto Sans JP',1,pid,eid,lid); n.setName('Regular',2,pid,eid,lid)
    n.setName('Noto Sans JP',4,pid,eid,lid); n.setName('NotoSansJP-Regular',6,pid,eid,lid)
if 'STAT' in f: del f['STAT']
fallback_metrics.apply(f)  # Barlow's ascent and descent (#546)
f.save('NotoSansJP-subset.ttf')
print(len(chars))
