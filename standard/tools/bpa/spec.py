"""Read the clause catalogue of the standard from docs/50-system/30-standard (with subfolders)."""
import os
import re

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))
STD_DIR = os.path.join(REPO, 'docs', '50-system', '30-standard')
AREA_PROFILE = {'TIME': 'Bitemporal', 'PROOF': 'Proof', 'LGS': 'Governance', 'PROTO': 'Protocol', 'SYNC': 'Sync'}
MARK = re.compile(r'`\[Profile: (\w+)\]`')
RANGE = re.compile(r'VER-CONF-(\d{4})…(\d{4})')


def clauses():
    """Return {clause_id: {file, title, profile, must, range: (lo, hi) or None, text}}."""
    out = {}
    files = []
    for dirpath, _, names in os.walk(STD_DIR):
        files += [os.path.relpath(os.path.join(dirpath, n), STD_DIR) for n in names if n.endswith('.md')]
    for f in sorted(files):
        text = open(os.path.join(STD_DIR, f), encoding='utf-8').read()
        title_line = next((l for l in text.splitlines() if l.startswith('# ')), '')
        h1 = MARK.search(title_line)
        section = None
        blocks = re.split(r'(?m)^(?=#{2,3} )', text)
        for b in blocks:
            head = b.split('\n', 1)[0]
            if head.startswith('## '):
                m = MARK.search(head)
                section = m.group(1) if m else None
                continue
            m = re.match(r'### (STD-([A-Z]+)-\d{3}) — (.*)', head)
            if not m:
                continue
            cid, area, title = m.groups()
            pm = MARK.search(title)
            profile = (pm.group(1) if pm else section or (h1.group(1) if h1 else None)
                       or AREA_PROFILE.get(area, 'Core'))
            cv = re.search(r'\*\*Conformance vectors:\*\* (.*)', b)
            rng = RANGE.search(cv.group(1)) if cv else None
            clause = re.search(r'\*\*Clause:\*\*(.*?)(?=\n- \*\*Grammar|\Z)', b, re.S)
            ctext = clause.group(1) if clause else ''
            out[cid] = {'file': f, 'title': MARK.sub('', title).strip(), 'profile': profile,
                        'must': bool(re.search(r'\bMUST\b', ctext)),
                        'range': (int(rng.group(1)), int(rng.group(2))) if rng else None,
                        'block': b.rstrip() + '\n'}
    return out
