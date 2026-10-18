from src.registry import lookup


def describe(name: str) -> str:
    return lookup(name) or "unknown"
