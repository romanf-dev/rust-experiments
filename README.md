Rust experiments
================

linked_list
-----------

Yet another attempt to implement C-style intrusive doubly-linked queue using raw pointers. It relies on aliasing, so perhaps it is not so good idea to do it in Rust. Nevertheless, it passes Miri and I hope it is a good start.

pool
----

Simple alloc-only allocator that allocates blocks from preallocated array. While it returns mutable references to items of the array it does not create more than one reference to the array itself. It works for both static and local data and also passes Miri.

mcu_bare_metal
--------------

Minimal Rust application for STM32 MCU. It doesn't use any external crates.

