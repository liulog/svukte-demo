# run/verify deliberately have no build prerequisite.
.DEFAULT_GOAL := help
QEMU ?= qemu-system-riscv64
CPU ?= rv64,priv_spec=v1.13.0,x-svukte=on
BIOS ?= default
KERNEL ?= target/riscv64imac-unknown-none-elf/release/svukte-demo
TIMEOUT ?= 20

.PHONY: help build run verify verify-no-svukte
help:
	@printf '%s\n' \
	  'make build             Explicitly compile the Rust kernel (needs RISC-V target)' \
	  'make run               Run existing kernel and verify the three rounds' \
	  'make verify            Same as run; log saved to logs/svukte.log' \
	  'make verify-no-svukte   Negative control: UKTE must be unavailable' \
	  'Override QEMU, CPU, BIOS, KERNEL, TIMEOUT as needed.' \
	  'run/verify never compile, install, or rebuild anything.'

build:
	cargo build --release

run verify:
	python3 -B scripts/run.py --qemu "$(QEMU)" --cpu "$(CPU)" --bios "$(BIOS)" --kernel "$(KERNEL)" --timeout "$(TIMEOUT)"

verify-no-svukte:
	python3 -B scripts/run.py --qemu "$(QEMU)" --cpu "rv64,priv_spec=v1.13.0,x-svukte=off" --bios "$(BIOS)" --kernel "$(KERNEL)" --timeout "$(TIMEOUT)" --negative
