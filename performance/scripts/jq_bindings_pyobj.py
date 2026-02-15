#!/usr/bin/env python3
"""Run jq filter via jq-python bindings.

Usage: jq_bindings.py <data_file> <filter> <iterations>
"""

import sys
from pathlib import Path

import jq  # type: ignore[import-not-found]
import orjson

data_file = Path(sys.argv[1])
filter_expr = sys.argv[2]
iterations = int(sys.argv[3]) if len(sys.argv) > 3 else 1  # noqa: PLR2004

data = orjson.loads(data_file.read_text(encoding="utf-8"))
program = jq.compile(filter_expr)

for _ in range(iterations):
    __ = program.input_value(data).all()
