"""Adapter operations (STD-CONF-005) and a vector runner."""
from . import tables as _tables
from . import lenses as _lenses
from . import protection as _protection
from . import BpaError
from . import values
from .canon import canonical, content_id, nfc
import hashlib
import hmac

from .classes import linearize, effective_class, public, dispatch, type_selector
from .store import Store
from . import crypto, proofs, expr, addr, isa, sheets
from .lang import custom_value, check_value_type, parse_query as lang_parse_query
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


def _validate(i):
    """validate_value; since 1.0-rc1 with custom value types given in `value_types` (STD-FND-026)."""
    decls = i.get('value_types')
    if decls is None:
        return values.validate_value(i['type'], i.get('constraints'), i['value'])
    known = set()
    for d in decls:
        check_value_type(d, known)
        known.add(d['name'])
    by_name = {d['name']: d for d in decls}
    if i['type'] in by_name:
        return custom_value(by_name[i['type']], i['value'], i.get('constraints'))
    return values.validate_value(i['type'], i.get('constraints'), i['value'])


PURE = {
    'canonical_json': lambda i: canonical(i['value']),
    'content_id': lambda i: content_id(i['domain'], i['value']),
    'normalize_text': lambda i: nfc(i['text']),
    'check_name': lambda i: values.check_name(i['grammar'], i['text']),
    'validate_value': lambda i: _validate(i),
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
    # BPA 1.0-rc1, PH-1 step 1d (DEC-042)
    'public_key': lambda i: crypto.public_key(i['seed']),
    'sign': lambda i: crypto.sign(i['seed'], i['content_id'], i['signer'], i['key_id'], i.get('role', 'signer')),
    'verify_signature': lambda i: crypto.verify(i['signature'], i['content_id'], i['did'], i.get('at')),
    'aead_decrypt': lambda i: crypto.aead_decrypt(i['alg'], i['key'], i['nonce'], i['ciphertext'], i.get('aad', '')),
    'inclusion_proof': lambda i: proofs.inclusion_proof(i['entries'], i['key']),
    'verify_inclusion': lambda i: proofs.verify_inclusion(i['proof'], i.get('root'), i.get('dids')),
    # BPA 1.0-rc1, PH-1 step 1e-1 (DEC-043)
    'check_expr': lambda i: expr.check_expr(i),
    'eval_expr': lambda i: expr.eval_expr(i),
    'parse_uri': lambda i: addr.parse_uri(i['uri']),
    'parse_did': lambda i: addr.parse_did(i['did']),
    'key_did': lambda i: addr.key_did(i['public_key']),
    # BPA 1.0-rc1, PH-1 step 1e-2 (DEC-044)
    'instructions': lambda i: {'abi': isa.ABI, 'instructions': [{k: v for k, v in d.items() if k != 'clause'}
                                                                for d in isa.instructions()]},
    'profile_limits': lambda i: dict(isa.PROFILE_LIMITS[i['profile']], families=list(isa.PROFILE_FAMILIES[i['profile']]))
    if i.get('profile') in isa.PROFILE_LIMITS else (_ for _ in ()).throw(BpaError('MODEL.INVALID_VALUE', 'unknown profile')),
    'verify_consistency': lambda i: proofs.verify_consistency(i['commits'], i.get('from'), i.get('to'), i.get('dids')),
}
# BPA 1.0-rc2, PH-1 step 10b (DEC-066)
PURE.update(sheets.sheet_ops())
PURE['parse_query'] = lambda i: lang_parse_query(i['text'])
# BPA 1.0-rc8, PH-2 step 1c (DEC-091)
PURE['check_table'] = lambda i: _tables.check_table(i['table'])
# BPA 1.0-rc8, PH-2 step 1d (DEC-092)
PURE['classify_change'] = lambda i: _lenses.classify_change(i)
PURE['apply_lens'] = lambda i: _lenses.apply_lens(i)
PURE['check_lens'] = lambda i: _lenses.lens_check(i)
# BPA 1.0-rc8, PH-2 step 1e (DEC-093)
PURE['verify_disclosure'] = lambda i: _protection.verify_disclosure(i)

