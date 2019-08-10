; ModuleID = 'crossmod.c'
source_filename = "crossmod.c"
target datalayout = "e-m:o-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-apple-macosx10.14.0"

; Function Attrs: noinline nounwind ssp uwtable
define i32 @cross_module_simple_caller(i32) local_unnamed_addr #0 {
  %2 = tail call i32 @simple_callee(i32 %0, i32 3) #3
  ret i32 %2
}

declare i32 @simple_callee(i32, i32) local_unnamed_addr #1

; Function Attrs: nounwind ssp uwtable
define i32 @cross_module_twice_caller(i32) local_unnamed_addr #2 {
  %2 = tail call i32 @simple_callee(i32 %0, i32 5) #3
  %3 = tail call i32 @simple_callee(i32 %0, i32 1) #3
  %4 = add nsw i32 %3, %2
  ret i32 %4
}

; Function Attrs: nounwind ssp uwtable
define i32 @cross_module_nested_near_caller(i32, i32) local_unnamed_addr #2 {
  %3 = add nsw i32 %1, %0
  %4 = tail call i32 @cross_module_simple_caller(i32 %3)
  ret i32 %4
}

; Function Attrs: nounwind ssp uwtable
define i32 @cross_module_nested_far_caller(i32, i32) local_unnamed_addr #2 {
  %3 = add nsw i32 %1, %0
  %4 = tail call i32 @simple_caller(i32 %3) #3
  ret i32 %4
}

declare i32 @simple_caller(i32) local_unnamed_addr #1

attributes #0 = { noinline nounwind ssp uwtable "correctly-rounded-divide-sqrt-fp-math"="false" "disable-tail-calls"="false" "less-precise-fpmad"="false" "min-legal-vector-width"="0" "no-frame-pointer-elim"="true" "no-frame-pointer-elim-non-leaf" "no-infs-fp-math"="false" "no-jump-tables"="false" "no-nans-fp-math"="false" "no-signed-zeros-fp-math"="false" "no-trapping-math"="false" "stack-protector-buffer-size"="8" "target-cpu"="penryn" "target-features"="+cx16,+fxsr,+mmx,+sahf,+sse,+sse2,+sse3,+sse4.1,+ssse3,+x87" "unsafe-fp-math"="false" "use-soft-float"="false" }
attributes #1 = { "correctly-rounded-divide-sqrt-fp-math"="false" "disable-tail-calls"="false" "less-precise-fpmad"="false" "no-frame-pointer-elim"="true" "no-frame-pointer-elim-non-leaf" "no-infs-fp-math"="false" "no-nans-fp-math"="false" "no-signed-zeros-fp-math"="false" "no-trapping-math"="false" "stack-protector-buffer-size"="8" "target-cpu"="penryn" "target-features"="+cx16,+fxsr,+mmx,+sahf,+sse,+sse2,+sse3,+sse4.1,+ssse3,+x87" "unsafe-fp-math"="false" "use-soft-float"="false" }
attributes #2 = { nounwind ssp uwtable "correctly-rounded-divide-sqrt-fp-math"="false" "disable-tail-calls"="false" "less-precise-fpmad"="false" "min-legal-vector-width"="0" "no-frame-pointer-elim"="true" "no-frame-pointer-elim-non-leaf" "no-infs-fp-math"="false" "no-jump-tables"="false" "no-nans-fp-math"="false" "no-signed-zeros-fp-math"="false" "no-trapping-math"="false" "stack-protector-buffer-size"="8" "target-cpu"="penryn" "target-features"="+cx16,+fxsr,+mmx,+sahf,+sse,+sse2,+sse3,+sse4.1,+ssse3,+x87" "unsafe-fp-math"="false" "use-soft-float"="false" }
attributes #3 = { nounwind }

!llvm.module.flags = !{!0, !1}
!llvm.ident = !{!2}

!0 = !{i32 1, !"wchar_size", i32 4}
!1 = !{i32 7, !"PIC Level", i32 2}
!2 = !{!"clang version 8.0.0 (tags/RELEASE_800/final)"}