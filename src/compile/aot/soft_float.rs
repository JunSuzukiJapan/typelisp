//! Floating-point arithmetic for a processor that has none: the functions
//! the code generator calls for `float` and `double`, under the names
//! `compiler-rt` gives them, written in LLVM's IR over integers.
//!
//! IEEE 754 arithmetic, rounded to nearest, ties to even, subnormals
//! included. Written to be small rather than fast: a bit at a time, with
//! additions, comparisons and shifts by one, which is also all that a
//! processor with 8-bit registers does in place.
//!
//! One text for both widths, in which `W` is how wide the number is, `M` how
//! many bits of it are its fraction, and the other capitals below are
//! constants that follow from those ([`text`]).
//!
//! A finite number on its way is a sign (`W` bits, only the top one other
//! than zero), an exponent (16 bits, biased) and a significand: with its
//! leading bit at `M + 3`, so that three bits are below the fraction — the
//! lowest of them set when anything lower was lost — its value is
//! `significand * 2^(exponent - bias - M - 3)`.

use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;

/// The functions of one width.
const FUNCTIONS: &str = "
; The number of a sign, an exponent and a significand that is not zero and is
; below 2^(M+5), rounded.
define internal iW @sf_round_W(iW %sign, i16 %e0, iW %m0) noinline {
entry:
  br label %up
up:
  %e1 = phi i16 [ %e0, %entry ], [ %e1n, %upmore ]
  %m1 = phi iW [ %m0, %entry ], [ %m1n, %upmore ]
  %small = icmp ult iW %m1, TOP
  br i1 %small, label %upmore, label %down
upmore:
  %e1n = sub i16 %e1, 1
  %m1n = shl iW %m1, 1
  br label %up
down:
  %e2 = phi i16 [ %e1, %up ], [ %e2n, %downmore ]
  %m2 = phi iW [ %m1, %up ], [ %m2n, %downmore ]
  %large = icmp uge iW %m2, TOP2
  %low = icmp slt i16 %e2, 1
  %some = icmp ne iW %m2, 0
  %sub = and i1 %low, %some
  %more = or i1 %large, %sub
  br i1 %more, label %downmore, label %round
downmore:
  %e2n = add i16 %e2, 1
  %lost = and iW %m2, 1
  %half = lshr iW %m2, 1
  %m2n = or iW %half, %lost
  br label %down
round:
  %under = icmp slt i16 %e2, 1
  %e3 = select i1 %under, i16 1, i16 %e2
  %over = icmp sge i16 %e3, EMAX
  br i1 %over, label %infinite, label %finite
infinite:
  %inf = or iW %sign, INF
  ret iW %inf
finite:
  %rest = and iW %m2, 7
  %q1 = lshr iW %m2, 1
  %q2 = lshr iW %q1, 1
  %q = lshr iW %q2, 1
  %odd = and iW %q, 1
  %tie = or iW %rest, %odd
  %past = icmp ugt iW %tie, 4
  %inc = zext i1 %past to iW
  %r = add iW %q, %inc
  %field = sub i16 %e3, 1
  %wide = zext i16 %field to iW
  %placed = call iW @sf_place_W(iW %wide)
  %n = add iW %placed, %r
  %s = or iW %n, %sign
  ret iW %s
}

; An exponent, where a number has it.
define internal iW @sf_place_W(iW %e) noinline {
entry:
  br label %loop
loop:
  %v = phi iW [ %e, %entry ], [ %v1, %loop ]
  %i = phi i8 [ M, %entry ], [ %i1, %loop ]
  %v1 = shl iW %v, 1
  %i1 = sub i8 %i, 1
  %done = icmp eq i8 %i1, 0
  br i1 %done, label %end, label %loop
end:
  ret iW %v1
}

; The exponent of a number, its sign left out.
define internal i16 @sf_exponent_W(iW %x) noinline {
entry:
  %a = and iW %x, MAGNITUDE
  br label %loop
loop:
  %v = phi iW [ %a, %entry ], [ %v1, %loop ]
  %i = phi i8 [ M, %entry ], [ %i1, %loop ]
  %v1 = lshr iW %v, 1
  %i1 = sub i8 %i, 1
  %done = icmp eq i8 %i1, 0
  br i1 %done, label %end, label %loop
end:
  %e = trunc iW %v1 to i16
  ret i16 %e
}

; The significand of a number whose exponent is %e, its leading bit at M.
define internal iW @sf_significand_W(iW %x, i16 %e) {
  %f = and iW %x, FRACTION
  %sub = icmp eq i16 %e, 0
  %lead = select i1 %sub, iW 0, iW ONE
  %m = or iW %f, %lead
  ret iW %m
}

