OUTPUT_ARCH(riscv)
ENTRY(_start)

OFFSET = 0xffffffc000000000;
SECTIONS
{
    . = 0x80200000;
    .boot : { KEEP(*(.text.boot)) }
    . = ALIGN(4096);
    .boot.bss (NOLOAD) : { *(.boot.bss) }
    __root_high = boot_root + OFFSET;

    . = ALIGN(4096) + OFFSET;
    __kernel_start = .;
    .text : AT(ADDR(.text) - OFFSET) {
        *(.text.entry) *(.text .text.*)
    }
    . = ALIGN(4096);
    .user : AT(ADDR(.user) - OFFSET) { KEEP(*(.user.text)) }
    ASSERT(SIZEOF(.user) <= 4096, "user program exceeds one page")
    . = ALIGN(4096);
    .user_exec : AT(ADDR(.user_exec) - OFFSET) { KEEP(*(.user.exec)) }
    ASSERT(SIZEOF(.user_exec) <= 4096, "execution target exceeds one page")
    . = ALIGN(4096);
    .rodata : AT(ADDR(.rodata) - OFFSET) { *(.rodata .rodata.*) }
    . = ALIGN(4096);
    .data : AT(ADDR(.data) - OFFSET) {
        *(.data .data.*) *(.sdata .sdata.*)
    }
    . = ALIGN(4096);
    __bss_start = .;
    .bss (NOLOAD) : AT(ADDR(.bss) - OFFSET) {
        *(.bss.stack) *(.bss .bss.*) *(.sbss .sbss.*) *(COMMON)
    }
    . = ALIGN(4096);
    __bss_end = .;
    ASSERT(. - OFFSET < 0x88000000, "kernel exceeds guest RAM")
    /DISCARD/ : { *(.eh_frame*) }
}
