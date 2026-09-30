"""The instruction set of ABI 1.0 (STD-ISA-001) and the error registry (STD-ERR-002), read from
the clause text of the standard, so that the oracle and the specification cannot drift."""
import re

from .spec import clauses

ABI = '1.0'
FAMILIES = ('commit', 'read', 'run', 'emit', 'search', 'call', 'ask')
# STD-CTX-004: families allowed per profile.
PROFILE_FAMILIES = {'strict': ('read', 'search', 'run', 'commit', 'emit'), 'standard': FAMILIES, 'batch': FAMILIES,
                    'simulation': FAMILIES, 'agent': FAMILIES}
PROFILE_LIMITS = {
    'strict': {'units': 1000000, 'instruction_calls': 1000, 'changes': 1000, 'depth': 16, 'connector_calls': 0,
               'model_calls': 0},
    'standard': {'units': 10000000, 'instruction_calls': 10000, 'changes': 100000, 'depth': 32, 'connector_calls': 50,
                 'model_calls': 5},
    'batch': {'units': 500000000, 'instruction_calls': 5000000, 'changes': 100000, 'depth': 32,
              'connector_calls': 10000, 'model_calls': 1000},
    'simulation': {'units': 1000000000, 'instruction_calls': 50000000, 'changes': 100000, 'depth': 32,
                   'connector_calls': 0, 'model_calls': 0},
    'agent': {'units': 20000000, 'instruction_calls': 20000, 'changes': 5000, 'depth': 16, 'connector_calls': 20,
              'model_calls': 50},
}
CELL = re.compile(r'^\s*\|\s*(Params|Result|Errors|Determinism|Cost|Authorization)\s*\|\s*(.*?)\s*\|\s*$', re.M)
ACTION = re.compile(r'`([a-z_]+)`')
_cache = {}


def _cost(text):
    m = re.match(r'(\d+)(?:\s*\+\s*(\d+)\s+per\s+(\w+))?', text)
    if not m:
        return {'base': 0, 'per_item': 0}
    out = {'base': int(m.group(1)), 'per_item': int(m.group(2) or 0)}
    if m.group(3):
        out['item'] = m.group(3)
    return out


def instructions():
    """[DAT-InstructionDecl] of ABI 1.0, in clause order."""
    if 'isa' in _cache:
        return _cache['isa']
    out = []
    for cid, c in sorted(clauses().items()):
        if not cid.startswith('STD-ISA-') or int(cid[-3:]) < 10:
            continue
        names = re.findall(r'`([a-z]+\.[a-z_]+)`', c['title'])
        cells = dict(CELL.findall(c['block']))
        for n in names:
            fam = n.split('.')[0]
            params = []
            for span in re.findall(r'`([^`]+)`', cells.get('Params', '')):
                pm = re.match(r'([a-z_]+)(\?)?(?::\s*(.+?))?\s*$', span)
                if not pm or not (pm.group(2) or pm.group(3)):
                    continue                     # plain words, such as selector names, are not parameters
                typ = (pm.group(3) or 'any').strip()
                opt = bool(pm.group(2)) or typ.endswith('?')
                typ = typ.rstrip('?').replace('\\|', '|').strip()
                if typ.startswith('{') or '|' in typ and ' ' in typ:
                    typ = 'map' if typ.startswith('{') else 'enum'
                params.append({'name': pm.group(1), 'type': typ, 'optional': opt})
            if not params and n in ('commit.reverse', 'commit.adjust'):
                sig = re.search(r'`%s\(([^)]*)\)`' % re.escape(n), c['block'])
                for part in (sig.group(1).split(',') if sig else []):
                    pn, _, pt = part.strip().partition(':')
                    params.append({'name': pn.strip().rstrip('?'), 'type': (pt.strip() or 'any').rstrip('?'),
                                   'optional': pn.strip().endswith('?') or pt.strip().endswith('?')})
            det = ACTION.search(cells.get('Determinism', ''))
            auth = ACTION.search(cells.get('Authorization', ''))
            out.append({'name': n, 'clause': cid, 'since': ABI, 'params': params,
                        'result': cells.get('Result', 'none').replace('`', ''),
                        'errors': re.findall(r'`([A-Z]+\.[A-Z_]+)`', cells.get('Errors', '')),
                        'determinism': det.group(1) if det else 'snapshot',
                        'cost': _cost(cells.get('Cost', '0')),
                        'authorization': auth.group(1) if auth else 'none',
                        'profiles': sorted(p for p, fams in PROFILE_FAMILIES.items() if fam in fams)})
    _cache['isa'] = out
    return out


def by_name():
    return {d['name']: d for d in instructions()}


def catchable():
    """{code: catchable} from the error registry (STD-ERR-002)."""
    if 'err' in _cache:
        return _cache['err']
    block = clauses()['STD-ERR-002']['block']
    out = {}
    for code, c in re.findall(r'^\s*\|\s*`([A-Z]+\.[A-Z_]+)`\s*\|[^|]*\|[^|]*\|\s*([a-z]+)[^|]*\|', block, re.M):
        out[code] = c == 'yes'
    _cache['err'] = out
    return out
