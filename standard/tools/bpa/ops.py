"""Adapter operations (STD-CONF-005) and a vector runner."""
from . import BpaError
from . import values
from .canon import canonical, content_id, nfc
import hashlib
import hmac

from .classes import linearize, effective_class, public, dispatch, type_selector
from .store import Store
from .tree import tree_root

ENVELOPE_REQUIRED = ('object_id', 'class', 'class_version', 'schema_version', 'kind', 'slug', 'valid')
ENTRY_REQUIRED = ('entry_id', 'class', 'class_version', 'ledger_key', 'valid_at')


def version_hash(envelope, body):
    for k in ENVELOPE_REQUIRED:
        if k not in envelope:
            raise BpaError('MODEL.INVALID_VALUE', 'envelope member %s' % k)
    env = dict(envelope, body_ref=content_id('ubos.chunk.v1', body))
    return {'body_ref': env['body_ref'], 'version_hash': content_id('ubos.version.v1', env)}


def entry_hash(entry, body):
    for k in ENTRY_REQUIRED:
        if k not in entry:
            raise BpaError('MODEL.INVALID_VALUE', 'entry member %s' % k)
    rec = {k: v for k, v in entry.items() if k not in ('seq', 'source_seq')}
    rec['body_ref'] = content_id('ubos.chunk.v1', body)
    return {'body_ref': rec['body_ref'], 'entry_hash': content_id('ubos.entry.v1', rec)}


def commit_id(commit):
    return content_id('ubos.commit.v1', {k: v for k, v in commit.items() if k != 'signatures'})


def blind_index(key, type_, constraints, value):
    """STD-OBJ-012: HMAC-SHA-256 under the tenant lookup key over the canonical JSON of the
    normalised value."""
    if not isinstance(key, str) or len(key) != 64 or any(c not in '0123456789abcdef' for c in key):
        raise BpaError('MODEL.INVALID_VALUE', 'a lookup key is 32 bytes in lowercase hexadecimal')
    norm = values.validate_value(type_, constraints, value)
    mac = hmac.new(bytes.fromhex(key), canonical(norm).encode('utf-8'), hashlib.sha256).hexdigest()
    return 'hmac-sha256:' + mac


PURE = {
    'canonical_json': lambda i: canonical(i['value']),
    'content_id': lambda i: content_id(i['domain'], i['value']),
    'normalize_text': lambda i: nfc(i['text']),
    'check_name': lambda i: values.check_name(i['grammar'], i['text']),
    'validate_value': lambda i: values.validate_value(i['type'], i.get('constraints'), i['value']),
    'decimal': lambda i: values.decimal_op(i['op'], i['a'], i.get('b'), i.get('scale'), i.get('rounding')),
    'money': lambda i: values.money_op(i['op'], i['a'], i['b']),
    'local_to_instant': lambda i: values.local_to_instant(i['date'], i['time'], i['tz'], i.get('disambiguation')),
    'classification_covers': lambda i: values.classification_covers(i['clearances'], i['level']),
    'version_hash': lambda i: version_hash(i['envelope'], i['body']),
    'entry_hash': lambda i: entry_hash(i['entry'], i['body']),
    'commit_id': lambda i: commit_id(i['commit']),
    'tree_root': lambda i: tree_root(i['entries']),
    'linearize': lambda i: linearize(i['classes'], i['class']),
    'effective_class': lambda i: public(effective_class(i['classes'], i['class'], i.get('extensions', ()))),
    'dispatch': lambda i: dispatch(i['classes'], i['class'], i['port'], i.get('after'), i.get('extensions', ())),
    'type_selector': lambda i: type_selector(i['classes'], i['class'], i['selector'], i.get('only', False)),
    'blind_index': lambda i: blind_index(i['key'], i['type'], i.get('constraints'), i['value']),
}

STATEFUL = {
    'reset': lambda s, i: s.reset(),
    'set_now': lambda s, i: s.set_now(i['now']),
    'set_limits': lambda s, i: s.set_limits(i.get('max_body_bytes'), i.get('max_derivation_inputs')),
    'load_genesis': lambda s, i: s.load_genesis(i['classes']),
    'create_branch': lambda s, i: s.create_branch(i['name'], i.get('from', 'main'), i.get('time'), i.get('parent_line')),
    'commit': lambda s, i: s.commit(i['branch'], i['changes'], i.get('process'), i.get('tx_time')),
    'read': lambda s, i: s.read(i['branch'], i['object_id'], i.get('asof'), i),
    'read_ledger': lambda s, i: s.read_ledger(i['branch'], i['class'], i['ledger_key'], i.get('asof'), i),
    'merge': lambda s, i: s.merge(i['source'], i['target'], i.get('resolutions'), i.get('process')),
    'diff': lambda s, i: s.diff(i['from'], i['to']),
    'export_state': lambda s, i: s.export_state(i['branch'], i),
    'list_objects': lambda s, i: s.list_objects(i.get('branch', 'main'), i['class'], i.get('only', False), i),
    'traverse': lambda s, i: s.traverse(i.get('branch', 'main'), i['object_id'], i['relation'], i),
    'class_at': lambda s, i: s.class_at(i.get('branch', 'main'), i['class'], i),
    # BPA 1.0-rc1, PH-1 step 1c (DEC-041)
    'branch_info': lambda s, i: s.branch_info(i['branch']),
    'set_parent_line': lambda s, i: s.set_parent_line(i['branch'], i.get('parent_line')),
    'commit_info': lambda s, i: s.commit_info(i['branch'], i),
    'read_range': lambda s, i: s.read_range(i['branch'], i['object_id'], i['from'], i.get('to'), i),
    'history': lambda s, i: s.history(i['branch'], i['object_id'], i.get('from'), i.get('to')),
    'tree_diff': lambda s, i: s.tree_diff(i['from'], i['to']),
    'merge_preview': lambda s, i: s.merge_preview(i['source'], i['target']),
    'change_set': lambda s, i: s.change_set(i),
    'tag': lambda s, i: s.tag(i['name'], i['branch'], i, i.get('signer')),
    'delete_tag': lambda s, i: s.delete_tag(i['name']),
    'tag_info': lambda s, i: s.tag_info(i['name']),
    'release': lambda s, i: s.release(i),
    'revert': lambda s, i: s.revert(i),
    'process_info': lambda s, i: s.process_info(i['process_id']),
    'lineage': lambda s, i: s.lineage(i['process_id'], i.get('direction', 'ancestors')),
    'derivation': lambda s, i: s.derivation(i['branch'], i.get('object_id'), i.get('entry_id'), i),
    'impact': lambda s, i: s.impact(i['branch'], i['time']),
    'outbox': lambda s, i: s.outbox_list(),
    'ack': lambda s, i: s.ack(i['effect_ids']),
    'bulk': lambda s, i: s.bulk(i),
}

