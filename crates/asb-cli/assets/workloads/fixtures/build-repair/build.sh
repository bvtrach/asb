#!/bin/sh
set -eu
python3 -m compileall -q src tests
python3 -m unittest discover -s test
python3 src/app.py