STATEFUL = {
    'reset': lambda s, i: s.reset(),
    'set_now': lambda s, i: s.set_now(i['now']),
    'set_limits': lambda s, i: s.set_limits(i.get('max_body_bytes'), i.get('max_derivation_inputs'),
                                            i.get('max_violations')),
    'load_genesis': lambda s, i: s.load_genesis(i['classes'], i.get('value_types')),
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
    'outbox': lambda s, i: s.outbox_list(i.get('as')),
    'ack': lambda s, i: s.ack(i['effect_ids']),
    'bulk': lambda s, i: s.bulk(i),
    # BPA 1.0-rc1, PH-1 step 1d (DEC-042)
    'node_did': lambda s, i: s.node_did(),
    'rotate_node_key': lambda s, i: s.rotate_node_key(i['seed']),
    'commit_record': lambda s, i: s.commit_record(i['branch'], i),
    'prove': lambda s, i: s.prove(i['branch'], i.get('object_id'), i.get('entry_id'), i),
    'consistency_proof': lambda s, i: s.consistency_proof(i['branch'], i['from'], i.get('to')),
    'tenant_root': lambda s, i: s.tenant_root(),
    'export_archive': lambda s, i: s.export_archive(),
    'import_archive': lambda s, i: s.import_archive(i['archive']),
    # BPA 1.0-rc1, PH-1 step 1e-1 (DEC-043)
    'resolve': lambda s, i: s.resolve(i['uri']),
    'query': lambda s, i: s.query(i),
    'did_document': lambda s, i: s.did_document(i['did']),
    # BPA 1.0-rc1, PH-1 step 1e-2 (DEC-044)
    'execute': lambda s, i: s.execute(i),
    'journal': lambda s, i: s.journal(i['process_id']),
    'metering': lambda s, i: s.metering_record(i['process_id']),
    'metering_total': lambda s, i: s.metering_total(),
    'replay': lambda s, i: s.replay(i['process_id'], i.get('tamper'), i.get('drop')),
    'verify_asset': lambda s, i: s.verify_asset(i.get('branch', 'main'), i['object_id'], i.get('verifier')),
    'bpu_descriptor': lambda s, i: s.bpu_descriptor(i.get('branch', 'main'), i['class']),
    'bpu_state': lambda s, i: s.bpu_state(i.get('branch', 'main'), i['object_id'], i),
    'verify_bpu': lambda s, i: s.verify_bpu(i.get('branch', 'main'), i['class']),
    # BPA 1.0-rc2, PH-1 step 10b (DEC-066)
    'cascade': lambda s, i: s.cascade(i.get('branch', 'main'), i['object_id'], i.get('ctx'), i),
    'explain_cascade': lambda s, i: s.explain_cascade(i.get('branch', 'main'), i['object_id'], i['property'],
                                                      i.get('ctx'), i),
    # BPA 1.0-rc2, PH-1 step 10c (DEC-067)
    'validate': lambda s, i: s.validate(i.get('branch', 'main'), i['changes'], i.get('process')),
    'decide': lambda s, i: s.decide(i.get('branch', 'main'), i['object_id'], i['port'], i.get('params'),
                                    i.get('ctx'), i),
    # BPA 1.0-rc2, PH-1 step 10d (DEC-068): the in-process UBTP binding
    'ubtp': lambda s, i: s.ubtp(i['messages'], i.get('session', 's1')),
    # BPA 1.0-rc3, PH-1 step 12a (DEC-071)
    'actions': lambda s, i: s.actions(i.get('branch', 'main'), i['object_id'], i.get('ctx'), i),
    # BPA 1.0-rc8, PH-2 step 1b (DEC-090): periods, matching and restatement
    'close_period': lambda s, i: s.close_period(i),
    'reopen_period': lambda s, i: s.reopen_period(i),
    'match': lambda s, i: s.match(i),
    'unmatch': lambda s, i: s.unmatch(i),
    'restate': lambda s, i: s.restate(i),
    # BPA 1.0-rc8, PH-2 step 1c (DEC-091): decision tables, deferred commits, approvals, simulation
    'decide_table': lambda s, i: s.decide_table(i),
    'pending_change': lambda s, i: s.pending_change(i),
    'approve': lambda s, i: s.approve(i),
    'simulate': lambda s, i: s.simulate(i),
    # BPA 1.0-rc8, PH-2 step 1d (DEC-092): workflows, sagas, quarantine, class changes, upstream changes
    'start_workflow': lambda s, i: s.start_workflow(i),
    'deliver_event': lambda s, i: s.deliver_event(i),
    'complete_task': lambda s, i: s.complete_task(i),
    'migrate_workflow': lambda s, i: s.migrate_workflow(i),
    'run_saga': lambda s, i: s.run_saga(i),
    'quarantine_policy': lambda s, i: s.quarantine_policy(i),
    'release_quarantine': lambda s, i: s.release_quarantine(i),
    'change_class': lambda s, i: s.change_class(i),
    'upstream_changes': lambda s, i: s.upstream_changes(i),
    'resolve_upstream': lambda s, i: s.resolve_upstream(i),
    # BPA 1.0-rc8, PH-2 step 1e (DEC-093): subject keys, erasure, relations, access, masking
    'subject_key': lambda s, i: s.subject_key(i),
    'key_availability': lambda s, i: s.key_availability(i),
    'erase_subject': lambda s, i: s.erase_subject(i),
    'stored_form': lambda s, i: s.stored_form(i),
    'disclose': lambda s, i: s.disclose(i),
    'lookup_key': lambda s, i: s.lookup_key(i),
    'relations': lambda s, i: s.relations(i),
    'check_access': lambda s, i: s.check_access(i),
    # BPA 1.0-rc8, PH-2 step 1f (DEC-094): connectors, notifications, addressing, REST facade
    'egress_allow': lambda s, i: s.egress_allow(i),
    'connector_stub': lambda s, i: s.connector_stub(i),
    'exchanges': lambda s, i: s.exchange_log(i),
    'principal': lambda s, i: s.principal(i),
    'resolve_path': lambda s, i: s.resolve_path(i),
    'rest': lambda s, i: s.rest(i),
    # BPA 1.0-rc8, PH-2 step 1g (DEC-095): views and the text index
    'resolve_view': lambda s, i: s.resolve_view(i),
    'index_lag': lambda s, i: s.set_index_lag(i),
    # BPA 1.0-rc9, PH-2 step 3a (DEC-098): currencies, FX rates, statements, projections
    'currency': lambda s, i: s.currency(i),
    'fx_rate': lambda s, i: s.fx_rate(i),
    'ingest_statement': lambda s, i: s.ingest_statement(i),
    'read_projection': lambda s, i: s.read_projection(i),
    'projection_lag': lambda s, i: s.projection_lag(i),
    'rebuild_projection': lambda s, i: s.rebuild_projection(i),
}

