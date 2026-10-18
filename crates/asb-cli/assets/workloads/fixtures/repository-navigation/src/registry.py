ENTRIES = {"alpha": "first", "beta": "second"}


def lookup(name: str) -> str | None:
    return ENTRIES.get(name)
