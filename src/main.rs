#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

global_asm!(include_str!("boot.S"));

const OFFSET: usize = 0xffff_ffc0_0000_0000;
const USER_CODE: usize = 0x10000;
const USER_ALIAS: usize = 0x12000;
const USER_STACK: usize = 0x13000;
const USER_EXEC_ALIAS: usize = 0x15000;
const HIGH_PAGE: usize = 0xffff_ffd0_0000_0000;
const HIGH_EXEC: usize = HIGH_PAGE + 4096;
const INITIAL: u64 = 0x1122_3344_5566_7788;
const WRITTEN: u64 = 0x8877_6655_4433_2211;
const UKTE: usize = 1 << 8;
const V: usize = 1;
const R: usize = 2;
const W: usize = 4;
const X: usize = 8;
const U: usize = 16;
const A: usize = 64;
const D: usize = 128;

#[repr(C, align(4096))]
struct Page([usize; 512]);
static mut LOW_L1: Page = Page([0; 512]);
static mut LOW_L0: Page = Page([0; 512]);
static mut HIGH_L1: Page = Page([0; 512]);
static mut HIGH_L0: Page = Page([0; 512]);
static mut DATA: Page = Page([0; 512]);
static mut STACK: Page = Page([0; 512]);
static mut ROUND: usize = 0;
static mut FAULTS: usize = 0;

extern "C" {
    static mut __root_high: Page;
    static user_start: u8;
    static user_load: u8;
    static user_store: u8;
    static user_after_exec: u8;
    static user_exec_target: u8;
    fn enter_user(high: usize, low: usize, ukte: usize, pc: usize, sp: usize) -> !;
}

// Legacy SBI console is supplied by QEMU's bundled OpenSBI; no MMIO mapping.
fn put(s: &str) {
    for byte in s.bytes() {
        unsafe {
            asm!("ecall", inlateout("a0") byte as usize => _,
                 inlateout("a1") 0usize => _, in("a7") 1usize);
        }
    }
}

fn hex(value: usize) {
    put("0x");
    for shift in (0..16).rev() {
        let digit = b"0123456789abcdef"[(value >> (shift * 4)) & 15];
        // Each digit is ASCII, thus always valid UTF-8.
        put(unsafe { core::str::from_utf8_unchecked(core::slice::from_ref(&digit)) });
    }
}

fn finish(success: bool) -> ! {
    unsafe {
        // SBI SRST: shutdown, reason=none/system failure.
        asm!("ecall", inlateout("a0") 0usize => _,
             inlateout("a1") usize::from(!success) => _,
             in("a6") 0usize, in("a7") 0x53525354usize);
    }
    loop {
        unsafe { asm!("wfi") }
    }
}

fn fail(message: &str) -> ! {
    put("[FAIL] ");
    put(message);
    put("\n");
    finish(false)
}

fn require(ok: bool, message: &str) {
    if !ok {
        fail(message);
    }
}

fn physical(va: usize) -> usize {
    va.wrapping_sub(OFFSET)
}

fn pte(pa: usize, flags: usize) -> usize {
    ((pa >> 12) << 10) | flags
}

unsafe fn entry(table: *mut Page, index: usize, value: usize) {
    write_volatile(table.cast::<usize>().add(index), value);
}