; What a number that is not a zero needs its exponent %e lowered by for its
; significand %m to lead at M, as that exponent.
define internal i16 @sf_leading_W(iW %m, i16 %e) noinline {
entry:
  %sub = icmp eq i16 %e, 0
  %e1 = select i1 %sub, i16 1, i16 %e
  br label %loop
loop:
  %en = phi i16 [ %e1, %entry ], [ %en1, %more ]
  %mn = phi iW [ %m, %entry ], [ %mn1, %more ]
  %small = icmp ult iW %mn, ONE
  br i1 %small, label %more, label %end
more:
  %en1 = sub i16 %en, 1
  %mn1 = shl iW %mn, 1
  br label %loop
end:
  ret i16 %en
}

; The significand %m with its leading bit at M.
define internal iW @sf_led_W(iW %m) noinline {
entry:
  br label %loop
loop:
  %mn = phi iW [ %m, %entry ], [ %mn1, %more ]
  %small = icmp ult iW %mn, ONE
  br i1 %small, label %more, label %end
more:
  %mn1 = shl iW %mn, 1
  br label %loop
end:
  ret iW %mn
}

define internal iW @sf_shl3_W(iW %x) {
  %a = shl iW %x, 1
  %b = shl iW %a, 1
  %c = shl iW %b, 1
  ret iW %c
}

define iW @__addSF3(iW %a, iW %b) noinline {
entry:
  %aa = and iW %a, MAGNITUDE
  %ab = and iW %b, MAGNITUDE
  %na = icmp ugt iW %aa, INF
  %nb = icmp ugt iW %ab, INF
  %nn = or i1 %na, %nb
  br i1 %nn, label %nan, label %numbers
nan:
  ret iW NAN
numbers:
  %swap = icmp ult iW %aa, %ab
  %x = select i1 %swap, iW %b, iW %a
  %y = select i1 %swap, iW %a, iW %b
  %ax = select i1 %swap, iW %ab, iW %aa
  %ay = select i1 %swap, iW %aa, iW %ab
  %differ = xor iW %x, %y
  %opposite = icmp slt iW %differ, 0
  %xinf = icmp eq iW %ax, INF
  br i1 %xinf, label %infinite, label %finite
infinite:
  %yinf = icmp eq iW %ay, INF
  %none = and i1 %yinf, %opposite
  %ir = select i1 %none, iW NAN, iW %x
  ret iW %ir
finite:
  %ex = call i16 @sf_exponent_W(iW %x)
  %ey = call i16 @sf_exponent_W(iW %y)
  %mx = call iW @sf_significand_W(iW %x, i16 %ex)
  %my = call iW @sf_significand_W(iW %y, i16 %ey)
  %xsub = icmp eq i16 %ex, 0
  %ex1 = select i1 %xsub, i16 1, i16 %ex
  %ysub = icmp eq i16 %ey, 0
  %ey1 = select i1 %ysub, i16 1, i16 %ey
  %gx = call iW @sf_shl3_W(iW %mx)
  %gy = call iW @sf_shl3_W(iW %my)
  %far = sub i16 %ex1, %ey1
  br label %align
align:
  %d = phi i16 [ %far, %finite ], [ %d1, %alignmore ]
  %g = phi iW [ %gy, %finite ], [ %g1, %alignmore ]
  %apart = icmp ne i16 %d, 0
  %left = icmp ugt iW %g, 1
  %go = and i1 %apart, %left
  br i1 %go, label %alignmore, label %aligned
alignmore:
  %d1 = sub i16 %d, 1
  %lost = and iW %g, 1
  %h = lshr iW %g, 1
  %g1 = or iW %h, %lost
  br label %align
aligned:
  %sum = add iW %gx, %g
  %difference = sub iW %gx, %g
  %m = select i1 %opposite, iW %difference, iW %sum
  %zero = icmp eq iW %m, 0
  br i1 %zero, label %nothing, label %something
nothing:
  %both = and iW %x, %y
  %zs = and iW %both, SIGN
  ret iW %zs
something:
  %s = and iW %x, SIGN
  %r = call iW @sf_round_W(iW %s, i16 %ex1, iW %m)
  ret iW %r
}

define iW @__subSF3(iW %a, iW %b) {
  %n = xor iW %b, SIGN
  %r = call iW @__addSF3(iW %a, iW %n)
  ret iW %r
}

