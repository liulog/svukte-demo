#!/usr/bin/env python3
"""Run an existing ELF only. Never invoke a compiler or dependency installer."""

import argparse
from pathlib import Path
import subprocess
import sys


CHECKS = (
    "[OK] Sv39 high kernel; boot identity mapping removed",
    "[ROUND] UKTE=0 baseline",
    "[OK] baseline U-mode high read/write succeeded",
    "[ROUND] UKTE=1 protected",
    "[OK] UKTE blocked high load (13)",
    "[OK] UKTE blocked high store (15)",
    "[OK] protected U-mode low alias read/write succeeded",
    "[ROUND] UKTE=0 restored",
    "[OK] restored U-mode high read/write succeeded",
    "[PASS] Svukte MVP: all 3 rounds passed",
)
UNAVAILABLE = "[FAIL] UKTE unavailable: senvcfg.UKTE readback is zero"


def validate(output, negative):
    """Markers are ordered; the kernel separately validates CSR/PC/data values."""
    if negative:
        return (
            UNAVAILABLE in output
            and CHECKS[2] in output
            and output.count("[FAIL]") == 1
            and "[PASS]" not in output
        )
    if "[FAIL]" in output or output.count(CHECKS[-1]) != 1:
        return False
    cursor = 0
    for marker in CHECKS:
        found = output.find(marker, cursor)
        if found < 0:
            return False
        cursor = found + len(marker)
    return (
        output.count("[OK] UKTE blocked high load (13)") == 1
        and output.count("[OK] UKTE blocked high store (15)") == 1
        and output.count("[OK] S-mode high U-page read/write with SUM=1") == 3
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default="qemu-system-riscv64")
    parser.add_argument("--cpu", default="rv64,priv_spec=v1.13.0,x-svukte=on")
    parser.add_argument("--bios", default="default")
    parser.add_argument("--kernel", default="target/riscv64imac-unknown-none-elf/release/svukte-demo")
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--negative", action="store_true")
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    if not Path(args.kernel).is_file():
        parser.error(f"kernel ELF does not exist: {args.kernel}; explicitly run make build first")
    command = [
        args.qemu, "-machine", "virt", "-accel", "tcg", "-cpu", args.cpu,
        "-m", "128M", "-smp", "1", "-nographic", "-monitor", "none",
        "-no-reboot", "-bios", args.bios, "-kernel", args.kernel,
    ]
    timed_out = False
    try:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                stdin=subprocess.DEVNULL, timeout=args.timeout)
        output = result.stdout.decode(errors="replace")
        returncode = result.returncode
    except subprocess.TimeoutExpired as error:
        output = (error.stdout or b"").decode(errors="replace")
        returncode = None
        timed_out = True
    except OSError as error:
        print(f"cannot start QEMU: {error}", file=sys.stderr)
        return 1
    log = Path("logs/svukte-negative.log" if args.negative else "logs/svukte.log")
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text(output)
    print(output, end="", flush=True)
    # OpenSBI's deliberate failure shutdown may give QEMU exit status 1.
    valid_exit = returncode == 0 or (args.negative and returncode == 1)
    if timed_out or not valid_exit or not validate(output, args.negative):
        print(f"verification failed (timeout={timed_out}, exit={returncode}); log: {log}",
              file=sys.stderr)
        return 1
    print(f"{'negative control' if args.negative else 'Svukte'} verified; log: {log}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
