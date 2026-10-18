# Bug-fix fixture

Run `python3 -m unittest discover -s tests` before changing the source. The
parser currently mishandles a trailing separator; preserve the existing API
while fixing the smallest reproducible defect.