define iW @__mulSF3(iW %a, iW %b) noinline {
entry:
  %sx = xor iW %a, %b
  %s = and iW %sx, SIGN
  %aa = and iW %a, MAGNITUDE
  %ab = and iW %b, MAGNITUDE
  %na = icmp ugt iW %aa, INF
  %nb = icmp ugt iW %ab, INF
  %nn = or i1 %na, %nb
  br i1 %nn, label %nan, label %numbers
nan:
  ret iW NAN
numbers:
  %ia = icmp eq iW %aa, INF
  %ib = icmp eq iW %ab, INF
  %ii = or i1 %ia, %ib
  %za = icmp eq iW %aa, 0
  %zb = icmp eq iW %ab, 0
  %zz = or i1 %za, %zb
  br i1 %ii, label %infinite, label %finite
infinite:
  %inf = or iW %s, INF
  %ir = select i1 %zz, iW NAN, iW %inf
  ret iW %ir
finite:
  br i1 %zz, label %nothing, label %something
nothing:
  ret iW %s
something:
  %ea = call i16 @sf_exponent_W(iW %a)
  %eb = call i16 @sf_exponent_W(iW %b)
  %ma = call iW @sf_significand_W(iW %a, i16 %ea)
  %mb = call iW @sf_significand_W(iW %b, i16 %eb)
  %ea1 = call i16 @sf_leading_W(iW %ma, i16 %ea)
  %eb1 = call i16 @sf_leading_W(iW %mb, i16 %eb)
  %la = call iW @sf_led_W(iW %ma)
  %lb = call iW @sf_led_W(iW %mb)
  %wa = call iW @sf_shl3_W(iW %la)
  br label %loop
loop:
  %acc = phi iW [ 0, %something ], [ %acc1, %loop ]
  %rest = phi iW [ %lb, %something ], [ %rest1, %loop ]
  %i = phi i8 [ BITS, %something ], [ %i1, %loop ]
  %bit = and iW %rest, 1
  %on = icmp ne iW %bit, 0
  %part = select i1 %on, iW %wa, iW 0
  %full = add iW %acc, %part
  %lost = and iW %full, 1
  %half = lshr iW %full, 1
  %acc1 = or iW %half, %lost
  %rest1 = lshr iW %rest, 1
  %i1 = sub i8 %i, 1
  %done = icmp eq i8 %i1, 0
  br i1 %done, label %end, label %loop
end:
  %e = add i16 %ea1, %eb1
  %e1 = sub i16 %e, BIASLESS
  %r = call iW @sf_round_W(iW %s, i16 %e1, iW %acc1)
  ret iW %r
}

define iW @__divSF3(iW %a, iW %b) noinline {
entry:
  %sx = xor iW %a, %b
  %s = and iW %sx, SIGN
  %aa = and iW %a, MAGNITUDE
  %ab = and iW %b, MAGNITUDE
  %na = icmp ugt iW %aa, INF
  %nb = icmp ugt iW %ab, INF
  %nn = or i1 %na, %nb
  br i1 %nn, label %nan, label %numbers
nan:
  ret iW NAN
numbers:
  %ia = icmp eq iW %aa, INF
  %ib = icmp eq iW %ab, INF
  %za = icmp eq iW %aa, 0
  %zb = icmp eq iW %ab, 0
  %iboth = and i1 %ia, %ib
  %zboth = and i1 %za, %zb
  %none = or i1 %iboth, %zboth
  br i1 %none, label %nan, label %defined
defined:
  %toinf = or i1 %ia, %zb
  br i1 %toinf, label %infinite, label %finite
infinite:
  %inf = or iW %s, INF
  ret iW %inf
finite:
  %tozero = or i1 %za, %ib
  br i1 %tozero, label %nothing, label %something
nothing:
  ret iW %s
something:
  %ea = call i16 @sf_exponent_W(iW %a)
  %eb = call i16 @sf_exponent_W(iW %b)
  %ma = call iW @sf_significand_W(iW %a, i16 %ea)
  %mb = call iW @sf_significand_W(iW %b, i16 %eb)
  %ea1 = call i16 @sf_leading_W(iW %ma, i16 %ea)
  %eb1 = call i16 @sf_leading_W(iW %mb, i16 %eb)
  %la = call iW @sf_led_W(iW %ma)
  %lb = call iW @sf_led_W(iW %mb)
  %less = icmp ult iW %la, %lb
  %la2 = shl iW %la, 1
  %r0 = select i1 %less, iW %la2, iW %la
  %down = zext i1 %less to i16
  %e = sub i16 %ea1, %eb1
  %e1 = sub i16 %e, %down
  %e2 = add i16 %e1, BIAS
  br label %loop
loop:
  %r = phi iW [ %r0, %something ], [ %r3, %loop ]
  %q = phi iW [ 0, %something ], [ %q2, %loop ]
  %i = phi i8 [ GUARDED, %something ], [ %i1, %loop ]
  %q1 = shl iW %q, 1
  %fits = icmp uge iW %r, %lb
  %bit = zext i1 %fits to iW
  %q2 = or iW %q1, %bit
  %taken = select i1 %fits, iW %lb, iW 0
  %r2 = sub iW %r, %taken
  %r3 = shl iW %r2, 1
  %i1 = sub i8 %i, 1
  %done = icmp eq i8 %i1, 0
  br i1 %done, label %end, label %loop
end:
  %some = icmp ne iW %r3, 0
  %sticky = zext i1 %some to iW
  %m = or iW %q2, %sticky
  %n = call iW @sf_round_W(iW %s, i16 %e2, iW %m)
  ret iW %n
}

