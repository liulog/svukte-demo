# Svukte MVP

无依赖、`no_std` 的 RV64 Rust 小内核。使用 QEMU `virt` 自带的 OpenSBI
进入 **S-mode**，开启 **Sv39** 后跳到高地址运行，再通过 `sret` 进入低地址
U-mode，检查 `senvcfg.UKTE` 对高地址读写的拦截。

## 运行

需要现有 QEMU 支持实验性 `x-svukte`，Rust stable、Cargo 及
`riscv64imac-unknown-none-elf` 目标。使用 Rust 自带的 `rust-lld`，
不需要 GNU 交叉工具链、第三方 crate、rootfs 或自行编译 OpenSBI。

首次准备目标（安装命令，需要自行明确执行）：

```sh
rustup target add riscv64imac-unknown-none-elf
```

显式构建，然后运行已有 ELF：

```sh
make build
make run
make verify-no-svukte
```

`make` 默认只打印帮助。`run` 与 `verify` 等价，均**不隐式构建**；ELF
不存在时直接报错，修改源码后需要显式重新执行 `make build`。验证脚本检查
输出、退出状态及超时，失败返回非零；日志位于 `logs/`。

```sh
make run QEMU=/opt/qemu-11.1.0/bin/qemu-system-riscv64 TIMEOUT=30
make run BIOS=/path/to/fw_dynamic.bin
```

默认 CPU 为 `rv64,priv_spec=v1.13.0,x-svukte=on`，可用 `CPU=...` 覆盖。
`verify-no-svukte` 固定使用 `x-svukte=off`；它只有在基线成功且 UKTE
写入读回为零时才通过，QEMU 参数错误、超时或其他失败均不算通过。
串口输出在 QEMU 退出或超时后一起显示。单核、128 MiB 内存、TCG；
固件需要支持 legacy SBI console 和 SBI SRST。

## 映射与执行流程

| 用途 | 虚拟地址 | 权限/映射 |
| --- | --- | --- |
| OpenSBI 入口 trampoline | `0x80200000` | 暂时 identity map，随后移除 |
| 内核 direct map | `0xffffffc080000000` 起 | 1 GiB，`U=0, RWX`，PA 从 `0x80000000` 起 |
| 用户代码 | `0x10000` | 4 KiB，`U=1, RX` |
| 用户低地址数据别名 | `0x12000` | 4 KiB，`U=1, RW` |
| 用户栈 | `0x13000` | 4 KiB，`U=1, RW`，初始 SP=`0x14000` |
| 高地址被测页 | `0xffffffd000000000` | 4 KiB，`U=1, RW`，与低地址别名共用物理页 |

所有地址均为合法 Sv39 canonical VA。数据页刻意设置 **U=1、R=1、W=1**，
并预置 A/D 位，避免把普通权限错误或 A/D 缺页当作 Svukte 行为。
高地址页没有执行权限，本 MVP 验证 load/store，不测试 instruction fetch。

内核先验证自身 PC 位于高地址且 `satp.MODE=8`，然后执行三轮：

1. **UKTE=0**：低地址 U-mode 成功读写高地址页，证明映射和权限允许访问。
2. **UKTE=1**：同一段用户代码仍能读写低地址别名；高地址 `ld` 触发
   `scause=13`，`sd` 触发 `scause=15`。内核逐次核对异常来源是 U-mode、
   fault PC、`stval` 和物理页内容，再跳过对应 4 字节指令。用户程序还检查
   失败的 load 未改变目标寄存器，失败的 store 未改变数据。
3. **UKTE=0**：高地址读写重新成功，验证关闭后的恢复行为。

每轮还在 S-mode 设置 `SUM=1` 后读写这个高地址 U-page，验证 S-mode
不受 UKTE 拦截。每次改变 UKTE 后执行全局 `sfence.vma`，清除上一轮的
翻译缓存；页表和 PTE 权限在三轮中保持不变。

成功输出包含：

```text
[OK] baseline U-mode high read/write succeeded
[OK] UKTE blocked high load (13)
[OK] UKTE blocked high store (15)
[OK] protected U-mode low alias read/write succeeded
[OK] restored U-mode high read/write succeeded
[PASS] Svukte MVP: all 3 rounds passed
```

`src/boot.S` 负责启动、完整整数寄存器 trap frame、用户代码；
`src/main.rs` 负责页表、CSR、异常校验和输出；`linker.ld` 区分物理加载地址
与高虚拟地址。内核使用粗粒度 RWX direct map 以保持 MVP 简单，
没有调度器、堆、设备驱动或中断。这是功能验证，**不测量恒定时间或侧信道性质**。

机制依据：QEMU 的
[Svukte UKTE 支持补丁](https://lists.gnu.org/archive/html/qemu-riscv/2024-11/msg00256.html)
定义 `senvcfg` bit 8；异常路径还对照了本机现有的
`../smmpt-demo/qemu-smmpt/target/riscv/tcg/cpu_helper.c` 中的
`do_svukte_check` / `check_svukte_addr`。

## 已验证结果

在本机 QEMU 11.1.0（自带 OpenSBI v1.8.1）上，`make build`、
`make verify` 和 `make verify-no-svukte` 均已通过。实测内核
PC=`0xffffffc08020285c`、`satp=0x8000000000080201`；受保护的 load/store
分别在用户 PC `0x10058` / `0x1006c` 触发 13 / 15 号异常，
`stval` 均为 `0xffffffd000000000`。具体代码地址会随编译结果变化。

完整正向与负对照日志分别保存在 `logs/svukte.log` 和
`logs/svukte-negative.log`（不纳入版本控制）。验证器还检查了合成日志中
缺失、顺序错误、重复及失败标记的拒绝行为。
