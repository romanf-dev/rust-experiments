.syntax unified
.cpu cortex-m3
.thumb

.extern _start
.extern _interrupt
.extern _exception

.section .vectors, "ax"
.code 16
.align 0
.global vectors

vectors:
.word     0x20005000                  // Top of stack
.word     RESET_Handler       	      // Reset Handler
.word     _exception                  // NMI
.word     _exception                  // HardFault
.word     _exception                  // MPU Fault Handler
.word     _exception                  // Bus Fault Handler
.word     _exception                  // Usage Fault Handler
.word     0                           // Reserved
.word     0                           // Reserved
.word     0                           // Reserved
.word     0                           // Reserved
.word     _exception                  // SVCall Handler
.word     _exception                  // Debug Monitor Handler
.word     0                           // Reserved
.word     _interrupt                  // PendSV Handler
.word     _interrupt                  // SysTick Handler
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt
.word     _interrupt

.section .text
.global RESET_Handler
.thumb_func
RESET_Handler:

init_bss_start:
  ldr   r0, =0
  ldr   r1, =_bss
  ldr   r2, =_bss_sz 
  cmp   r2, $0
  beq   image_init_done
init_bss:
  strb  r0, [r1]
  adds  r1, r1, $1
  subs  r2, r2, $1
  bne   init_bss
        
image_init_done:
  b _start