; -1, 0 or 1 as the first is less, equal or greater, and 2 when they have no
; order.
define internal i8 @sf_compare_W(iW %a, iW %b) noinline {
entry:
  %aa = and iW %a, MAGNITUDE
  %ab = and iW %b, MAGNITUDE
  %na = icmp ugt iW %aa, INF
  %nb = icmp ugt iW %ab, INF
  %nn = or i1 %na, %nb
  br i1 %nn, label %none, label %ordered
none:
  ret i8 2
ordered:
  %sa = icmp slt iW %a, 0
  %sb = icmp slt iW %b, 0
  %ma = sub iW 0, %aa
  %mb = sub iW 0, %ab
  %x = select i1 %sa, iW %ma, iW %aa
  %y = select i1 %sb, iW %mb, iW %ab
  %g = icmp sgt iW %x, %y
  %l = icmp slt iW %x, %y
  %gw = zext i1 %g to i8
  %lw = zext i1 %l to i8
  %r = sub i8 %gw, %lw
  ret i8 %r
}

; `truncate`, `floor`, `ceil` and `rint`, and the square root.
define iW @truncLF(iW %x) noinline {
entry:
  %e = call i16 @sf_exponent_W(iW %x)
  %n = sub i16 %e, BIAS
  %less = icmp slt i16 %n, 0
  br i1 %less, label %zero, label %whole
zero:
  %s = and iW %x, SIGN
  ret iW %s
whole:
  %large = icmp sge i16 %n, M
  br i1 %large, label %same, label %cut
same:
  ret iW %x
cut:
  br label %loop
loop:
  %i = phi i16 [ %n, %cut ], [ %i1, %more ]
  %mask = phi iW [ FRACTION, %cut ], [ %mask1, %more ]
  %done = icmp eq i16 %i, 0
  br i1 %done, label %end, label %more
more:
  %i1 = sub i16 %i, 1
  %mask1 = lshr iW %mask, 1
  br label %loop
end:
  %keep = xor iW %mask, -1
  %r = and iW %x, %keep
  ret iW %r
}

define iW @floorLF(iW %x) noinline {
  %t = call iW @truncLF(iW %x)
  %c = call i8 @sf_compare_W(iW %x, iW %t)
  %less = icmp eq i8 %c, -1
  br i1 %less, label %down, label %end
down:
  %r = call iW @__subSF3(iW %t, iW UNIT)
  ret iW %r
end:
  ret iW %t
}

define iW @ceilLF(iW %x) noinline {
  %t = call iW @truncLF(iW %x)
  %c = call i8 @sf_compare_W(iW %x, iW %t)
  %more = icmp eq i8 %c, 1
  br i1 %more, label %up, label %end
up:
  %r = call iW @__addSF3(iW %t, iW UNIT)
  ret iW %r
end:
  ret iW %t
}

define iW @rintLF(iW %x) noinline {
  %a = and iW %x, MAGNITUDE
  %small = icmp ult iW %a, WHOLE
  br i1 %small, label %round, label %end
round:
  %up = call iW @__addSF3(iW %a, iW WHOLE)
  %r = call iW @__subSF3(iW %up, iW WHOLE)
  %s = and iW %x, SIGN
  %n = or iW %r, %s
  ret iW %n
end:
  ret iW %x
}

