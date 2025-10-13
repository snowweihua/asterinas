/* SPDX-License-Identifier: MPL-2.0 */

KERNEL_VMA             = 0xffff000000000000

// Copy code from https://github.com/RiversJin/ToyOS
/* SCR_EL3 Secure Configuration Register EL3 */
/*
    [0]:1 NS none secure EL0与EL1处于Non-secure状态
    [1]:0 IRQ 路由 当设为0时:(在EL3级别以下产生IRQ时,不会带入EL3;当在EL3时,不会产生IRQ) 当设为1时:无论哪一EL,产生IQR时都会带入到EL3
    [2]:0 FIQ 路由 此位控制IRQ 与上面的IRQ路由同理
    [3]:0 EA 路由 控制Abort与SErro路由 与上面IRQ同理

    [5:4]:1 RES1 保留位 置1 注意Arm手册中,有两种保留位,RES1需要置1;RES0需要置0
    [6]:0 RES0
    [7]:1 SMD Secure Monitor Call 在EL1级别及以上禁用SMC指令(Secure Monitor Call)
    [8]:1 HCE Hypervisor Call instruction enable. 使能hvc指令
    [9]:0 SIF Secure instruction fetch 允许在stage1的地址翻译时从标记为非安全状态的内存中获取安全状态的指令 置1为不允许,咱们用不上Arm的secure,那么置0就行
    [10]:1 若此位为0 则EL3级别以下只可使用aarch32 置1后可以使用aarch64
    再高位的东西咱们也用不上,就不继续讲解了,有兴趣的可以参考aarch64手册
    https://developer.arm.com/documentation/ddi0595/2021-06/AArch64-Registers/SCR-EL3--Secure-Configuration-Register
*/
#define SCR_NS (1)
#define SCR_EXCEPTION_ROUTE (0 << 1)
#define SCR_RES1 (0b11 << 4)
#define SCR_RES0 (0 << 6)
#define SCR_SMD_DISABLE (1 << 7)
#define SCR_HCE_ENABLE (1 << 8)
#define SCR_SIF (0 << 9)
#define SCR_RW (1 << 10)

// SCR_VALUE 的值应该是 0b0101_1011_0001 0x5B1 1457
#define SCR_VALUE                                         \
    (SCR_NS | SCR_EXCEPTION_ROUTE | SCR_RES1 | SCR_RES0 | \
     SCR_SMD_DISABLE | SCR_HCE_ENABLE | SCR_SIF | SCR_RW)

/* SPSR_ELx Saved Program Status Register */
/*
    可以理解为 此寄存器保存了CPSR_EL(x-1),所以当使用eret指令后,
    此寄存器状态控制了返回至EL(x-1)的状态
    向spsr_el3写入0b0001_1100_1001
	[3:0]:1001 
	[4]:0 此位为0,表示进入此EL之前工作在aarch64状态
	[5]:0 RES0
	[6]:1 FIQ interrupt mask 当跳转至EL2时,此位复制到EL2的PSTATE,也就关闭了FIQ中断
	[7]:1 IRQ mask
	[8]:1 SError mask
	[9]:0 Debug mask
	剩下的用不上 有兴趣参考手册
	https://developer.arm.com/documentation/ddi0595/2021-06/AArch64-Registers/SPSR-EL3--Saved-Program-Status-Register--EL3-
*/
#define SPSR_MASK_FIS (0b111 << 6)
#define SPSR_EL1h (0b0101 << 0)
#define SPSR_EL2h (0b1001 << 0)
// SPSR_EL3_VALUE 的值应该是 0b0001_1100_1001 0x1C9 457
#define SPSR_EL3_VALUE (SPSR_MASK_FIS | SPSR_EL2h)
// SPSR_EL2_VALUE 的值应该是 0b0001_1100_0101 0x1C5 453
#define SPSR_EL2_VALUE (SPSR_MASK_FIS | SPSR_EL1h)

