# crossbeam::queue::ring_buffer proposal

This is my personal view of what could be a flexible interface to satisfy different use cases,
 as well as providing multiple layers of complexity, from simple pop’s to low level slices.
But it is also a way to check if I have understood [the original proposal](https://paper.dropbox.com/doc/crossbeamqueuering_buffer--AeXY~PGl~eryC~ABuzrC~VEpAg-ubfhM7exqCZgymd9ieSGg),
 by explaining it with my own words here.
About the terms used in the code and comments (region, sync, elements, snapshot, …),
 I hope they are clear enough to describe the code and ideas, but feel free to replace with anything that suits better.

The proposed interfaces, as well as some tests that represent examples of usage [can be found here](src/lib.rs).

Note that [the implementation](src/ring_buffer.rs) is `super-dummy-single-thread-not-useful-at-all`,
 with the only purpose to help evaluating and tuning the user interfaces. Do not use it anywhere :-P

Note also that this work makes [my previous proposal document](https://paper.dropbox.com/doc/crossbeamqueuering_buffer-Christians-ideas--AeWY7WALVq86Y6qL_qUa6DrMAg-JZaVVMlPlmw2W7xqqy7py) obsolete.

## Rationale

There are different use cases and types of users:

### Sequential access with one sync per access:

The first surface of the API should satisfy the most trivial use case without having to learn many things,
 which is reading sequentially, and satisfied by the `pop` method in the `Consumer` structure.
In that case the user is not that concerned about the performance penalty of having to sync
 the `Consumer` cursor on every access, and they don’t need to care about what a region or a pair of slices are.

### Sequential access for a batch of elements with only one sync at the end:

The `region` method provides an advanced interface that allows other kind of use cases.
 In this case, it provides sequential read through either calling the `pop` method or by using the region as an `Iterator`.
 The synchronisation of the `Consumer` cursor is delayed to the end, when either the region is dropped
 or the iterator fully consumed (for example with a `collect`).
The `region` method borrows the `Consumer` mutably which prevents accesses to it while the region structure is in use.
The user cares somehow about performance, so delaying the syncs to the end matters,
 but don’t want to repeat the same code to create iterators and chain them every time they need
 to iterate the full range of elements.

### Low level access to the underlying elements

Given that this is a ring buffer, it might happen that at some point the elements ready for consumption
 goes beyond the end of the internal buffer, and then continues by the beginning of it.
 In that case it is not possible to provide a single slice to the data. This is why a region provides
 the method `as_slices` that return a pair of slices. When the view doesn’t cross the end of the internal buffer,
 then the first slice contains all the data available, and the second one is empty, but when it does cross it,
 then the first slice contains the elements until the end, and the second slice the elements that continue
 from the beginning of the internal buffer.
This interface allows either sequential access, random access and peek semantics.
The user of this interface cares completely about every performance detail and knows very well how to read
 the pair of slices without requiring much logic for that. This user is in full charge of using the `advance`
 method to update the region cursor, and knows very well what happens if it is not invoked and the region is
 dropped (which is that the cursor won’t advance and the ring buffer will saturate at some point).