define iW @sqrtLF(iW %x) noinline {
entry:
  %a = and iW %x, MAGNITUDE
  %zero = icmp eq iW %a, 0
  br i1 %zero, label %same, label %nonzero
same:
  ret iW %x
nonzero:
  %nan = icmp ugt iW %a, INF
  %neg = icmp slt iW %x, 0
  %none = or i1 %nan, %neg
  br i1 %none, label %undefined, label %positive
undefined:
  ret iW NAN
positive:
  %inf = icmp eq iW %a, INF
  br i1 %inf, label %same, label %finite
finite:
  %e = call i16 @sf_exponent_W(iW %x)
  %m = call iW @sf_significand_W(iW %x, i16 %e)
  %e1 = call i16 @sf_leading_W(iW %m, i16 %e)
  %l = call iW @sf_led_W(iW %m)
  %p = sub i16 %e1, BIAS
  %oddp = and i16 %p, 1
  %odd = icmp ne i16 %oddp, 0
  %l2 = shl iW %l, 1
  %l3 = select i1 %odd, iW %l2, iW %l
  %h = ashr i16 %p, 1
  %e2 = add i16 %h, BIAS
  %g = call iW @sf_shl3_W(iW %l3)
  %x0 = shl iW %g, 1
  br label %loop
loop:
  %rest = phi iW [ %x0, %finite ], [ %rest2, %loop ]
  %root = phi iW [ 0, %finite ], [ %root1, %loop ]
  %twice = phi iW [ 0, %finite ], [ %twice1, %loop ]
  %bit = phi iW [ TOP2, %finite ], [ %bit1, %loop ]
  %try = add iW %twice, %bit
  %fits = icmp ule iW %try, %rest
  %taken = select i1 %fits, iW %try, iW 0
  %set = select i1 %fits, iW %bit, iW 0
  %rest1 = sub iW %rest, %taken
  %root1 = add iW %root, %set
  %set2 = shl iW %set, 1
  %twice1 = add iW %twice, %set2
  %rest2 = shl iW %rest1, 1
  %bit1 = lshr iW %bit, 1
  %done = icmp eq iW %bit1, 0
  br i1 %done, label %end, label %loop
end:
  %some = icmp ne iW %rest2, 0
  %sticky = zext i1 %some to iW
  %r0 = or iW %root1, %sticky
  %r1 = and iW %r0, 1
  %r2 = lshr iW %r0, 1
  %r3 = or iW %r2, %r1
  %r = call iW @sf_round_W(iW 0, i16 %e2, iW %r3)
  ret iW %r
}

; From an integer and to one.
define iW @__floatunsiSF(i32 %n) noinline {
entry:
  %zero = icmp eq i32 %n, 0
  br i1 %zero, label %nothing, label %something
nothing:
  ret iW 0
something:
  br label %loop
loop:
  %m = phi i32 [ %n, %something ], [ %m1, %more ]
  %e = phi i16 [ BIASED31, %something ], [ %e1, %more ]
  %led = icmp slt i32 %m, 0
  br i1 %led, label %end, label %more
more:
  %m1 = shl i32 %m, 1
  %e1 = sub i16 %e, 1
  br label %loop
end:
  %s = call iW @sf_from32_W(i32 %m)
  %r = call iW @sf_round_W(iW 0, i16 %e, iW %s)
  ret iW %r
}

define iW @__floatsiSF(i32 %n) noinline {
  %neg = icmp slt i32 %n, 0
  %m = sub i32 0, %n
  %a = select i1 %neg, i32 %m, i32 %n
  %f = call iW @__floatunsiSF(i32 %a)
  %s = select i1 %neg, iW SIGN, iW 0
  %r = or iW %f, %s
  ret iW %r
}

define i32 @__fixunsSFsi(iW %x) noinline {
entry:
  %e = call i16 @sf_exponent_W(iW %x)
  %n = sub i16 %e, BIAS
  %none = icmp slt i16 %n, 0
  br i1 %none, label %nothing, label %something
nothing:
  ret i32 0
something:
  %m = call iW @sf_significand_W(iW %x, i16 %e)
  br label %loop
loop:
  %v = phi iW [ %m, %something ], [ %v1, %more ]
  %r = phi i32 [ 0, %something ], [ %r2, %more ]
  %i = phi i16 [ %n, %something ], [ %i1, %more ]
  %top = and iW %v, ONE
  %on = icmp ne iW %top, 0
  %bit = zext i1 %on to i32
  %r1 = shl i32 %r, 1
  %r2 = or i32 %r1, %bit
  %v1 = shl iW %v, 1
  %done = icmp eq i16 %i, 0
  br i1 %done, label %end, label %more
more:
  %i1 = sub i16 %i, 1
  br label %loop
end:
  ret i32 %r2
}

define i32 @__fixSFsi(iW %x) noinline {
  %a = and iW %x, MAGNITUDE
  %n = call i32 @__fixunsSFsi(iW %a)
  %neg = icmp slt iW %x, 0
  %m = sub i32 0, %n
  %r = select i1 %neg, i32 %m, i32 %n
  ret i32 %r
}
";

/// The comparison `__NAMESF2`, whose answer without an order is `UNORDERED`.
const COMPARISON: &str = "
define i16 @__NAMESF2(iW %a, iW %b) {
  %c = call i8 @sf_compare_W(iW %a, iW %b)
  %none = icmp eq i8 %c, 2
  %r = select i1 %none, i8 UNORDERED, i8 %c
  %w = sext i8 %r to i16
  ret i16 %w
}
";

