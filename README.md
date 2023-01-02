Rust experiments
================

linked_list
-----------

Attempt to implement C-style intrusive doubly-linked list using raw pointers.
Node object is intended to be embedded into some other (embracing) object.
Unlike C, prev/next pointers may be completely hidden by using closures.
Also, this implementation has no special cases like 'empty list', so both insertion and deletion procedures don't use 'if's, simple and clean.

mcu_bare_metal
--------------

Minimal Rust application for STM32 MCU. It doesn't use any external crates.

