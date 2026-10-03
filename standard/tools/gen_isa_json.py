#!/usr/bin/env python3
"""Write the instruction catalogue of ABI 1.0 (STD-ISA-*), the error registry (STD-ERR-002)
and the profile table (STD-CTX-004), as the oracle reads them from the clause text, to
`crates/ubos-runtime/src/isa.json` (a generated file: never edited by hand)."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from bpa import isa  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'crates', 'ubos-runtime', 'src',
                   'isa.json')


def main():
    data = {'abi': isa.ABI, 'instructions': isa.instructions(), 'catchable': isa.catchable(),
            'profile_limits': isa.PROFILE_LIMITS,
            'profile_families': {k: list(v) for k, v in isa.PROFILE_FAMILIES.items()}}
    text = json.dumps(data, indent=1, sort_keys=True, ensure_ascii=False) + '\n'
    old = open(OUT, encoding='utf-8').read() if os.path.exists(OUT) else None
    if old != text:
        open(OUT, 'w', encoding='utf-8').write(text)
        print('isa.json: written')
    else:
        print('isa.json: unchanged')


if __name__ == '__main__':
    main()
