all:
	arm-none-eabi-gcc -mcpu=cortex-m3 -mthumb -c -o startup.o startup.s
	cargo build

clean:
	cargo clean
	rm -f *.o

