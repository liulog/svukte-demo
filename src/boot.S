.option norvc
.option norelax
.section .text.boot,"ax"
.global _start
_start:
    # OpenSBI enters here in S-mode, satp=Bare; no stack is needed yet.
    csrw sie, zero
    csrw sscratch, zero
    la t0, boot_root
    mv t1, t0
    li t2, 512
1:  sd zero, 0(t1)
    addi t1, t1, 8
    addi t2, t2, -1
    bnez t2, 1b
    # Two 1 GiB supervisor leaves: identity and higher-half direct map.
    li t1, 0x200000cf       # PA=0x80000000, V|R|W|X|A|D, U=0
    sd t1, 16(t0)           # VPN[2]=2
    li t2, 2064
    add t2, t0, t2
    sd t1, 0(t2)            # VPN[2]=258 (offset + 0x80000000)
    srli t0, t0, 12
    li t1, 0x8000000000000000
    or t0, t0, t1
    # Load an absolute high VA through a nearby literal before enabling VM.
    la t1, .Lhigh_entry
    ld t1, 0(t1)
    sfence.vma
    csrw satp, t0
    sfence.vma
    jr t1
.balign 8
.Lhigh_entry: .dword high_entry

.section .boot.bss,"aw",@nobits
.balign 4096
.global boot_root
boot_root: .skip 4096

.section .text.entry,"ax"
high_entry:
    la t0, __bss_start
    la t1, __bss_end
1:  bgeu t0, t1, 2f
    sd zero, 0(t0)
    addi t0, t0, 8
    j 1b
2:  la sp, kernel_stack_top
    la t0, trap_entry
    csrw stvec, t0
    call kernel_main
3:  wfi
    j 3b

.section .bss.stack,"aw",@nobits
.balign 16
kernel_stack: .skip 32768
kernel_stack_top:

.section .text,"ax"
.global enter_user
enter_user:
    # a0=high page, a1=low alias, a2=UKTE, a3=entry, a4=user sp
    csrw sscratch, sp
    csrw sepc, a3
    li t0, 0x40102          # SUM | SPP | SIE (interrupts stay disabled)
    csrc sstatus, t0
    mv sp, a4
    sret

.balign 4
.global trap_entry
trap_entry:
    csrrw sp, sscratch, sp
    bnez sp, 1f
    # sscratch=0 identifies an unexpected S-mode exception.
    csrrw sp, sscratch, sp
1:  addi sp, sp, -256
    sd ra, 8(sp)
    sd gp, 24(sp)
    sd tp, 32(sp)
    sd t0, 40(sp)
    sd t1, 48(sp)
    sd t2, 56(sp)
    sd s0, 64(sp)
    sd s1, 72(sp)
    sd a0, 80(sp)
    sd a1, 88(sp)
    sd a2, 96(sp)
    sd a3, 104(sp)
    sd a4, 112(sp)
    sd a5, 120(sp)
    sd a6, 128(sp)
    sd a7, 136(sp)
    sd s2, 144(sp)
    sd s3, 152(sp)
    sd s4, 160(sp)
    sd s5, 168(sp)
    sd s6, 176(sp)
    sd s7, 184(sp)
    sd s8, 192(sp)
    sd s9, 200(sp)
    sd s10, 208(sp)
    sd s11, 216(sp)
    sd t3, 224(sp)
    sd t4, 232(sp)
    sd t5, 240(sp)
    sd t6, 248(sp)
    csrr t0, sscratch
    sd t0, 16(sp)
    csrw sscratch, zero
    mv a0, sp
    call handle_trap
    addi t0, sp, 256
    csrw sscratch, t0
    ld ra, 8(sp)
    ld gp, 24(sp)
    ld tp, 32(sp)
    ld t0, 40(sp)
    ld t1, 48(sp)
    ld t2, 56(sp)
    ld s0, 64(sp)
    ld s1, 72(sp)
    ld a0, 80(sp)
    ld a1, 88(sp)
    ld a2, 96(sp)
    ld a3, 104(sp)
    ld a4, 112(sp)
    ld a5, 120(sp)
    ld a6, 128(sp)
    ld a7, 136(sp)
    ld s2, 144(sp)
    ld s3, 152(sp)
    ld s4, 160(sp)
    ld s5, 168(sp)
    ld s6, 176(sp)
    ld s7, 184(sp)
    ld s8, 192(sp)
    ld s9, 200(sp)
    ld s10, 208(sp)
    ld s11, 216(sp)
    ld t3, 224(sp)
    ld t4, 232(sp)
    ld t5, 240(sp)
    ld t6, 248(sp)
    ld sp, 16(sp)
    sret

# This page contains no absolute code/data references, so it also runs at
# USER_CODE. All instructions are 4 bytes, including the faulting ld/sd.
.section .user.text,"ax"
.balign 4096
.global user_start, user_load, user_store, user_after_exec
user_start:
    li t3, 0x1122334455667788
    li t4, 0x8877665544332211
    ld t0, 0(a1)
    bne t0, t3, .Luser_fail
    sd t4, 0(a1)
    ld t0, 0(a1)
    bne t0, t4, .Luser_fail
    li t0, 0
user_load:
    ld t0, 0(a0)
    bnez a2, .Lprotected_load
    bne t0, t4, .Luser_fail
    j user_store
.Lprotected_load:
    bnez t0, .Luser_fail  # a denied load must leave its destination intact
user_store:
    sd t3, 0(a0)
    ld t0, 0(a1)
    bnez a2, .Lprotected_store
    bne t0, t3, .Luser_fail
    j .Luser_exec
.Lprotected_store:
    bne t0, t4, .Luser_fail  # denied store must not change backing memory
.Luser_exec:
    # Execute the same X-only physical code page through its low alias first.
    li a5, 0
    li t0, 0x15000
    jalr ra, 0(t0)
    li t1, 0x5a5
    bne a5, t1, .Luser_fail
    # Then call the high alias. With UKTE=1, the first fetch must fault.
    li a5, 0
    li t0, 0xffffffd000001000
    jalr ra, 0(t0)
user_after_exec:
    bnez a2, .Lprotected_exec
    bne a5, t1, .Luser_fail
    j .Luser_pass
.Lprotected_exec:
    bnez a5, .Luser_fail  # the high target must not have executed
.Luser_pass:
    li a7, 1
    ecall
.Luser_fail:
    li a7, 2
    ecall
1:  j 1b

# A separate page mapped U=1, X=1, R=W=0 at both high and low user VAs.
.section .user.exec,"ax"
.balign 4096
.global user_exec_target
user_exec_target:
    li a5, 0x5a5
    ret
