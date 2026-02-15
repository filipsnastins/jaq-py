# jaq-py

Python bindings for [jaq](https://github.com/01mf02/jaq), a jq clone written in Rust.

> [!NOTE]
> This project was created as a learning exercise for building Python bindings with Rust and PyO3.
> It is not feature-complete and has not been tested in production.

## Usage

```python
import jaq_py

# Compile a filter and run it
prog = jaq_py.compile(".name")
result = prog.input_value({"name": "Alice"}).first()  # "Alice"

# Get all results
jaq_py.compile(".[] | . * 2").input_value([1, 2, 3]).all()  # [2, 4, 6]

# Use variables
prog = jaq_py.compile("$a + $b + .", args={"a": 100, "b": 20})
prog.input_value(3).first()  # 123

# Fast path with JSON string input
prog = jaq_py.compile(".name")
prog.input_text('{"name": "Bob"}').first()  # "Bob"

# Slurp multiple JSON values into an array
jaq_py.compile(".").input_text("1\n2\n3", slurp=True).first()  # [1, 2, 3]

# Get result as JSON string
jaq_py.compile(".").input_value({"a": 1}).first_text()  # '{"a":1}'
```

## Performance

See [performance/README.md](performance/README.md) for details.

## Development

Requires Rust and Python 3.13+.

```bash
make install    # Install dependencies
make            # Format, build, lint, and test
make bench      # Run performance benchmarks
```

## Acknowledgements

- [jaq](https://github.com/01mf02/jaq) - the underlying jq clone written in Rust
- [jq.py](https://github.com/mwilliamson/jq.py) - inspiration for the Python API design

## TODO

- [ ] GIL release - experiment with free-threaded mode in Python 3.13
- [ ] Implement `__iter__` interface for `JaqProgramWithInput`.
- [ ] Align Python API with Rust `jaq_all` crate.
- [ ] Publish wheels for all platforms to PyPI.