/* SCTLR_EL1, System Control Register (EL1). */
#define SCTLR_RESERVED \
    ((3 << 28) | (3 << 22) | (1 << 20) | (1 << 11) | (1 << 8) | (1 << 7))
#define SCTLR_EE_LITTLE_ENDIAN (0 << 25)
#define SCTLR_E0E_LITTLE_ENDIAN (0 << 24)
#define SCTLR_I_CACHE (1 << 12)
#define SCTLR_D_CACHE (1 << 2)
#define SCTLR_MMU_DISABLED (0 << 0)
#define SCTLR_MMU_ENABLED (1 << 0)

#define SCTLR_VALUE_MMU_DISABLED_VALUE \
    (SCTLR_RESERVED | SCTLR_EE_LITTLE_ENDIAN | SCTLR_E0E_LITTLE_ENDIAN | SCTLR_I_CACHE | SCTLR_D_CACHE | SCTLR_MMU_DISABLED)


.section ".boot", "awx", @progbits
.globl _start
_start:
    # EL1, 
    mrs     x2, CurrentEL // 读取当前异常级别 在此寄存器中 我们需要的是[3:2]
    and     x2, x2，#0b1100
    cmp     x2，#0b1000
    beq     switch_to_el1_from_el2 // branch on equal

switch_to_el2_from_el3:
    // 向scr_el3写入0101_1011_0001 0x5B1
    ldr     x1, =(SCR_VALUE)
    msr     scr_el3, x1
    // 向spsr_el3写入0001_1100_1001
    ldr     x2, =(SPSR_EL3_VALUE)
    msr     spsr_el3, x2
    // 加载函数switch_to_el1_from_el2的地址,设置从EL3返回时的地址,这样调用eret即可跳转至此函数继续执行
    adr     x3, switch_to_el1_from_el2
    msr     elr_el3, x3
    eret

switch_to_el1_from_el2:
    /* 使能el1与el0对el物理计数器reg,定时器reg的访问 */
    mrs     x0, cnthctl_el2
    orr     x0, x0, #3
    msr     cnthctl_el2, x0
    msr     cntvoff_el2, xzr
    // hcr_el2.rw = 1 设置在el1时使用aarch64 (否则为aarch32)
    mov     x1, #(1 << 31)
    // hcr_el2.swio = 1 设置在el1时 Set/Way指令有效
    orr     x1, x1, #(1 << 1)   /* SWIO hardwired on Pi3 */
    msr     hcr_el2, x1
    mrs     x1, hcr_el2

    /* Setup SCTLR access. */
    ldr     x3, =(SCTLR_VALUE_MMU_DISABLED_VALUE)
    msr     sctlr_el1, x3
    
    ldr     x4, =(SPSR_EL2_VALUE)
    msr     spsr_el2, x4
    // 读取符号"jump_to_main"的位置
    adr     x4, jump_to_main
    // 设定从"jump_to_main"处进入当前的EL2状态,这样在使用eret指令后即可以EL1的状态跳转至"jump_to_main"
    msr     elr_el2, x4
    eret