/// What the widths differ in.
const BOTH: &str = "
define internal i32 @sf_from32_32(i32 %m) {
  %l = and i32 %m, 31
  %some = icmp ne i32 %l, 0
  %s = zext i1 %some to i32
  %r0 = lshr i32 %m, 5
  %r = or i32 %r0, %s
  ret i32 %r
}

; 24 bits more are below.
define internal i64 @sf_from32_64(i32 %m) {
entry:
  %w = zext i32 %m to i64
  br label %loop
loop:
  %v = phi i64 [ %w, %entry ], [ %v1, %loop ]
  %i = phi i8 [ 24, %entry ], [ %i1, %loop ]
  %v1 = shl i64 %v, 1
  %i1 = sub i8 %i, 1
  %done = icmp eq i8 %i1, 0
  br i1 %done, label %end, label %loop
end:
  ret i64 %v1
}

define i64 @__extendsfdf2(i32 %x) noinline {
entry:
  %sx = and i32 %x, -2147483648
  %s64 = zext i32 %sx to i64
  %s = shl i64 %s64, 32
  %a = and i32 %x, 2147483647
  %nan = icmp ugt i32 %a, 2139095040
  br i1 %nan, label %isnan, label %number
isnan:
  ret i64 9221120237041090560
number:
  %inf = icmp eq i32 %a, 2139095040
  br i1 %inf, label %isinf, label %finite
isinf:
  %i = or i64 %s, 9218868437227405312
  ret i64 %i
finite:
  %zero = icmp eq i32 %a, 0
  br i1 %zero, label %iszero, label %some
iszero:
  ret i64 %s
some:
  %e = call i16 @sf_exponent_32(i32 %x)
  %m = call i32 @sf_significand_32(i32 %x, i16 %e)
  %sub = icmp eq i16 %e, 0
  %e1 = select i1 %sub, i16 1, i16 %e
  %e2 = add i16 %e1, 904
  %g = call i64 @sf_from32_64(i32 %m)
  %r = call i64 @sf_round_64(i64 %s, i16 %e2, i64 %g)
  ret i64 %r
}

define i32 @__truncdfsf2(i64 %x) noinline {
entry:
  %h = lshr i64 %x, 32
  %hi = trunc i64 %h to i32
  %s = and i32 %hi, -2147483648
  %a = and i64 %x, 9223372036854775807
  %nan = icmp ugt i64 %a, 9218868437227405312
  br i1 %nan, label %isnan, label %number
isnan:
  ret i32 2143289344
number:
  %inf = icmp eq i64 %a, 9218868437227405312
  br i1 %inf, label %isinf, label %finite
isinf:
  %i = or i32 %s, 2139095040
  ret i32 %i
finite:
  %zero = icmp eq i64 %a, 0
  br i1 %zero, label %iszero, label %some
iszero:
  ret i32 %s
some:
  %e = call i16 @sf_exponent_64(i64 %x)
  %m = call i64 @sf_significand_64(i64 %x, i16 %e)
  %sub = icmp eq i16 %e, 0
  %e1 = select i1 %sub, i16 1, i16 %e
  %e2 = sub i16 %e1, 896
  br label %loop
loop:
  %v = phi i64 [ %m, %some ], [ %v1, %loop ]
  %k = phi i8 [ 26, %some ], [ %k1, %loop ]
  %lost = and i64 %v, 1
  %half = lshr i64 %v, 1
  %v1 = or i64 %half, %lost
  %k1 = sub i8 %k, 1
  %done = icmp eq i8 %k1, 0
  br i1 %done, label %end, label %loop
end:
  %t = trunc i64 %v1 to i32
  %r = call i32 @sf_round_32(i32 %s, i16 %e2, i32 %t)
  ret i32 %r
}
";