# Members of `ubtp` frames an implementation assigns (process identifiers), stripped like the
# implementation-dependent members (DEC-068).
UBTP_STRIPPED = ('process_id', 'pending_change_id')

OPERATIONS = set(PURE) | set(STATEFUL)
IMPLEMENTATION_DEPENDENT = ('version_hash', 'entry_hash', 'root', 'head', 'versions',
                            # since 1.0-rc1 (DEC-041): commit identifiers, transaction times,
                            # signatures and content identifiers over them
                            'commit_id', 'tx_time', 'parents', 'created_from', 'evaluated_at', 'preview',
                            'base', 'source_head', 'target_head', 'merged_commit', 'signature',
                            'provenance_ref', 'inputs_commitment', 'class_version',
                            # since 1.0-rc1 (DEC-042): signed records, proofs and archives
                            'signatures', 'proof', 'records', 'heads', 'archive', 'from_commit', 'to_commit',
                            # since 1.0-rc1 (DEC-043): query cursors bind a commit
                            'cursor',
                            # since 1.0-rc1 (DEC-044): journals, metering and verification records
                            'snapshots', 'logic_versions', 'commit_ids', 'changes_hash', 'cpu_us', 'peak_memory',
                            'wall_us', 'asset', 'record_id', 'json_schema',
                            # since 1.0-rc2 (DEC-066): sheet versions bound by a cascade
                            'sheet_version', 'bindings',
                            # since 1.0-rc8 (DEC-093): the random members of encrypted fields
                            'nonce', 'ct', 'salt', 'commitment')


def strip(value, extra=()):
    """Remove implementation-dependent members of a stateful result (STD-CONF-005, rule 6)."""
    if isinstance(value, dict):
        return {k: strip(v, extra) for k, v in value.items() if k not in IMPLEMENTATION_DEPENDENT and k not in extra}
    if isinstance(value, list):
        return [strip(v, extra) for v in value]
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