/* 接下来配置TCR 翻译控制寄存器 */
/*
    TCR_ELx的格式如下
    [63:39] RES0
    [38] TBI1 Top Byte ignored 指示地址的顶部字节是用于TTBR1_EL1区域的地址匹配(置0),还是被忽略并用于标记的地址(置1)
    [37] TBI0 同TBI1 只不过是TTBR0_EL1区域地址
    [36] ASID Size 置1时ASID为16位 0为8位 若实现不支持16位,那么此位视为RES0
    [35] RES0
    [34:32] 中间物理地址大小
        000 32bits 4G
        001 26bits 64G
        010 40bits 1TB
        011 42bits 4TB
        100 44bits 16TB
        101 48bits 256TB
        其余值视为101,但未来架构可能修改
    [31:30] TG1 TTBR1_EL1的粒度
        01 16K
        10 4K
        11 64K
    [29:28] SH1 TTBR1_EL1的Shareability
        01 Non-shareable
        10 Outer-shareable
        11 Inner-shareable
    [27:26] ORGN1 TTBR1_EL1的Outer cacheability
        00 Outer Non-cacheable
        01 Outer 写回 写分配
        10 写直通
        11 写回 不写分配
    [25:24] IRGN1 TTBR1_EL1的Inner cacheability 编码与ORNG1相同
    [23] EPD1 当使用TTBR1时 若此为置1,当遍历页表时TLB发生miss,会发生translation fault
    [22] 当为0时,TTBR0_EL1.ASID定义ASID 为1时TTBR1_EL1.ASID定义ASID
    [21:16] T1SZ  TTBR1_EL1的内存区域为64-T1SZ 比如我们需要48位的虚地址空间,那就需要将TxSZ设置为64-48
    [15:14] TG0 TTBR0_EL1的粒度
        00 4K
        01 64K
        10 16K
    [13:12] SH0 TTBR0_EL1的Shareability 与SH1编码相同
    [11:10] ORGN0 TTBR0_EL1的Outer cacheability 编码与ORGN1相同
    [9:8] IRGN0
    [7] EPD0 见EPD1
    [6] RES0
    [5:0] T0SZ
*/
#define TCR_IPS (0 << 32)
#define TCR_TG1_4K (0b10 << 30)
#define TCR_SH1_INNER (0b11 << 28)
#define TCR_ORGN1_IRGN1_WRITEBACK_WRITEALLOC ((0b01 << 26) | (0b01 << 24))
#define TCR_TG0_4K (0 << 14)
#define TCR_SH0_INNER (0b11 << 12)
#define TCR_ORGN0_IRGN0_WRITEBACK_WRITEALLOC ((0b01 << 10) | (0b01 << 8))
#define TCR_VALUE                                                        \
    (TCR_IPS |                                                           \
     TCR_TG1_4K | TCR_SH1_INNER | TCR_ORGN1_IRGN1_WRITEBACK_WRITEALLOC | \
     TCR_TG0_4K | TCR_SH0_INNER | TCR_ORGN0_IRGN0_WRITEBACK_WRITEALLOC)

/* 
    在AARCH64的MMU页表项中,在4K页粒度下,格式如下
    |63 --- 52|51 --- 48|47 --- 12|11 --- 2| 1 | 0|
    [63:52] : Upper attributes
    [11: 2] : Lower attributes
    这两个合起来才是所有的属性,在这些属性中,我们要关注的位有这些
    [47:12] : 输出地址(有可能是下一级页表的地址,也有可能是物理页框的地址)
    在Stage1翻译的情况下,我们要关注的有
    [54] UXN/XN Execute-never bit 也就是说,此位置1时,禁止EL0特权时执行此内存内容
    [53] PXN    Priviledged execute-never bit 当此位置1时,禁止EL1特权执行此内存内容
    [10] AF Access flag 当此位置0时,访问此位会产生异常 可以使用此位用来实现页面回收 如果不想要这个功能 置1即可
    [9:8] SH Shareability field 控制共享域 00 Non-share 10 Outer-Share 11 Inner-Share
    [7:6] AP Access permissions 控制读写权限
            AP      EL1     EL0
            00      R/W     None
            01      R/W     R/W
            10      R       None
            11      R       R
    [4:2] AttrIdx 控制内存属性的索引,指向寄存器MAIR_ELx的八个元素,而这个寄存器中,需要哪些属性,在哪个位置,由用户决定
    [1:0] 页表类型,参见MM_TYPE_INVALID MM_TYPE_BLOCK MM_TYPE_TABLE
*/

#define MM_TYPE_INVALID 0b00
#define MM_TYPE_BLOCK 0b01
#define MM_TYPE_TABLE 0b11
#define MM_TYPE_PAGE  0b11

