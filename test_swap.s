	.text
	.file	"test_swap.ddce3c0347def641-cgu.0"
	.section	.text._ZN9test_swap4test17hc46d4667928c08cfE,"ax",@progbits
	.globl	_ZN9test_swap4test17hc46d4667928c08cfE
	.p2align	2
	.type	_ZN9test_swap4test17hc46d4667928c08cfE,@function
_ZN9test_swap4test17hc46d4667928c08cfE:
	mov	w8, #1
.LBB0_1:
	ldaxrb	wzr, [x0]
	stxrb	w9, w8, [x0]
	cbnz	w9, .LBB0_1
	ret
.Lfunc_end0:
	.size	_ZN9test_swap4test17hc46d4667928c08cfE, .Lfunc_end0-_ZN9test_swap4test17hc46d4667928c08cfE

	.ident	"rustc version 1.86.0-nightly (854f22563 2025-01-31)"
	.section	".note.GNU-stack","",@progbits
