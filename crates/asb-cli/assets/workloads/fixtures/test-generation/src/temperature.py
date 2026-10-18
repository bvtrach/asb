def classify(celsius: float) -> str:
    if celsius < 0:
        return "freezing"
    if celsius > 30:
        return "hot"
    return "mild"
