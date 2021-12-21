# `manytasks`

A tool to create a lot of (Linux) tasks.

## Building

Container:
```
docker build -t manytasks .
```

Locally, with `cargo`:
```
cargo build
```

## Running


Container:
```
docker run -it manytasks <taskcount>
```

Locally, with `cargo`:
```
cargo run <taskcount>
```

From the built code:
```
manytasks <taskcount>
```

Using `taskset` to limit system impact (only use half the CPUs):
```
taskset --cpu-list "0-$( nproc --ignore=1 ):2" cargo run 100000
```

## Diagnostics

If you see an error similar to:

`Error making task: Resource temporarily unavailable (os error 11)`

then the tool automatically retries this, printing the diagnostic for information.

Other errors are written to STDERR and should be reasonably self-explanatory.
