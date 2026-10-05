<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 수

정수, 부동소수점 수, 유리수, 복소수, 진리값의 연산과 그 밖의 수 관련 함수. 호출 형식 읽는 법은
[내장 함수](README.md)를 참고한다.

## 1. 고정 폭 정수

정수 타입은 7가지이다. **`int`**(CL의 `integer`. 임의 정밀도이며 주석 없는 정수 리터럴의 기본 타입. 3장)와 고정 폭의
`i8` `i16` `i32` `u8` `u16` `u32`. 연산이 어느 타입의 것으로 해석될지는 첫 번째 인수의 타입이 정한다(서로 독립이며 암묵적
변환은 없다). **64비트 정수 타입은 없다.** 실행 시의 값은 아래쪽 비트가 태그인 한 워드이므로 즉시값 정수에는 63비트만
남으며, 64비트를 자처하는 타입은 어딘가에서 최상위 비트를 버려야 한다. `int`는 그 63비트를 넘으면 다배정도가 되므로 폭이
상관없다면 `int`를 쓴다. 아래 표는 고정 폭 6가지 타입의 것이다(`int`의 표는 3장).

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | 사칙 연산. `/`는 0 쪽으로 버리며, 0으로 나누면 panic |
| `mod` | `(mod a b)` | `(T,T)→T` | 나머지(CL의 `mod`, **바닥 나눗셈**: 부호는 나누는 수를 따른다. `(mod -7 3)`→`2`). 0으로 나누면 panic |
| `rem` | `(rem a b)` | `(T,T)→T` | 나머지(CL의 `rem`, **버림 나눗셈**: 부호는 나뉘는 수를 따른다. `(rem -7 3)`→`-1`). 0으로 나누면 panic |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | CL의 2인수 `floor`/`ceiling`/`round`/`truncate`에 해당한다(`(floor 7 2)`→몫 3, 나머지 1). 다중 값 대신 몫과 나머지를 `cons-cell`로 반환한다(`car`=몫, `cdr`=나머지). `round-div`는 CL처럼 동률을 짝수로 반올림한다 |
| `abs` | `(abs x)` | `T→T` | 절댓값 |
| `signum` | `(signum x)` | `T→T` | 부호(`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | 최대공약수 |
| `lcm` | `(lcm a b)` | `(T,T)→T` | 최소공배수(어느 한쪽이 0이면 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | 큰 쪽 / 작은 쪽(인수 세 개 이상은 8장의 가변 인수 표기로 전개된다) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | 비교 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | 모두 `=`와 같다(같은 타입의 수에서는 차이가 없다) |
| `int->float` | `(int->float x)` | `T→f64` | `f64`로의 확대 변환 |
| `int->int` | `(int->int x)` | `T→int` | `int`로의 확대 변환(항상 정확). `(as int x)`가 하는 일 |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | `ratio`로의 확대 변환(항상 정확) |
| `int->char` | `(int->char x)` | `T→char` | 값을 Unicode 스칼라 값으로 해석한다. 잘못된 값이면 panic |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | 실패하면 `None`을 반환하는 `int->char` |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | 폭 변환. 들어가지 않는 값은 잘린다(Rust의 `as`처럼) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | 같은 변환을 질문으로. 값이 그 폭에 들어가지 않으면 `None` |

이 변환들은 특수 형식 `(as Type x)`/`(try-as Type x)`([문법 레퍼런스](../syntax.md#7-기타-특수-형식))가 하는 일이기도
하다. 비트 연산(`logand`/`ash`/`ldb` 등)과 술어(`zerop`/`evenp` 등)는 타입에 걸쳐 같은 모양이므로 11장과 9장에 모았다.

`i8` `i16` `u8` `u16` `u32`는 정확히 이 장의 표를, `f32`는 정확히 4장의 `f64` 표를 가진다.

**타입 이름은 폭과 부호 유무를 뜻할 뿐 그 이상은 아니다.** `i32`는 "32비트를 부호 있는 것으로 다룬다", `u32`는 "32비트를
부호 없는 것으로 다룬다"는 뜻이다. `(+ (the u8 200) (the u8 100))`은 `44`, `(+ 2147483647 1)`(`i32`로서)은
`-2147483648`, `(lognot (the u32 0))`은 `4294967295`이다. `f32`도 마찬가지로 진짜 binary32이다.
`(/ (the f32 1.0) (the f32 3.0))`은 `0.33333334`로 출력되며, `f64`의 결과 `0.3333333333333333`과는 다른 값이다.

파생된 CL 카탈로그(`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt`와 9장의 술어)는 `int`/`i32`/`f64`/`ratio`에 있다. 다른
폭에서 필요하면 `(as int x)` / `(as i32 x)`로 옮긴다(폭 변환은 모든 쌍에 있다).

## 2. C 경계의 원시 워드(`ptr` / `c-long` / `c-ulong`)

[`defffi`](../syntax.md#33-defffi--c-함수-선언ffi)로 선언한 C 함수와 값을 주고받기 위해서만 쓰는 세 가지 타입. `ptr`은
불투명 포인터, `c-long` / `c-ulong`은 C의 `long` / `unsigned long`이다. 값으로 만들려면 `(unsafe ...)` 안이어야 한다.

**산술은 없다.** 1장의 표는 하나도 적용되지 않는다. `(+ p 1)`도 `(< n m)`도 쓸 수 없다. 이것들은 계산하기 위한 타입이
아니라 C에 건넬 워드이므로, 계산하려면 폭이 있는 타입으로 옮긴다. `c-long` / `c-ulong`에는 변환만 있다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | 1장과 같은 폭 변환. 들어가지 않는 값은 잘린다 |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | 같은 변환을 질문으로 |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | 들어오는 길. 다른 원시 워드와 1장의 정수 타입에서 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | 위와 같음 |
| `int->int` | `(int->int x)` | `T→int` | **항상 정확**. `i32`에 들어가지 않는 `size_t`를 정직하게 읽는 방법 |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)`이 이것들이 하는 일이며, 변환은 1장의 정수 타입과의 모든
쌍에 있다. `ptr`에는 이 표조차 없다. 포인터를 수로 읽는 방법은 제공하지 않는다. 받고, 넘기고, 다른 C 함수에 건네기만
하는 값이다.

**출력할 수도 없다.** `(println "~a" x)`는 원시 워드를 받지 않으므로(`Sexpr` 표현이 없다) `(println "~a" (as int n))`처럼
먼저 폭이 있는 타입으로 옮긴다.

1장 첫머리의 "64비트 정수 타입은 없다"는 이 셋에도 성립한다. 성립하는 것은 **저장할 수 없기 때문이다**. `defstruct`의
필드, `defvar`, 타입 인수 안, `Sexpr` 안 어디에도 둘 수 없으므로 인수, 반환값, 지역 변수로 함수를 지나가기만 하는
워드이다. 자세한 내용은 [문법 레퍼런스](../syntax.md#ptr--c-long--c-ulong--원시-워드)를 참고한다.

## 3. 임의 정밀도 정수 `int`

CL의 `integer`이며 이 언어의 **정수**이다. 주석 없는 정수 리터럴은 이 타입이고, `length`나 `char->int`처럼 수를 반환하는
내장 함수는 이 타입을 반환한다. 값은 들어가는 동안 63비트 즉시값(fixnum)으로 갖고, 연산 결과가 들어가지 않게 되면
자동으로 다배정도로 승격되며, 다시 들어가게 되면 즉시값으로 돌아간다. `eq`는 fixnum 범위에서 항상 값의 동일성이고,
`eql`/`=`는 전 범위에서 수의 동일성이다. 고정 폭 정수 타입(1장)과는 다른 타입이며 암묵적 변환은 없다. `(as int x)`는 고정
폭에서의 정확한 확대이고, `(as i32 n)` / `(try-as i32 n)`은 `int`에서의 절단 / 검사이다(1장의 `int->W` / `try-int->W`와
같은 의미).

`Sexpr`의 정수 변형도 그냥 `int`이다(`(int n)`은 fixnum과 다배정도를 모두 받는다).

인덱스나 개수를 받는 내장 함수(`substring`, `Vector`의 `get`, `ash`의 시프트 양 등)는 `int`를 받지만 fixnum에 들어가지
않는 값을 넘기면 실행 시 오류이다("an integer argument does not fit a fixnum").

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | 넘치지 않는다(승격한다) |
| `/` | `(/ a b)` | `(int,int)→int` | 0 쪽으로 버린다. 0으로 나누면 panic |
| `mod` | `(mod a b)` | `(int,int)→int` | 바닥 나눗셈의 나머지(부호는 나누는 수를 따른다) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | 모두 `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | 11장과 같다(무한 비트의 2의 보수) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | 1장과 같다 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | 절단 / 검사. `W`는 6가지 폭 또는 `c-long`/`c-ulong` |
| `int->int` | | `int→int` | 항등(고정 폭과 C 워드 쪽의 `int->int`는 확대. 1장) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | 1장과 같은 모양. `expt`는 음이 아닌 지수만 받는다 |

## 4. 부동소수점 수(`f64` / `f32`)

`f32`도 같은 표를 가진다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. 0으로 나누어도 panic하지 않고 `inf`/`NaN`이 된다 |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | 바닥 나눗셈의 나머지(CL과 같다. 부호는 나누는 수를 따른다. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | 버림 나눗셈의 나머지(CL과 같다. 부호는 나뉘는 수를 따른다. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | 비교 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | 모두 `=`와 같다 |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | 거듭제곱 |
| `abs` | `(abs x)` | `f64→f64` | 절댓값 |
| `signum` | `(signum x)` | `f64→f64` | 부호(`1.0`/`-1.0`. `±0.0`/`NaN`은 그대로 반환한다. CL과 같고 Rust의 `signum`과는 다르다) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | 큰 쪽 / 작은 쪽(인수 세 개 이상은 8장의 가변 인수 표기로 전개된다) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | 단항 연산 |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | 초월 함수. `log`는 자연로그 |
| `log`(2인수) | `(log x base)` | `(f64,f64)→f64` | 밑을 지정한 로그. `(/ (log x) (log base))`로 전개된다(8장) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | CL의 2인수판에 해당한다(`(floor 7.0 2.0)`→몫 3, 나머지 1). 1장의 같은 이름의 함수와 같은 설계(`car`=몫, `cdr`=나머지) |
| `float->int` | `(float->int x)` | `f64→int` | 0 쪽으로 버려 `int`로 바꾼다(CL의 `truncate`. 크기와 관계없이 유한한 값이면 정확). 무한대와 NaN이면 panic. 고정 폭이 필요하면 `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | 정확한 2진 유리수로 `ratio`로 바꾼다(CL의 `rational`) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | 부동소수점 폭 사이의 변환. `float->f32`는 가장 가까운 값으로 반올림, `float->f64`는 항상 정확. `(as f32 x)`가 하는 일 |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | 같은 변환을 질문으로. 반올림으로 값이 바뀌면 `none`(`f64`로의 확대는 항상 `some`). `(try-as f32 x)`가 하는 일 |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CL의 같은 이름의 함수. 위의 `floor`/`ceiling`/`round`/`truncate`의 별칭이다. CL에서는 접두사 없는 것이 정수를 반환하므로 `f`가 붙은 것이 이 언어의 동작과 맞는다 |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | 각각 2 / 53 / 53(`0.0`의 정밀도만 0). `f64`는 항상 IEEE-754 binary64이므로 상수이다 |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` 또는 `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | 가수(`[1/2,1)`, 부호 없음)와 지수. CL은 세 값을 반환하지만 다중 값이 없으므로 부호는 `float-sign`에 맡긴다 |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | 같은 분해를 53비트의 정확한 정수 가수로. `가수 * 2^지수`가 정확히 원래 값이다 |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **그 부동소수점 수로 다시 읽히는 가장 간단한 유리수**(`(rationalize 0.1)`은 `1/10`). 정확한 2진 값은 `float->ratio` |

**CL과의 차이: `round`의 반올림 방식.** `round`(따라서 `fround`/`round-div`)는 **0에서 멀어지는 쪽으로** 반올림한다
(`(round 2.5)` = `3.0`). CL은 **짝수 쪽으로** 반올림해 `2`가 된다.

## 5. 유리수 `ratio`

CL 호환의 임의 정밀도 유리수. 항상 기약 분수이며 분모는 양수이고, 힙에 할당된다. 정수 타입이나 `f64`와의 암묵적 변환은
없다(명시적인 변환 메서드나 `as`/`try-as`를 쓴다). 비 리터럴의 구문은 [문법 레퍼런스](../syntax.md#1-어휘-요소)를
참고한다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | 사칙 연산(결과는 항상 기약 분수). `/`는 0으로 나누면 panic |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | 바닥 나눗셈의 나머지(CL과 같다. 부호는 나누는 수를 따른다) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | 버림 나눗셈의 나머지(CL과 같다. 부호는 나뉘는 수를 따른다) |
| `abs` | `(abs x)` | `ratio→ratio` | 절댓값 |
| `signum` | `(signum x)` | `ratio→ratio` | 부호(`1`/`-1`/`0`을 `ratio`로 반환한다) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | 거듭제곱. 지수는 정수 값의 `ratio`여야 한다(아니면 panic). 음의 지수는 역수가 된다 |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | 큰 쪽 / 작은 쪽 |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio`에는 비트 연산이 없다(CL에서도 정수 전용) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | 비교 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | 모두 `=`와 같다 |
| `numerator` | `(numerator x)` | `ratio→int` | 기약 분수의 분자(CL과 같은 이름) |
| `denominator` | `(denominator x)` | `ratio→int` | 기약 분수의 분모(항상 양수) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | 정수 부분(0 쪽으로 버림) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | `f64`로 바꾼다 |

고정 폭 정수와 `f64`에서 들어오는 길은 `int->int`/`int->ratio`(1장)와 `float->int`/`float->ratio`(4장)이다.
`int`/`ratio`는 `i32` 등과 독립된 별개의 타입이며, 섞인 산술에는 명시적 변환이 필요하다.

## 6. 복소수 `complex`

표준 라이브러리의 구조체(`defstruct`)이다.

**CL과 두 가지 다르다**(둘 다 정적 타입에서 나온다).

1. **성분은 항상 `f64`이다.** CL의 복소수는 유리수도 가질 수 있으며 `(complex 1 2)`와 `(complex 1.0 2.0)`은 다른
   타입이다. 정적 타입은 하나를 골라야 하고, 초월 함수는 부동소수점 쪽을 반환한다.
2. **`(sqrt -1.0)`은 실수의 `sqrt`(NaN)이다.** CL에서는 `sqrt`가 실수에서 복소수를 반환할 수 있지만 `f64`의 `sqrt`는
   `f64`를 반환해야 한다. 복소수 결과는 복소수 인수에서 나온다. `(sqrt (complex -1.0 0.0))`은 `i`이다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | 구성. 성분은 `z::re`/`z::im`으로 바로 읽을 수 있다 |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | 실수부와 허수부. CL처럼 **실수에도 동작한다**(`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`) |
| `conjugate` | `(conjugate z)` | `complex→complex` | 켤레(실수에도 동작한다) |
| `phase` | `(phase z)` | `complex→f64` | (-pi,pi]의 편각(실수에도 동작한다) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | 절댓값. **받는 쪽의 타입을 반환하지 않는 유일한 `abs`**(CL처럼 복소수의 절댓값은 실수) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | 복소수 산술 |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | 성분별 동등. `Eq`도 구현되어 있다(`Ord`는 없다. 복소수에는 순서가 없고 CL의 `<`도 거부한다) |
| `zerop` | `(zerop z)` | `complex→bool` | 두 성분이 모두 0인지 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt`는 주값 |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | 벡터 `(x,y)`의 각도. **CL의 2인수 `(atan y x)`는 이것의 표기이다**(2인수 `log`처럼 인수 개수로 분기한다) |

`print-object`를 구현하므로 `~a`/`~s`는 CL처럼 `#C(re im)`으로 출력한다(이 언어의 리더에는 그것을 다시 읽는 `#C` 구문이
없다).

## 7. 진리값

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | 부정 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | 모두 값의 동등을 비교한다 |

`and`/`or`는 단락 평가가 필요하므로 특수 형식이다([문법 레퍼런스](../syntax.md#4-바인딩과-조건-분기)).

## 8. 수의 보조 함수와 호출 표기

`abs`/`signum`(모든 수 타입), `gcd`/`lcm`(정수 타입만), `rem`(`f64`를 포함한 모든 실수 타입), `expt`(`int`/`f64`/`ratio`)는
각 수 타입의 메서드로 정의되어 있다(받는 쪽의 타입으로 해석된다. `(abs x)`는 `x`의 타입의 메서드). 타입별 자세한 내용은
1, 3, 4, 5장에 있다. 고정 폭 정수에는 `expt`가 없다(승격이 없어 넘치게 된다. `(as int x)`로 `int`로 옮겨 그 `expt`를
쓴다).

### 8.1 가변 인수와 0/1인수 형식

CL의 산술과 비교는 가변 인수이지만 메서드는 인수 개수가 아니라 받는 쪽의 타입으로만 해석된다. 그래서 **검사기가 다음
형식을 2인수 호출로 전개한다**.

| 쓸 수 있는 형식 | 전개 | 대상 |
|---|---|---|
| `(op a b c ...)` | 왼쪽 접기 `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | 각 항을 임시 변수에 묶은 `(and (cmp a b) (cmp b c) ...)` | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | 위 중 항등원이 있는 것 |
| `(op x)` | `+ * max min logand logior logxor`는 `x` 자신. `(- x)`는 부호 반전, `(/ x)`는 역수, `(gcd x)`/`(lcm x)`는 `(abs x)`(CL과 같다) | 위와 같음 |
| `(cmp x)` | `x`를 평가하고 `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

각 항은 왼쪽에서 오른쪽으로 정확히 한 번 평가된다(가변 인수 비교가 임시 변수를 거치는 이유). `/=`의 가변 인수 형식은
**이웃한 쌍**을 비교한다. 모든 쌍이 다른지 묻는 CL과 다르다.

### 8.2 `isqrt`와 정수 `expt`

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | 제곱근을 넘지 않는 가장 큰 정수. 음수면 panic |
| `expt` | `(expt n e)` | `(T,T)→T` | 거듭제곱(제곱 반복법). CL은 음의 지수에 유리수를 반환하지만 정수 타입은 그것을 나타낼 수 없으므로 panic한다. 먼저 `ratio`로 바꾼다 |

## 9. 술어

| 이름 | 형식 | 타입 | 대상 타입 |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32`(CL처럼 정수 타입만) |

CL의 `numberp`/`integerp`/`floatp` 같은 **타입 술어는 없다**. 정적 타입에서는 값의 타입이 실행 중에 묻지 않아도 이미 정해져
있다.

## 10. 상수

| 이름 | 타입 | 값 |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | `boole`에 넘기는 연산 코드(CL의 키워드 대신) |

수의 한계 상수(CLHS 12.1.4.2 / 12.1.3):

| 이름 | 타입 | 설명 |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | 63비트 즉시값의 상한 / 하한(2^62-1 / -2^62). 넘는 `int`는 다배정도가 된다 |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | 가장 큰 / 가장 작은 유한값 |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | 비정규화 수를 포함한 0이 아닌 가장 작은 크기 |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | 같은 것을 정규화 수로 한정한 것 |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | CL의 정의(`(/= (+ 1 e) 1)`이 되는 가장 작은 양의 `e`)를 따르므로 2^-53보다 **1 ULP 크다**. 2^-53 자체는 최근접 짝수 반올림에서 `1.0`으로 되돌아간다 |

## 11. 비트 연산

무한 비트의 2의 보수로 정의된다(CL 12.10). 고정 폭 정수 타입과 `int`에 구현되어 있고 `ratio`에는 없다(CL에서도 비트
연산은 정수 전용).

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | 비트 논리곱, 논리합, 배타적 논리합(가변 인수와 0인수판은 8.1) |
| `lognot` | `(lognot x)` | `T→T` | 비트 반전 |
| `ash` | `(ash x count)` | `(T,int)→T` | 산술 시프트. `count`가 양수면 왼쪽, 음수면 오른쪽 |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | `index`번째 비트가 서 있는지(**인수 순서는 CL과 반대**. 아래 참고) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | 서 있는 비트 수(음수면 0인 비트 수) |
| `integer-length` | `(integer-length x)` | `T→T` | 부호를 빼고 나타내는 데 필요한 비트 수 |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | 나머지 7가지. 위의 것들로 합성한다 |

**`ash`의 두 번째 인수만 `T`가 아니라 `int`이다.** 받는 쪽 타입의 값이 아니라 비트 단위의 **거리**이므로, 받는 쪽의 폭과
부호 유무는 거리에 대해 아무것도 말하지 않는다(CL의 `(ash integer count)`에서 `count`가 아무 정수인 것과 같은 이유).
부호 없는 값의 오른쪽 시프트는 논리 시프트이고(`(ash (the u8 200) -3)` = `25`), 부호 있는 값은 음의 무한대 쪽으로 반올림하는
산술 시프트이다(`(ash (the i32 -100) -4)` = `-7`). `logbitp`의 `index`가 `int`인 것도 같은 이유이다.

**바이트 지정자.** CL의 `byte`가 반환하는 불투명 객체 대신 `cons-cell<int,int>`(`car`=크기, `cdr`=위치)를 쓴다. 크기도
위치도 비트 수이므로 분해하는 정수의 폭과 관계없이 `int`이다.

**정수가 첫 번째 인수이며, CL과 순서가 다르다.** CL은 `(ldb bytespec integer)`라고 쓰지만, 이 언어는 받는 쪽(첫 번째 인수)의
타입으로 메서드를 고르므로 지정자가 앞에 있으면 정수의 타입으로 고를 수 없다. 다른 비트 연산은 모두 `(op integer ...)`의
모양이며(`(logand a b)`, `(ash x count)`, `(lognot x)`), `ldb` 계열과 `logbitp`만 반대였으므로 그것을 맞추었다. 나머지
인수는 CL의 상대 순서를 유지하므로 `(dpb newbyte spec n)`은 `(dpb n newbyte spec)`이 된다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | 바이트 지정자를 만든다 |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | 성분을 꺼낸다 |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | `x`에서 지정한 바이트를 오른쪽으로 맞춰 꺼낸다 |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | 지정한 바이트 안에 서 있는 비트가 있는지 |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | 지정한 바이트 밖을 지운다(위치는 유지) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | 오른쪽으로 맞춘 `newbyte`를 `x`의 지정한 바이트에 넣는다 |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | 위치를 유지하는 `dpb` |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | 16가지 2항 논리 연산 중 `op`(10장의 `boole-*` 상수)로 고른 것 |

`T`는 `Bits` 트레이트를 구현한 타입, 즉 `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`이다. `boole`만은 CL의 순서를 바꿀 이유가
없으므로 `op`가 앞에 있다.

## 12. 난수

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | `0` 이상 `n` 미만의 난수. 상태를 생략하면 `*random-state*`에서 뽑고 그것을 진행시킨다 |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | 인수가 없으면 새 상태, 주면 그 사본(사본은 같은 수열을 재현한다) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | 항상 `true`(정적 타입이 이미 다른 타입을 배제한다. CL에 대응시키기 위해서만 있다) |
| `*random-state*` | — | `random-state` | `random`의 기본 상태. 대입할 수 있는 전역 변수(`setf`로 바꾼다) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | 정수가 가리키는 상태. 같은 시드는 항상 같은 수열을 재현한다 |

생성기는 xorshift64이며, 인터프리트해도 컴파일해도 같은 수열을 반환한다.

`make-random-state`의 새 상태는 벽시계로 시드되므로 실행마다 재현할 수 없다. 재현하려면 `seed-random-state`를 쓴다.

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; 실행할 때마다 같은 세 수를 출력한다
```

**CL에는 시드를 주는 이식 가능한 방법이 없으므로**(`make-random-state`는 `nil`/`t`/상태만 받는다) 이 이름은 CL이 아니라
SBCL의 `sb-ext:seed-random-state`를 따랐다.

시드가 다르면 수열도 다르다. `(seed-random-state 0)`과 `(seed-random-state 1)`은 다른 수열을, `-7`과 `7`도 다른 수열을
준다.
