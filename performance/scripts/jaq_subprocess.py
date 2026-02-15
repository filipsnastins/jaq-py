#!/usr/bin/env python3
"""Run jaq filter via subprocess.

Usage: jaq_subprocess.py <data_file> <filter> <iterations>
"""

import subprocess  # noqa: S404
import sys
from pathlib import Path

data_file = Path(sys.argv[1])
filter_expr = sys.argv[2]
iterations = int(sys.argv[3]) if len(sys.argv) > 3 else 1  # noqa: PLR2004

data = data_file.read_text(encoding="utf-8")

for _ in range(iterations):
    __ = subprocess.run(  # noqa: S603
        [Path.home() / ".cargo/bin/jaq", filter_expr],
        input=data,
        capture_output=True,
        text=True,
        check=True,
    )
