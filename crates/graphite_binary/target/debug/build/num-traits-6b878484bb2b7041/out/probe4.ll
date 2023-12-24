; ModuleID = 'probe4.19a32b8006b717d7-cgu.0'
source_filename = "probe4.19a32b8006b717d7-cgu.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"
target triple = "arm64-apple-macosx11.0.0"

@alloc_5f53a383c894c5ae722f760bee094514 = private unnamed_addr constant <{ [75 x i8] }> <{ [75 x i8] c"/rustc/79e9716c980570bfd1f666e3b16ac583f0168962/library/core/src/num/mod.rs" }>, align 1
@alloc_cf3034d727710e592fcfa8b540e4c106 = private unnamed_addr constant <{ ptr, [16 x i8] }> <{ ptr @alloc_5f53a383c894c5ae722f760bee094514, [16 x i8] c"K\00\00\00\00\00\00\00v\04\00\00\05\00\00\00" }>, align 8
@str.0 = internal constant [25 x i8] c"attempt to divide by zero"

; probe4::probe
; Function Attrs: uwtable
define void @_ZN6probe45probe17h700942ab99476e80E() unnamed_addr #0 {
start:
  %0 = call i1 @llvm.expect.i1(i1 false, i1 false)
  br i1 %0, label %panic.i, label %"_ZN4core3num21_$LT$impl$u20$u32$GT$10div_euclid17h4c5246855ab5b1c4E.exit"

panic.i:                                          ; preds = %start
; call core::panicking::panic
  call void @_ZN4core9panicking5panic17hca9af9b940213a79E(ptr align 1 @str.0, i64 25, ptr align 8 @alloc_cf3034d727710e592fcfa8b540e4c106) #3
  unreachable

"_ZN4core3num21_$LT$impl$u20$u32$GT$10div_euclid17h4c5246855ab5b1c4E.exit": ; preds = %start
  ret void
}

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare i1 @llvm.expect.i1(i1, i1) #1

; core::panicking::panic
; Function Attrs: cold noinline noreturn uwtable
declare void @_ZN4core9panicking5panic17hca9af9b940213a79E(ptr align 1, i64, ptr align 8) unnamed_addr #2

attributes #0 = { uwtable "frame-pointer"="non-leaf" "target-cpu"="apple-m1" }
attributes #1 = { nocallback nofree nosync nounwind willreturn memory(none) }
attributes #2 = { cold noinline noreturn uwtable "frame-pointer"="non-leaf" "target-cpu"="apple-m1" }
attributes #3 = { noreturn }

!llvm.module.flags = !{!0}
!llvm.ident = !{!1}

!0 = !{i32 8, !"PIC Level", i32 2}
!1 = !{!"rustc version 1.74.0 (79e9716c9 2023-11-13)"}