OPERATIONS = set(PURE) | set(STATEFUL)
IMPLEMENTATION_DEPENDENT = ('version_hash', 'entry_hash', 'root', 'head', 'versions',
                            # since 1.0-rc1 (DEC-041): commit identifiers, transaction times,
                            # signatures and content identifiers over them
                            'commit_id', 'tx_time', 'parents', 'created_from', 'evaluated_at', 'preview',
                            'base', 'source_head', 'target_head', 'merged_commit', 'signature',
                            'provenance_ref', 'inputs_commitment', 'class_version')


def strip(value):
    """Remove implementation-dependent members of a stateful result (STD-CONF-005, rule 6)."""
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if k not in IMPLEMENTATION_DEPENDENT}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value


def pointer(doc, ptr):
    for part in [p for p in ptr.split('/') if p != '']:
        part = part.replace('~1', '/').replace('~0', '~')
        doc = doc[int(part)] if isinstance(doc, list) else doc[part]
    return doc


def resolve(value, results):
    if isinstance(value, dict):
        if set(value) == {'$ref'}:
            label, _, ptr = value['$ref'].partition('/')
            return pointer(results[label], ptr)
        return {k: resolve(v, results) for k, v in value.items()}
    if isinstance(value, list):
        return [resolve(v, results) for v in value]
    return value


def execute(store, op, inp):
    """Run one operation; return ('ok', result) or ('error', code, detail)."""
    try:
        if op in PURE:
            return ('ok', PURE[op](inp))
        if op in STATEFUL:
            return ('ok', STATEFUL[op](store, inp))
        raise BpaError('MODEL.INVALID_VALUE', 'unknown operation %s' % op)
    except BpaError as e:
        return ('error', e.code, e.detail)


def subset(expected, actual):
    if isinstance(expected, dict):
        return isinstance(actual, dict) and all(k in actual and subset(v, actual[k]) for k, v in expected.items())
    if isinstance(expected, list):
        return isinstance(actual, list) and len(expected) == len(actual) and \
            all(subset(e, a) for e, a in zip(expected, actual))
    return expected == actual


def parse_text(text):
    import json

    def pairs(items):
        keys = [k for k, _ in items]
        if len(set(keys)) != len(keys):
            raise BpaError('MODEL.INVALID_VALUE', 'duplicate member name')
        return dict(items)
    try:
        return json.loads(text, object_pairs_hook=pairs)
    except ValueError:
        raise BpaError('MODEL.INVALID_VALUE', 'not JSON')


def step_input(step, results):
    if 'input_text' in step:
        return ('text', step['input_text'])
    return ('value', resolve(step.get('input', {}), results))


def run_step(store, step, results):
    kind, inp = step_input(step, results)
    if kind == 'text':
        try:
            inp = parse_text(inp)
        except BpaError as e:
            return ('error', e.code, e.detail)
    return execute(store, step['op'], inp)


def run_vector(vec):
    """Run a vector against the oracle; return the list of failures (empty when it passes)."""
    store, results, failures = Store(), {}, []
    for phase in ('setup', 'steps'):
        for n, step in enumerate(vec[phase], 1):
            out = run_step(store, step, results)
            label = step.get('label', '%s%d' % (phase[0], n))
            exp = step.get('expect')
            if out[0] == 'ok':
                results[label] = out[1]
            if exp is None:
                if out[0] == 'error':
                    failures.append((label, 'unexpected error %s %s' % (out[1], out[2])))
                continue
            if 'error_code' in exp:
                if out[0] != 'error' or out[1] != exp['error_code']:
                    failures.append((label, 'expected %s, got %r' % (exp['error_code'], out[:2])))
                elif 'details' in exp and not subset(exp['details'], out[2]):
                    failures.append((label, 'error details mismatch'))
            elif out[0] == 'error':
                failures.append((label, 'unexpected error %s %s' % (out[1], out[2])))
            elif 'same_as' in exp:
                l, _, p = exp['same_as'].partition('/')
                if out[1] != pointer(results[l], p):
                    failures.append((label, 'not the same as %s' % exp['same_as']))
            elif exp.get('match', 'exact') == 'subset':
                if not subset(exp['result'], out[1]):
                    failures.append((label, 'subset mismatch'))
            elif canonical(exp['result']) != canonical(strip(out[1]) if step['op'] in STATEFUL else out[1]):
                failures.append((label, 'mismatch'))
    return failures
