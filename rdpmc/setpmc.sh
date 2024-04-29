#!/bin/bash

echo 2 | tee /sys/devices/cpu/rdpmc

wrmsr 0xc0010202 0x430341
wrmsr 0xc0010203 0
wrmsr 0xc0010204 0x430329
wrmsr 0xc0010205 0