/// The functions, in a module of `ctx`.
pub(super) fn module(ctx: &'static Context) -> Result<Module<'static>, String> {
    // A buffer of text ends with a NUL.
    let buffer = MemoryBuffer::create_from_memory_range_copy(format!("{}\0", text()).as_bytes(), "soft_float");
    ctx.create_module_from_ir(buffer).map_err(|e| format!("internal error: the floating-point functions: {}", e))
}

/// The text of the functions, as LLVM's IR.
fn text() -> String {
    let mut text = BOTH.to_string();
    for (width, fraction, mode, suffix) in [(32u32, 23u32, "sf", "f"), (64, 52, "df", "")] {
        let exponent = width - 1 - fraction;
        let bias = (1u64 << (exponent - 1)) - 1;
        let infinity = ((1u64 << exponent) - 1) << fraction;
        let comparisons: String = [("eq", 1), ("ne", 1), ("lt", 1), ("le", 1), ("gt", -1), ("ge", -1)]
            .iter()
            .map(|(name, unordered)| COMPARISON.replace("NAME", name).replace("UNORDERED", &unordered.to_string()))
            .collect();
        let unordered = "
define i16 @__unordSF2(iW %a, iW %b) {
  %c = call i8 @sf_compare_W(iW %a, iW %b)
  %none = icmp eq i8 %c, 2
  %r = zext i1 %none to i16
  ret i16 %r
}
";
        let constants: [(&str, String); 19] = [
            ("truncLF", format!("trunc{}", suffix)),
            ("floorLF", format!("floor{}", suffix)),
            ("ceilLF", format!("ceil{}", suffix)),
            ("rintLF", format!("rint{}", suffix)),
            ("sqrtLF", format!("sqrt{}", suffix)),
            ("MAGNITUDE", ((1u64 << (width - 1)) - 1).to_string()),
            ("BIASLESS", (bias - 1).to_string()),
            ("BIASED31", (bias + 31).to_string()),
            ("BIAS", bias.to_string()),
            ("FRACTION", ((1u64 << fraction) - 1).to_string()),
            ("GUARDED", (fraction + 4).to_string()),
            ("WHOLE", ((bias + u64::from(fraction)) << fraction).to_string()),
            ("UNIT", (bias << fraction).to_string()),
            ("TOP2", (1u64 << (fraction + 4)).to_string()),
            ("TOP", (1u64 << (fraction + 3)).to_string()),
            ("EMAX", ((1u64 << exponent) - 1).to_string()),
            ("SIGN", if width == 32 { i32::MIN.to_string() } else { i64::MIN.to_string() }),
            ("BITS", (fraction + 1).to_string()),
            ("INF", infinity.to_string()),
        ];
        let mut functions = format!("{}{}{}", FUNCTIONS, comparisons, unordered);
        functions = functions.replace("iW", &format!("i{}", width)).replace("_W(", &format!("_{}(", width));
        functions = functions.replace("SF", mode);
        for (name, value) in constants {
            functions = functions.replace(name, &value);
        }
        functions = functions.replace("NAN", &(infinity | 1 << (fraction - 1)).to_string());
        functions = functions.replace("ONE", &(1u64 << fraction).to_string());
        functions = functions.replace("[ M,", &format!("[ {},", fraction)).replace("%n, M\n", &format!("%n, {}\n", fraction));
        text.push_str(&functions);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Numbers with every kind of exponent and fraction among them.
    fn samples<const N: usize>(width: u32, fraction: u32) -> Vec<u64> {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let exponents = (1u64 << (width - 1 - fraction)) - 1;
        let mask = (1u64 << fraction) - 1;
        let bias = exponents >> 1;
        let mut out = Vec::new();
        for sign in [0, 1u64 << (width - 1)] {
            for e in [0, 1, 2, bias - 1, bias, bias + 1, bias + u64::from(fraction), bias + u64::from(fraction) + 1, exponents - 1, exponents] {
                for f in [0, 1, mask, 1 << (fraction - 1), mask >> 1, next() & mask] {
                    out.push(sign | e << fraction | f);
                }
            }
        }
        for _ in 0..N {
            let f = next() & mask;
            // Mostly near one another, so that sums and differences are of
            // interest.
            let e = if next() % 4 == 0 { next() % exponents } else { bias - 30 + next() % 60 };
            out.push((next() & 1) << (width - 1) | e << fraction | f);
        }
        out
    }

    #[test]
    fn the_functions_answer_what_the_processor_does() {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let module = module(crate::compile::llvm_context()).unwrap();
        module.verify().unwrap();
        let engine = module.create_jit_execution_engine(inkwell::OptimizationLevel::None).unwrap();
        let addresses: HashMap<String, usize> = module
            .get_functions()
            .map(|f| f.get_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with("sf_"))
            .map(|name| (name.clone(), engine.get_function_address(&name).unwrap()))
            .collect();
        macro_rules! function {
            ($name:expr, $t:ty) => {
                unsafe { std::mem::transmute::<usize, $t>(addresses[&$name]) }
            };
        }
        macro_rules! check_width {
            ($f:ty, $bits:ty, $width:expr, $fraction:expr, $mode:expr, $suffix:expr) => {{
                let numbers: Vec<$f> = samples::<400>($width, $fraction).into_iter().map(|b| <$f>::from_bits(b as $bits)).collect();
                let same = |what: &str, a: $f, b: $f, got: $bits, want: $f| {
                    let got = <$f>::from_bits(got);
                    assert!(got.to_bits() == want.to_bits() || got.is_nan() && want.is_nan(), "{} {:e} {:e}: {:e}, not {:e}", what, a, b, got, want);
                };
                type Binary = extern "C" fn($bits, $bits) -> $bits;
                type Unary = extern "C" fn($bits) -> $bits;
                let binary: [(&str, fn($f, $f) -> $f); 4] = [("add", |a, b| a + b), ("sub", |a, b| a - b), ("mul", |a, b| a * b), ("div", |a, b| a / b)];
                for (name, want) in binary {
                    let f = function!(format!("__{}{}3", name, $mode), Binary);
                    for a in &numbers {
                        for b in &numbers {
                            same(name, *a, *b, f(a.to_bits(), b.to_bits()), want(*a, *b));
                        }
                    }
                }
                let unary: [(&str, fn($f) -> $f); 5] =
                    [("trunc", |a| a.trunc()), ("floor", |a| a.floor()), ("ceil", |a| a.ceil()), ("rint", |a| a.round_ties_even()), ("sqrt", |a| a.sqrt())];
                for (name, want) in unary {
                    let f = function!(format!("{}{}", name, $suffix), Unary);
                    for a in &numbers {
                        same(name, *a, *a, f(a.to_bits()), want(*a));
                    }
                }
                type Compare = extern "C" fn($bits, $bits) -> i16;
                let compare: [(&str, fn(i16) -> bool, fn($f, $f) -> bool); 7] = [
                    ("eq", |c| c == 0, |a, b| a == b),
                    ("ne", |c| c != 0, |a, b| a != b),
                    ("lt", |c| c < 0, |a, b| a < b),
                    ("le", |c| c <= 0, |a, b| a <= b),
                    ("gt", |c| c > 0, |a, b| a > b),
                    ("ge", |c| c >= 0, |a, b| a >= b),
                    ("unord", |c| c != 0, |a, b| a.is_nan() || b.is_nan()),
                ];
                for (name, read, want) in compare {
                    let f = function!(format!("__{}{}2", name, $mode), Compare);
                    for a in &numbers {
                        for b in &numbers {
                            assert_eq!(read(f(a.to_bits(), b.to_bits())), want(*a, *b), "{} {:e} {:e}", name, a, b);
                        }
                    }
                }
                let float = function!(format!("__floatsi{}", $mode), extern "C" fn(i32) -> $bits);
                let float_unsigned = function!(format!("__floatunsi{}", $mode), extern "C" fn(u32) -> $bits);
                let fix = function!(format!("__fix{}si", $mode), extern "C" fn($bits) -> i32);
                let fix_unsigned = function!(format!("__fixuns{}si", $mode), extern "C" fn($bits) -> u32);
                for n in numbers.iter().map(|n| n.to_bits() as u32).chain([0, 1, 255, 256, 1 << 31, u32::MAX, 16777217, 33554435]) {
                    same("float", 0.0, 0.0, float(n as i32), n as i32 as $f);
                    same("floatuns", 0.0, 0.0, float_unsigned(n), n as $f);
                }
                for a in &numbers {
                    if *a > -2147483649.0 && *a < 2147483648.0 {
                        assert_eq!(fix(a.to_bits()), *a as i32, "fix {:e}", a);
                    }
                    if *a >= 0.0 && *a < 4294967296.0 {
                        assert_eq!(fix_unsigned(a.to_bits()), *a as u32, "fixuns {:e}", a);
                    }
                }
            }};
        }
        check_width!(f32, u32, 32, 23, "sf", "f");
        check_width!(f64, u64, 64, 52, "df", "");
        let extend = function!("__extendsfdf2".to_string(), extern "C" fn(u32) -> u64);
        for a in samples::<2000>(32, 23).into_iter().map(|b| f32::from_bits(b as u32)) {
            let (got, want) = (f64::from_bits(extend(a.to_bits())), f64::from(a));
            assert!(got.to_bits() == want.to_bits() || got.is_nan() && want.is_nan(), "extend {:e}: {:e}", a, got);
        }
        let narrow = function!("__truncdfsf2".to_string(), extern "C" fn(u64) -> u32);
        let near = samples::<2000>(64, 52).into_iter().chain((0..4000).map(|i| (1023 - 160 + i % 300) << 52 | (i * 0x3_1415_9265_3589) & ((1 << 52) - 1)));
        for a in near.map(f64::from_bits) {
            let (got, want) = (f32::from_bits(narrow(a.to_bits())), a as f32);
            assert!(got.to_bits() == want.to_bits() || got.is_nan() && want.is_nan(), "narrow {:e}: {:e}, not {:e}", a, got, want);
        }
    }
}
