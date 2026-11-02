# Svukte MVP

验证结论：**高地址 User page 在 UKTE 关闭时允许 U-mode 按页权限读写或执行，开启后这些访问被拦截，再关闭后恢复。**

内核运行于高地址 S-mode，使用 Sv39；用户程序从低地址发起访问。
数据页 `0xffffffd000000000` 保持 `U=1, R=1, W=1`；
代码页 `0xffffffd000001000` 保持 `U=1, X=1, R=W=0`。
通过 `senvcfg.UKTE` 开关保护，三轮实测结果如下：

| UKTE | 读取数据页 | 写入数据页 | 执行代码页 |
| --- | --- | --- | --- |
| 0（关闭） | 成功 | 成功 | 成功返回并设置标记 |
| 1（开启） | Load page fault（13） | Store page fault（15） | Instruction page fault（12） |
| 0（再次关闭） | 恢复成功 | 恢复成功 | 恢复成功 |

同时验证低地址数据别名仍能读写、低地址代码别名仍能执行。
执行异常核对 `sepc=stval=0xffffffd000001000`，再返回低地址继续测试。
读写、执行测试及关闭扩展的负对照均已在 QEMU 11.1.0 上验证通过。

## User page 地址布局

下图为 Sv39 虚拟地址，每个用户页大小为 4 KiB，地址从上到下递减（不按比例）。
`[A]` / `[B]` 表示共用的物理页。当前高地址代码页是 **X-only**（`R=0, X=1`）；
低地址发起测试的用户代码页是 **RX**。

```text
        Virtual address       U   RWX   Page / physical backing
        +----------------------------------------------------+
 High   | 0xffffffd000001000   1   --X   code target     [B] |
        | 0xffffffd000000000   1   RW-   data target     [A] |
        +----------------------------------------------------+
        |                      ...                           |
        +----------------------------------------------------+
 Low    | 0x0000000000015000   1   --X   code alias      [B] |
        | 0x0000000000013000   1   RW-   user stack          |
        | 0x0000000000012000   1   RW-   data alias      [A] |
        | 0x0000000000010000   1   R-X   user test code      |
        +----------------------------------------------------+
```

## 复现

需要支持 `x-svukte` 的 QEMU、Rust stable、Cargo 和 Python 3，使用 QEMU 自带的 OpenSBI。

```sh
# 首次安装 RISC-V 目标
rustup target add riscv64imac-unknown-none-elf

# 编译并验证
make build
make run

# 可选：关闭 QEMU 的 Svukte 扩展，验证 UKTE 不可用
make verify-no-svukte
```

复现脚本为 [`scripts/run.py`](scripts/run.py)，`make run` 会调用它检查结果，
不会隐式编译。可用 `make run QEMU=/path/to/qemu-system-riscv64` 指定 QEMU。

预期关键输出：

```text
[OK] baseline U-mode high read/write succeeded
[OK] baseline U-mode high execution succeeded
[OK] UKTE blocked high load (13)
[OK] UKTE blocked high store (15)
[OK] UKTE blocked high execution (12)
[OK] restored U-mode high read/write succeeded
[OK] restored U-mode high execution succeeded
[PASS] Svukte MVP: all 3 rounds passed
```

完整日志：`logs/svukte.log`；负对照日志：`logs/svukte-negative.log`。
