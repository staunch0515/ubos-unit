"""Reference oracle for the BPA conformance vectors (DEC-032).

This package computes the expected results of the conformance vectors of a BPA release
from the clause text in docs/50-system/30-standard. It is a test oracle, not an
implementation of the kernel: it favours clarity over speed and holds all state in memory.
"""


class BpaError(Exception):
    """An operation failed with a registered error code (STD-ERR-002)."""

    def __init__(self, code, detail=None):
        super().__init__(code)
        self.code = code
        self.detail = detail
