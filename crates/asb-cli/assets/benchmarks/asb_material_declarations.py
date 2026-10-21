"""Bounded private pre-creation receipts, separate from recovery projections."""
import copy
import json
import os

from asb_material import MaterialError


def read(journal, attribute, filename):
    """Require the exact cached owner declaration and its durable private copy."""
    from asb_resources import private_file, RecoveryError
    if getattr(journal,"lock",None) is None:
        raise MaterialError("prepared material creation declaration lost its active owner")
    cached = getattr(journal,attribute,None)
    if not isinstance(cached,dict) or cached.get("available") is not True:
        raise MaterialError("prepared material inputs lack an owned creation declaration")
    try:
        descriptor = private_file(journal.directory,filename,os.O_RDONLY)
        with os.fdopen(descriptor,"rb") as source:
            content = source.read(1024**2+1)
        if (len(content) > 1024**2 or json.loads(content) != cached
                or cached.get("schema_version") != 1
                or any(cached[key] != journal.record[key] for key in ["attempt","daemon","token"])):
            raise MaterialError("durable prepared material creation declaration changed")
    except (RecoveryError,OSError,ValueError,UnicodeError,KeyError,TypeError):
        raise MaterialError("durable prepared material creation declaration is unavailable or invalid") from None
    return copy.deepcopy(cached)