/* Memory Attributes 控制这个页表项对应的内存区域的内存类型,缓存策略 */
#define MA_DEVICE_nGnRnE_Flags 0x0   // 设备内存 禁止聚集(non Gathering) 禁止重排(non re-order) 禁止提前的写入ACK(Early Write Acknowledgement)
#define MA_MEMORY_Flags 0xFF         // 普通内存 使用写回写分配 读分配 (最快的形式,各种缓存buff拉满)
#define MA_MEMORY_NoCache_Flags 0x44 // 普通内存 禁止所有缓存策略

// MAIR_ELx 可以放置8种Memory Attributes,但是我们只需要这三种就够了
#define MA_DEVICE_nGnRnE 0
#define MA_MEMORY 1
#define MA_MEMORY_NoCache 2

#define PTE_AIDX_DEVICE_nGnRn (MA_DEVICE_nGnRnE << 2)
#define PTE_AIDX_MEMORY (MA_MEMORY << 2)
#define PTE_AIDX_MEMORY_NOCACHE (MA_MEMORY_NoCache << 2)
// 这个值 我们一会放到MAIR_EL1 寄存器中
#define MAIR_VALUE \
    (MA_DEVICE_nGnRnE_Flags << (8 * MA_DEVICE_nGnRnE)) | (MA_MEMORY_Flags << (8 * MA_MEMORY)) | (MA_MEMORY_NoCache_Flags << (8 * MA_MEMORY_NoCache))

jump_to_main:
// 此时CPU已经工作在EL1状态
    /* 配置页表 */
    adr     x2, boot_l4pt // 取得页表地址
    msr     ttbr0_el1, x2
    msr     ttbr1_el1, x2
    ldr     x3, =(TCR_VALUE)
    msr     tcr_el1, x3
    ldr     x4, =(MAIR_VALUE)
    msr     mair_el1, x4

    /* 使能MMU */
    mrs     x5, sctlr_el1
    orr     x5, x5, #SCTLR_MMU_ENABLED
    msr     sctlr_el1, x5 
    isb     //修改页表后要指令同步

    # Update SP/PC to use the virtual address.
    msr     spsel, #1
    mrs     x6, mpidr_el1
    and     x6, x6, #0xFF
// Copy code End 
    mov     x1, #KERNEL_VMA
    ldr     sp, =boot_stack_top
    orr     sp, sp, x1
    ldr     x1, =_start_virt
    br      x1      

#define PTE_VALID 1
#define PTE_BLOCK MM_TYPE_BLOCK
#define PTE_TABLE MM_TYPE_TABLE
#define PTE_PAGE MM_TYPE_PAGE
#define PTE_KERNEL (AP_PRIVILEGED << 6)
#define PTE_USER (AP_UNPREVILEGED << 6)
#define PTE_RW (AP_RW << 7)
#define PTE_RO (AP_RO << 7)
#define PTE_SH (0b11 << 8) // 对于SMP系统来说,全部设置为Inner-share就可以了
#define PTE_AF_USED (1 << 10)
#define PTE_AF_UNUSED (0 << 10)

// 默认页表项 带缓存的memory,inner share,特权读写 AF置位
#define PTE_NORMAL_MEMORY ( MM_TYPE_BLOCK | PTE_AIDX_MEMORY | PTE_SH | PTE_AF_USED | PTE_RW )
// 设备内存页表项
#define PTE_DEVICE_MEMORY (MM_TYPE_BLOCK | PTE_AIDX_DEVICE_nGnRn | PTE_AF_USED)

.balign 4096
.globl boot_l4pt
boot_l4pt:    
    .zero 8 * 512
boot_l3pt:
    .zero 8 * 512
boot_l2pt:
    .zero 8 * 512

.section ".boot.stack", "aw", @nobits
.globl boot_stack_top
boot_stack_bottom:
    .balign 4096
    .skip 0x40000  # 256 KiB
boot_stack_top:

# From here, we're in the .text section: we no longer use physical address.
.text
.globl _start_virt
_start_virt:

    # Jump into Rust code.
    b     aarch64_boot
