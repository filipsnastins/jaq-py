# Performance Benchmarks

Compares jq CLI, jaq CLI, Python jq bindings, and jaq-py.

Run with `make bench` from the root of the repository.

```console
❯ make bench
Building jaq-py in release mode...

=== Large File (53 MB, 1 iteration) ===
Subprocess wins: no data conversion overhead

Data: data/large.json ( 53M)  |  Iterations: 1  |  Runs: 3

CLI:
  jq                       1.84s     243 MB
  jaq                       .97s     350 MB

Python + subprocess:
  jq + subprocess          2.12s     465 MB
  jaq + subprocess         1.35s     465 MB

Python bindings:
  jq-python                2.77s     776 MB
  jaq-py                   2.00s     944 MB

=== Many Invocations (1000x) ===
Bindings win: no fork/exec overhead per call

Data: data/small.json (4.0K)  |  Iterations: 1000  |  Runs: 3

CLI:
  jq                          0s       2 MB
  jaq                         0s       7 MB

Python + subprocess:
  jq + subprocess          4.76s      26 MB
  jaq + subprocess        10.07s      26 MB

Python bindings:
  jq-python                 .08s      26 MB
  jaq-py                    .06s      26 MB
```