unsafe fn setup_pages() {
    let root = addr_of_mut!(__root_high);
    entry(root, 0, pte(physical(addr_of!(LOW_L1) as usize), V));
    entry(
        addr_of_mut!(LOW_L1),
        0,
        pte(physical(addr_of!(LOW_L0) as usize), V),
    );
    entry(
        addr_of_mut!(LOW_L0),
        USER_CODE >> 12,
        pte(physical(addr_of!(user_start) as usize), V | R | X | U | A),
    );
    entry(
        addr_of_mut!(LOW_L0),
        USER_ALIAS >> 12,
        pte(physical(addr_of!(DATA) as usize), V | R | W | U | A | D),
    );
    entry(
        addr_of_mut!(LOW_L0),
        USER_STACK >> 12,
        pte(physical(addr_of!(STACK) as usize), V | R | W | U | A | D),
    );
    entry(
        addr_of_mut!(LOW_L0),
        USER_EXEC_ALIAS >> 12,
        pte(physical(addr_of!(user_exec_target) as usize), V | X | U | A),
    );
    entry(
        root,
        (HIGH_PAGE >> 30) & 511,
        pte(physical(addr_of!(HIGH_L1) as usize), V),
    );
    entry(
        addr_of_mut!(HIGH_L1),
        (HIGH_PAGE >> 21) & 511,
        pte(physical(addr_of!(HIGH_L0) as usize), V),
    );
    entry(
        addr_of_mut!(HIGH_L0),
        (HIGH_PAGE >> 12) & 511,
        pte(physical(addr_of!(DATA) as usize), V | R | W | U | A | D),
    );
    entry(
        addr_of_mut!(HIGH_L0),
        (HIGH_EXEC >> 12) & 511,
        pte(physical(addr_of!(user_exec_target) as usize), V | X | U | A),
    );
    entry(root, 2, 0); // Remove the boot identity mapping entirely.
    asm!("sfence.vma zero, zero");
}

unsafe fn set_ukte(enabled: bool) {
    if enabled {
        asm!("csrs 0x10a, {bit}", bit = in(reg) UKTE);
    } else {
        asm!("csrc 0x10a, {bit}", bit = in(reg) UKTE);
    }
    let value: usize;
    asm!("csrr {value}, 0x10a", value = out(reg) value);
    if enabled && value & UKTE == 0 {
        fail("UKTE unavailable: senvcfg.UKTE readback is zero");
    }
    require((value & UKTE != 0) == enabled, "UKTE readback mismatch");
    // Changing UKTE must not reuse U-mode translations cached in an earlier round.
    asm!("sfence.vma zero, zero");
}

unsafe fn data() -> u64 {
    read_volatile(addr_of!(DATA).cast::<u64>())
}

unsafe fn prepare_round() {
    write_volatile(addr_of_mut!(DATA).cast::<u64>(), INITIAL);
    FAULTS = 0;
    set_ukte(ROUND == 1);
    put(match ROUND {
        0 => "[ROUND] UKTE=0 baseline\n",
        1 => "[ROUND] UKTE=1 protected\n",
        _ => "[ROUND] UKTE=0 restored\n",
    });
    // S-mode can still read/write the same U=1 high mapping with SUM=1.
    asm!("csrs sstatus, {sum}", sum = in(reg) 1usize << 18);
    require(
        read_volatile(HIGH_PAGE as *const u64) == INITIAL,
        "S-mode high read",
    );
    write_volatile(HIGH_PAGE as *mut u64, WRITTEN);
    require(data() == WRITTEN, "S-mode high write");
    write_volatile(HIGH_PAGE as *mut u64, INITIAL);
    asm!("csrc sstatus, {sum}", sum = in(reg) 1usize << 18);
    put("[OK] S-mode high U-page read/write with SUM=1\n");
}

#[no_mangle]
unsafe extern "C" fn kernel_main() -> ! {
    put("\nSvukte MVP: S-mode / Sv39 / higher-half kernel\n");
    let pc: usize;
    let satp: usize;
    asm!("auipc {pc}, 0", pc = out(reg) pc);
    asm!("csrr {satp}, satp", satp = out(reg) satp);
    put("kernel pc=");
    hex(pc);
    put(" satp=");
    hex(satp);
    put("\n");
    require(pc >= OFFSET && satp >> 60 == 8, "higher-half Sv39 required");
    setup_pages();
    put("[OK] Sv39 high kernel; boot identity mapping removed\n");
    put("high U=1 RW page=");
    hex(HIGH_PAGE);
    put(" low alias=");
    hex(USER_ALIAS);
    put("\n");
    put("high U=1 X-only page=");
    hex(HIGH_EXEC);
    put(" low execution alias=");
    hex(USER_EXEC_ALIAS);
    put("\n");
    set_ukte(false);
    prepare_round();
    enter_user(HIGH_PAGE, USER_ALIAS, 0, USER_CODE, USER_STACK + 4096)
}

