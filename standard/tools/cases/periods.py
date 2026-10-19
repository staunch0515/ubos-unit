"""Vector cases of BPA 1.0-rc8 for money and periods (PH-2 step 1b, DEC-090): period
transitions (STD-KIND-025), close artefacts (STD-KIND-026), matching (STD-KIND-027), match
records (STD-KIND-028), re-evaluation modes (STD-TIME-041) and restatement (STD-TIME-042).
Not required of the PH-0 prototype."""
from .common import CLASSES, cls, field, uid, commit, create, base, usd, gl, setup

PER_MAR = create(uid(50), 'FiscalPeriod', '2026-03', {'start': '2026-03-01', 'end': '2026-04-01', 'state': 'open'})
PER_APR = create(uid(51), 'FiscalPeriod', '2026-04', {'start': '2026-04-01', 'end': '2026-05-01', 'state': 'open'})
MAR = '2026-03-15T10:00:00.000000Z'
APR = '2026-04-10T09:00:00.000000Z'

BOOK = cls('BookLine', [field('account', 'text', True), field('reference', 'text'), field('amount', 'money', True)],
           kind='ledger', ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': False})
BANK = cls('BankLine', [field('account', 'text', True), field('reference', 'text'), field('amount', 'money', True)],
           kind='ledger', ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': False})
MATCH_CLASSES = CLASSES + [BOOK, BANK]


def now(t):
    return {'op': 'set_now', 'input': {'now': t}}


def upd(oid, label, body):
    return {'op': 'update', 'object_id': oid, 'base': base(label, oid), 'body': body}


def op(op_, label=None, error=None, **inp):
    s = {'op': op_, 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def read(oid, label=None, **kw):
    return op('read', label=label, branch='main', object_id=oid, **kw)


def line(klass, n, ref, amount, valid_at='2026-03-10', account='1010', currency='USD'):
    return {'op': 'append', 'class': klass, 'entry_id': uid(n), 'ledger_key': {'account': account},
            'valid_at': valid_at, 'body': {'account': account, 'reference': ref,
                                          'amount': {'amount': amount, 'currency': currency}}}


def side(klass, account='1010'):
    return {'class': klass, 'ledger_key': {'account': account}}


def rule(rid, kind='one_to_one', equal=(('reference', 'reference'),), tolerance='0', window_days=0, **extra):
    r = {'id': rid, 'kind': kind, 'equal': [list(p) for p in equal], 'amount': ['amount', 'amount'],
         'tolerance': tolerance, 'window_days': window_days}
    r.update(extra)
    return r


def cases(g):
    def V(clause, desc, steps, now_=MAR, classes=None, also=()):
        g.vector(clause, desc, setup=setup(now_, classes), steps=steps, prototype=False, also=also)

    two = [gl(uid(10), '1010', '100.00'), gl(uid(11), '4000', '-100.00')]
    closing = upd(uid(50), 'p', {'state': 'closing'})

    # ---- STD-KIND-025 period transitions (1.0-rc8) ----
    V('STD-KIND-025', 'an update moves a period from open to closing and back', [
        commit([PER_MAR], label='p'), commit([closing], label='c'),
        commit([upd(uid(50), 'c', {'state': 'open'})]), read(uid(50))])
    V('STD-KIND-025', 'an update that closes a period fails; close_period closes it', [
        commit([PER_MAR], label='p'), commit([closing], label='c'),
        commit([upd(uid(50), 'c', {'state': 'closed'})], error='MODEL.CONSTRAINT_VIOLATED')])
    V('STD-KIND-025', 'an update from open to closed fails', [
        commit([PER_MAR], label='p'), commit([upd(uid(50), 'p', {'state': 'closed'})], error='MODEL.CONSTRAINT_VIOLATED')])
    V('STD-KIND-025', 'appends into a period in closing commit (soft close)', [
        commit([PER_MAR], label='p'), commit([closing]), commit(two)])
    V('STD-KIND-025', 'a closed period is reopened with a reason and accepts appends again', [
        commit([PER_MAR], label='p'), commit([closing]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60)),
        commit(two, error='LEDG.PERIOD_CLOSED'),
        op('reopen_period', branch='main', period=uid(50), reason='late supplier invoice'),
        commit(two), read(uid(50))])
    V('STD-KIND-025', 'a reopening states its reason', [
        commit([PER_MAR], label='p'), commit([closing]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60)),
        op('reopen_period', branch='main', period=uid(50), error='MODEL.DECLARATION_INCOMPLETE')])
    V('STD-KIND-025', 'only a closed period is reopened', [
        commit([PER_MAR]), op('reopen_period', branch='main', period=uid(50), reason='x',
                              error='MODEL.CONSTRAINT_VIOLATED')])
    V('STD-KIND-025', 'a correction after the close posts in the open period with the valid time of the original', [
        commit([PER_MAR, PER_APR], label='p'), commit(two), commit([closing]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60)),
        now(APR),
        commit([{'op': 'reverse', 'entry_id': uid(10), 'new_entry_id': uid(12)},
                {'op': 'reverse', 'entry_id': uid(11), 'new_entry_id': uid(13)}]),
        op('read_ledger', branch='main', **{'class': 'GlEntry', 'ledger_key': {'account': '1010'}})])
    V('STD-KIND-025', 'entries merged into a period closed by close_period fail', [
        commit([PER_MAR], label='p'), op('create_branch', name='feature/a', **{'from': 'main'}),
        commit(two, branch='feature/a'), commit([closing]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60)),
        op('merge', source='feature/a', target='main', error='LEDG.PERIOD_CLOSED')], also=('STD-VER-055',))

    # ---- STD-KIND-026 close artefacts ----
    V('STD-KIND-026', 'close_period closes the period, writes the close artefact and tags the commit', [
        commit([PER_MAR], label='p'), commit(two), commit([closing]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60)),
        read(uid(50)), read(uid(60)), op('tag_info', name='close-2026-03-1')])
    V('STD-KIND-026', 'the artefact holds the balances per key of the entries posted before the end', [
        commit([PER_MAR, PER_APR], label='p'), commit(two),
        commit([gl(uid(20), '1010', '30.00', group='g2'), gl(uid(21), '5000', '-30.00', group='g2')]),
        commit([closing]), now(APR),
        commit([gl(uid(22), '1010', '7.00', group='g3'), gl(uid(23), '4000', '-7.00', group='g3')]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60))])
    V('STD-KIND-026', 'a period that is not in closing is not closed', [
        commit([PER_MAR]), op('close_period', branch='main', period=uid(50), artefact_id=uid(60),
                              error='MODEL.CONSTRAINT_VIOLATED')])
    V('STD-KIND-026', 'the artefact keeps the figures as closed after a later correction', [
        commit([PER_MAR, PER_APR], label='p'), commit(two), commit([closing]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60), label='cl'), now(APR),
        commit([{'op': 'reverse', 'entry_id': uid(10), 'new_entry_id': uid(12)},
                {'op': 'reverse', 'entry_id': uid(11), 'new_entry_id': uid(13)}]),
        dict(read(uid(60)), match='subset')])
    V('STD-KIND-026', 'a close artefact is not created by a commit', [
        commit([create(uid(60), 'CloseArtefact', 'x', {'period': uid(50)})], error='LOGIC.FORBIDDEN_TARGET')])
    V('STD-KIND-026', 'a close artefact is not changed by a commit', [
        commit([PER_MAR], label='p'), commit([closing]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60)),
        read(uid(60), label='a'),
        commit([{'op': 'update', 'object_id': uid(60), 'base': {'$ref': 'a/version_hash'}, 'body': {'sequence': 2}}],
               error='LOGIC.FORBIDDEN_TARGET')])
    V('STD-KIND-026', 'a second close after a reopening writes a second artefact; the first stays', [
        commit([PER_MAR], label='p'), commit(two), commit([closing], label='c'),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(60)),
        op('reopen_period', branch='main', period=uid(50), reason='audit adjustment'),
        commit([gl(uid(20), '1010', '5.00', group='g2'), gl(uid(21), '4000', '-5.00', group='g2')]),
        op('read', branch='main', object_id=uid(50), label='r'),
        commit([{'op': 'update', 'object_id': uid(50), 'base': {'$ref': 'r/version_hash'}, 'body': {'state': 'closing'}}]),
        op('close_period', branch='main', period=uid(50), artefact_id=uid(61)),
        read(uid(60)), read(uid(61)), op('tag_info', name='close-2026-03-2')])
    V('STD-KIND-026', 'close_period names an object of a period class', [
        commit([create(uid(1), body={'name': 'Acme'})]),
        op('close_period', branch='main', period=uid(1), artefact_id=uid(60), error='MODEL.INVALID_VALUE')])

    # ---- STD-KIND-027 matching ----
    def M(desc, lines, rules, error=None, also=(), **kw):
        steps = [commit(lines)] if lines else []
        steps.append(op('match', branch='main', match_set='bank-1010', left=side('BookLine'),
                        right=side('BankLine'), rules=rules, error=error, **kw))
        V('STD-KIND-027', desc, steps, classes=MATCH_CLASSES, also=also)

    M('one-to-one matching on the reference and the exact amount', [
        line('BookLine', 10, 'INV-1', '100.00'), line('BookLine', 11, 'INV-2', '50.00'),
        line('BankLine', 20, 'INV-2', '50.00'), line('BankLine', 21, 'INV-1', '100.00')], [rule('exact')])
    M('a tolerance accepts a small difference in amount', [
        line('BookLine', 10, 'INV-1', '100.00'), line('BankLine', 20, 'INV-1', '99.95')],
      [rule('tol', tolerance='0.10')])
    M('without a tolerance a difference in amount is a break', [
        line('BookLine', 10, 'INV-1', '100.00'), line('BankLine', 20, 'INV-1', '99.95')], [rule('exact')])
    M('a date window bounds the distance of the valid dates', [
        line('BookLine', 10, 'INV-1', '100.00', valid_at='2026-03-01'),
        line('BankLine', 20, 'INV-1', '100.00', valid_at='2026-03-04'),
        line('BookLine', 11, 'INV-2', '40.00', valid_at='2026-03-01'),
        line('BankLine', 21, 'INV-2', '40.00', valid_at='2026-03-09')], [rule('window', window_days=3)])
    M('rules apply in priority order; a later rule sees only what is left', [
        line('BookLine', 10, 'INV-1', '100.00'), line('BankLine', 20, 'INV-1', '100.00'),
        line('BookLine', 11, 'INV-2', '60.00'), line('BankLine', 21, 'X-77', '60.00')],
      [rule('by-reference'), rule('by-amount', equal=())])
    M('one-to-many matching groups the lines with equal fields whose sum is within the tolerance', [
        line('BookLine', 10, 'INV-1', '100.00'), line('BankLine', 20, 'INV-1', '60.00'),
        line('BankLine', 21, 'INV-1', '40.00'), line('BankLine', 22, 'INV-9', '40.00')],
      [rule('split', kind='one_to_many')])
    M('many-to-one matching groups the lines of the left side', [
        line('BookLine', 10, 'BATCH-3', '25.00'), line('BookLine', 11, 'BATCH-3', '75.00'),
        line('BankLine', 20, 'BATCH-3', '100.00')], [rule('batch', kind='many_to_one')])
    M('a group whose sum is outside the tolerance is a break', [
        line('BookLine', 10, 'INV-1', '100.00'), line('BankLine', 20, 'INV-1', '60.00'),
        line('BankLine', 21, 'INV-1', '30.00')], [rule('split', kind='one_to_many')])
    M('negate compares the left amount with the negated right amount', [
        line('BookLine', 10, 'INV-1', '100.00'), line('BankLine', 20, 'INV-1', '-100.00')],
      [rule('sign', negate=True)])
    M('amounts in different currencies do not match', [
        line('BookLine', 10, 'INV-1', '100.00'), line('BankLine', 20, 'INV-1', '100.00', currency='EUR')],
      [rule('exact')])
    M('a rule names its kind', [], [{'id': 'r', 'amount': ['amount', 'amount']}], error='MODEL.DECLARATION_INCOMPLETE')
    M('rule ids are unique', [], [rule('a'), rule('a')], error='MODEL.INVALID_VALUE')
    V('STD-KIND-027', 'matching is deterministic: a second run gives the same matches and breaks', [
        commit([line('BookLine', 10, 'INV-1', '10.00'), line('BookLine', 11, 'INV-1', '10.00'),
                line('BankLine', 20, 'INV-1', '10.00'), line('BankLine', 21, 'INV-1', '10.00'),
                line('BankLine', 22, 'INV-1', '10.00')]),
        op('match', label='m1', branch='main', match_set='s', left=side('BookLine'), right=side('BankLine'),
           rules=[rule('exact')]),
        dict(op('match', branch='main', match_set='s', left=side('BookLine'), right=side('BankLine'),
                rules=[rule('exact')]), same_as='m1')], classes=MATCH_CLASSES)

    # ---- STD-KIND-028 match records ----
    lines2 = [line('BookLine', 10, 'INV-1', '100.00'), line('BankLine', 20, 'INV-1', '100.00'),
              line('BookLine', 11, 'INV-2', '50.00'), line('BankLine', 21, 'INV-2', '50.00')]

    def run(commit_=True, label=None):
        return op('match', label=label, branch='main', match_set='bank-1010', left=side('BookLine'),
                  right=side('BankLine'), rules=[rule('exact')], commit=commit_)

    def records():
        return op('read_ledger', branch='main', **{'class': 'MatchRecord', 'ledger_key': {'match_set': 'bank-1010'}})

    V('STD-KIND-028', 'a committed run appends one match record per match', [
        commit(lines2), run(), records()], classes=MATCH_CLASSES)
    V('STD-KIND-028', 'matched lines are not matched again', [
        commit(lines2), run(), commit([line('BookLine', 12, 'INV-3', '5.00'), line('BankLine', 22, 'INV-3', '5.00')]),
        run(commit_=False)], classes=MATCH_CLASSES)
    V('STD-KIND-028', 'an unmatch supersedes the match and frees its lines; both records stay', [
        commit(lines2), run(label='m'),
        op('unmatch', branch='main', match_set='bank-1010', entry_id={'$ref': 'm/matches/0/entry_id'},
           reason='wrong invoice'),
        run(commit_=False), records()], classes=MATCH_CLASSES)
    V('STD-KIND-028', 'a match is undone once', [
        commit(lines2), run(label='m'),
        op('unmatch', branch='main', match_set='bank-1010', entry_id={'$ref': 'm/matches/0/entry_id'}),
        op('unmatch', branch='main', match_set='bank-1010', entry_id={'$ref': 'm/matches/0/entry_id'},
           error='MODEL.CONSTRAINT_VIOLATED')], classes=MATCH_CLASSES)
    V('STD-KIND-028', 'an unmatch names a match record of the set', [
        commit(lines2), op('unmatch', branch='main', match_set='bank-1010', entry_id=uid(99), error='IAM.NOT_FOUND')],
      classes=MATCH_CLASSES)
    V('STD-KIND-028', 'match records are not appended by a commit', [
        commit([{'op': 'append', 'class': 'MatchRecord', 'entry_id': uid(30), 'ledger_key': {'match_set': 's'},
                 'body': {'match_set': 's', 'left': [], 'right': [], 'actor': 'x'}}], error='LOGIC.FORBIDDEN_TARGET')],
      classes=MATCH_CLASSES)
    V('STD-KIND-028', 'a match record names the rule version and the actor', [
        commit(lines2), op('match', branch='main', match_set='bank-1010', left=side('BookLine'), right=side('BankLine'),
                           rules=[rule('exact')], commit=True,
                           process={'initiator': 'did:ubos:acme:01900000-0000-7000-8000-000000000009'}),
        records()], classes=MATCH_CLASSES)

    # ---- STD-TIME-041 re-evaluation modes, STD-TIME-042 restatement ----
    restatement(g)


