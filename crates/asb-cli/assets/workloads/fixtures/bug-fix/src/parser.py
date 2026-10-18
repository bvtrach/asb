def parse_path(value: str) -> list[str]:
    """Split a slash-separated path into non-empty segments."""
    return value.split("/")
