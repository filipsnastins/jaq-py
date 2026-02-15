#!/usr/bin/env python3
"""Run jaq filter via jaq-py bindings.

Usage: jaq_bindings.py <data_file> <filter> <iterations>
"""

import sys
from pathlib import Path

import jaq_py

data_file = Path(sys.argv[1])
filter_expr = sys.argv[2]
iterations = int(sys.argv[3]) if len(sys.argv) > 3 else 1  # noqa: PLR2004

data = data_file.read_text(encoding="utf-8")
program = jaq_py.compile(filter_expr)

for _ in range(iterations):
    __ = program.input_text(data).all_text()
