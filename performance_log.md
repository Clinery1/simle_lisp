# Constant stats
Max call stack depth: 3
Instruction count: 122_692_035
I am running ***ONE*** test for this, so it is NOT scientific enough to be called good
benchmarking. However, the performance gaps are large enough to not be just noise unless noted.
I am running this on my FW16 with the Ryzen 7 7840HS on the standard 180W charger. Idle CPU%
is around 1%.

I am running `cargo flamegraph -F 1000 -- -s run example.slp` for most of the tests. The first
couple don't have the `-F 1000` arg, but the others do.


# Baseline
The state of the interpreter in the master branch, with fixes to `src/interpreter/ast.rs` to make it
actually work.

Allocations: 40040014. This stays constant for the next few optimizations.

Runtime: 5.219884249s

23.50M ins/s


# Changing the `collect` function to use `retain`
This got us a 17.36% speedup

Runtime: 4.313923817s

28.44M ins/s


# Changing from `fnv` to `rustc-hash`
This got us ANOTHER large speedup of 14.5%

Runtime: 3.688427844s

33.26M ins/s


# Caching old `Env`s
This gave us a 1.23% improvement, but it fluctuated, so I would say it didn't do much for this
example. Probably because I implemented tail calls and don't create many `Env`s

Runtime: 3.642979553s

33.68M ins/s


# Fixing a bug: I forgot to actually drop the `DataBox`
It seems by fixing a bug I made it MUCH faster.
I guess it would leak some memory since we are using closures. It was probably just increasing
memory infinitely and taking forever by calling the kernel for more memory, but I really don't know.

This is a whopping 23.59% increase! I cannot believe its that much better!

Runtime: 2.783627386s

44.08M ins/s


# Remove an allocation from `set_func_args`
We have a modest 4.11% speedup this time.
So far we have a 48.87% speedup from baseline. That is almost 2x just by doing simple things! It
really helps to have a graph showing what is slow.

Allocation count is now down to 33760011.

Runtime: 2.669001678s

45.97M ins/s


# (INVALID) If I change the test to run the collector after every `fizzBuzz`
This yields a 7.52% increase. Quite a bit, but not much. Almost contradictory to run the collector
more often, but it seems the time taken to allocate costs more as the heap grows in size.

Runtime: 2.468174569s

49.48M ins/s