fn user_pc(symbol: *const u8) -> usize {
    USER_CODE + (symbol as usize - addr_of!(user_start) as usize)
}

// Index n is saved integer register xn. Trap entry preserves all GPRs.
#[repr(C)]
pub struct TrapFrame {
    regs: [usize; 32],
}

#[no_mangle]
pub unsafe extern "C" fn handle_trap(frame: &mut TrapFrame) {
    let cause: usize;
    let epc: usize;
    let tval: usize;
    let status: usize;
    asm!("csrr {v}, scause", v = out(reg) cause);
    asm!("csrr {v}, sepc", v = out(reg) epc);
    asm!("csrr {v}, stval", v = out(reg) tval);
    asm!("csrr {v}, sstatus", v = out(reg) status);
    put("trap: scause=");
    hex(cause);
    put(" sepc=");
    hex(epc);
    put(" stval=");
    hex(tval);
    put("\n");
    require(status & (1 << 8) == 0, "unexpected S-mode trap");
    if cause == 12 || cause == 13 || cause == 15 {
        require(ROUND == 1, "page fault with UKTE disabled");
        let (expected_cause, expected_pc, expected_address) = match FAULTS {
            0 => (13, user_pc(addr_of!(user_load)), HIGH_PAGE),
            1 => (15, user_pc(addr_of!(user_store)), HIGH_PAGE),
            2 => (12, HIGH_EXEC, HIGH_EXEC),
            _ => fail("extra page fault"),
        };
        require(
            cause == expected_cause && epc == expected_pc && tval == expected_address,
            "wrong fault cause, PC, or address",
        );
        require(data() == WRITTEN, "denied access changed data");
        FAULTS += 1;
        let resume_pc = if cause == 12 {
            let return_pc = user_pc(addr_of!(user_after_exec));
            require(frame.regs[1] == return_pc, "wrong execution return PC");
            require(frame.regs[15] == 0, "denied high code executed");
            put("[OK] UKTE blocked high execution (12)\n");
            // Fetch fault sepc points into the high page, so resume at the
            // known low caller continuation, rather than skipping high code.
            return_pc
        } else {
            put(if cause == 13 {
                "[OK] UKTE blocked high load (13)\n"
            } else {
                "[OK] UKTE blocked high store (15)\n"
            });
            epc + 4
        };
        asm!("csrw sepc, {pc}", pc = in(reg) resume_pc);
        return;
    }
    require(cause == 8, "unexpected exception (expected U ecall)");
    require(frame.regs[17] == 1, "U-mode data comparison failed");
    let protected = ROUND == 1;
    require(FAULTS == if protected { 3 } else { 0 }, "wrong fault count");
    require(
        data() == if protected { WRITTEN } else { INITIAL },
        "wrong final data",
    );
    put(match ROUND {
        0 => "[OK] baseline U-mode high read/write succeeded\n",
        1 => "[OK] protected U-mode low alias read/write succeeded\n",
        _ => "[OK] restored U-mode high read/write succeeded\n",
    });
    put(match ROUND {
        0 => "[OK] baseline U-mode high execution succeeded\n",
        1 => "[OK] protected U-mode low execution alias succeeded\n",
        _ => "[OK] restored U-mode high execution succeeded\n",
    });
    if ROUND == 2 {
        put("[PASS] Svukte MVP: all 3 rounds passed\n");
        finish(true);
    }
    ROUND += 1;
    prepare_round();
    frame.regs[2] = USER_STACK + 4096;
    frame.regs[10] = HIGH_PAGE;
    frame.regs[11] = USER_ALIAS;
    frame.regs[12] = usize::from(ROUND == 1);
    asm!("csrw sepc, {pc}", pc = in(reg) USER_CODE);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail("Rust panic")
}
