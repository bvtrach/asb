def request_label(value: str) -> str:
    # The next major client keeps this small call compatible.
    return f"request:{value}"