RATE = cls('Rate', [field('value', 'decimal', True)])
FEE = cls('Fee', [field('rate', 'decimal', True), field('method', 'text', True)])
RESTATE_CLASSES = CLASSES + [RATE, FEE]


def calc(method):
    return {'name': 'acme.fee', 'tier': 'host', 'abi': '1.0', 'profile': 'standard', 'state': 'committed',
            'capabilities': ['read', 'commit'], 'exports': ['compute'],
            'source': {'functions': {'compute': {'params': ['d'], 'body': [
                {'do': 'read.get', 'args': {'target': '"ubos://acme/Rate/r-1"'}, 'as': 'r'},
                {'do': 'read.get', 'args': {'target': '"ubos://acme/Fee/f-1"'}},
                {'do': 'commit.update', 'args': {'target': '"ubos://acme/Fee/f-1"',
                                                 'set': {'rate': 'r.fields.value', 'method': '"%s"' % method},
                                                 'timeline': {'op': '"supersede_from"', 'from': 'params.d'}}},
                {'return': '1'}]}}}}


def restatement(g):
    def V(clause, desc, steps, also=()):
        g.vector(clause, desc, setup=setup('2026-01-05T09:00:00.000000Z', RESTATE_CLASSES), steps=steps,
                 prototype=False, also=also)

    deploy = [
        commit([create(uid(1), 'Rate', 'r-1', {'value': '0.10'}, valid={'from': '2026-01-01'}),
                create(uid(2), 'Fee', 'f-1', {'rate': '0', 'method': 'none'}, valid={'from': '2026-01-01'}),
                create(uid(80), 'Logic', 'acme-fee', calc('v1'))], label='d'),
        op('verify_asset', branch='main', object_id=uid(80)),
        now('2026-02-16T09:00:00.000000Z'),
        op('execute', branch='main', asset='acme.fee', function='compute', args=['2026-02-15'],
           ctx={'asof': '2026-02-15'}, process={'process_id': uid(501)}, entropy_seed='11' * 32),
        now('2026-03-01T09:00:00.000000Z'),
        op('read', branch='main', object_id=uid(80), label='l'),
        commit([{'op': 'update', 'object_id': uid(80), 'base': {'$ref': 'l/version_hash'},
                 'body': {'source': calc('v2')['source']}}]),
        op('verify_asset', branch='main', object_id=uid(80)),
        now('2026-03-05T09:00:00.000000Z'),
        op('read', branch='main', object_id=uid(1), label='r'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'r/version_hash'}, 'body': {'value': '0.12'},
                 'valid': {'kind': 'supersede_from', 'from': '2026-01-01'}}], label='fix')]

    def restate(name='correction/rate-fix', error=None, label=None, **kw):
        return op('restate', label=label, error=error, branch='main', time={'$ref': 'fix/cseq'}, name=name, **kw)

    V('STD-TIME-042', 'a restatement re-runs the producing process on a correction branch; the source is unchanged',
      deploy + [restate(), op('read', branch='correction/rate-fix', object_id=uid(2), asof='2026-02-20'),
                op('read', branch='main', object_id=uid(2), asof='2026-02-20')])
    V('STD-TIME-042', 'the restated result keeps its valid time', deploy + [
        restate(), op('read_range', branch='correction/rate-fix', object_id=uid(2), **{'from': '2026-01-01'})])
    V('STD-TIME-042', 'a materiality expression flags each change', deploy + [
        restate(materiality='this.rate - old.rate > 0.01')])
    V('STD-TIME-042', 'a restatement runs on a branch of kind correction', deploy + [
        restate(name='feature/rate-fix', error='MODEL.INVALID_VALUE')])
    V('STD-TIME-041', 'as_effective is the default: February data follows the February logic', deploy + [
        restate(), op('read', branch='correction/rate-fix', object_id=uid(2), asof='2026-02-20')])
    V('STD-TIME-041', 'as_originally runs the logic version of the original process', deploy + [
        restate(mode='as_originally'), op('read', branch='correction/rate-fix', object_id=uid(2), asof='2026-02-20')])
    V('STD-TIME-041', 'current runs the head version of the logic', deploy + [
        restate(mode='current'), op('read', branch='correction/rate-fix', object_id=uid(2), asof='2026-02-20')])
    V('STD-TIME-041', 'an unknown mode fails', deploy + [restate(mode='latest', error='MODEL.INVALID_VALUE')])
