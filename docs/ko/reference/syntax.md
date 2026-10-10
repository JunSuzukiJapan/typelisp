<!-- translated-from: docs/ja/reference/syntax.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# typelisp 문법 레퍼런스

typelisp는 S 식으로 쓰는 정적 타입 Lisp이다. 내장 함수와 메서드의 목록은 [내장 함수](functions/README.md), 타입 목록은
[types.md](types.md), 오류 메시지 읽는 법은 [errors.md](errors.md)를 참고한다.

## 1. 어휘 요소

- **대소문자를 구별하지 않는다.** 심볼은 읽을 때 모두 소문자로 정규화된다.
- **주석**: `;`부터 줄 끝까지(줄 주석). `#| ... |#`(중첩할 수 있는 블록 주석).
- **읽기 시 평가**: `#.(식)`은 **읽는 동안** 뒤의 형식을 **실행하고** 그 값을 읽은 것으로 다룬다. 리더가 텍스트의 함수
  이상이 되는 유일한 곳이다. 어디까지 닿을 수 있는지는 CL처럼 읽기 경로에 따라 다르다.
  - `(load ...)`와 REPL은 형식을 하나씩 평가하므로 **같은 텍스트 안에서 앞서 정의한 함수**를 호출할 수 있다(CL의 `load`).
  - 모듈 파일은 하나의 단위로 검사되고 실행은 그것을 `use`하는 쪽이 하므로, `#.`이 닿을 수 있는 것은 표준 라이브러리와
    세션이 이미 실행한 것뿐이다. 파일 자신의 정의도, 그것이 `use`하는 모듈의 정의도 **아직 실행되지 않았다**(CL의
    `compile-file`에 `eval-when`이 필요한 것과 같다).
  - 프로그램 안의 `read` / `read-from-string`도 `#.`을 평가한다(CL과 같다).
  - `*read-eval*`(기본값 `true`)을 `false`로 하면 `#.`은 어디서나 읽기 오류가 된다. 데이터로 읽는 텍스트가 코드를 실행하지
    못하게 하는 스위치이다(CL과 같다). `#.`을 만날 때마다 참조하므로 `setf`는 다음에 읽는 형식부터 효과가 있다.
    `with-standard-io-syntax` 안에서는 `true`이다.
- **진리값**: `true` / `false`.
- **정수**: 10진수(`42`, `-7`). 앞에 부호 `+`/`-`를 붙일 수 있다. 10진수 이외는 CL의 기수 구문 `#b`/`#o`/`#x`/`#NNr`로
  쓴다(부호는 표시 뒤: `#x-ff`). `0x` 접두사는 CL에 없으므로 채택하지 않는다. `0xff`는 심볼로 읽힌다.
  타입 주석이 없는 정수 리터럴의 기본은 `int`이다(임의 정밀도, [수](functions/numbers.md#3-임의-정밀도-정수-int)). 크기에
  상한이 없다. **기대 타입이 고정 폭 정수 타입이면 리터럴은 그 타입이 되고, 그 타입이 값을 담을 수 있는지 검사된다.**
  `(the u8 300)`은 타입 오류이다(잘라 내려면 `(as u8 300)`이라고 쓴다). `(the u32 4294967295)`와 `(the u32 #xFFFFFFFF)`를
  쓸 수 있는 것은 이 규칙 덕분이다. `int`의 값이 63비트 즉시값에 들어갈지 다배정도가 될지는 값의 크기가 정하며 전용 구문은
  없다(CL과 같다).
- **부동소수점 수**: 소수점이나 지수(`e`/`E`)를 포함하는 수(`1.5`, `3.0e10`). 기본은 `f64`(기대 타입이 `f32`이면 그 타입).
- **비(ratio)**: `분자/분모`(10진수만. 예: `1/3`). CL이 정한 대로 읽을 때 약분된다(`2/4`는 `1/2`). 값이 정수인 것(`4/2` 등)은
  `ratio`가 아니라 `int`로 읽힌다. 분모가 0(`1/0`)이면 읽기 오류이다.
- **문자**: `#\` 뒤에 한 글자 또는 문자 이름. 예: `#\a` `#\Space` `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul`(`#\Null`도
  된다) `#\Backspace`. 이름은 대소문자를 구별하지 않는다.
- **문자열**: `"..."`. 이스케이프는 `\n` `\t` `\r` `\0` `\\` `\"`(그 밖의 `\x`는 그냥 `x`).
- **심볼**: 영숫자와 기호를 포함하는 아무 토큰(`+` `<=` `my-func` 등).
  `]`와 `}`는 토큰을 끝내므로 심볼 안에 쓸 수 없고, 데이터의 시작에 나오면 읽기 오류다. `[`와 `{`는 심볼 안에 쓸 수 있다. CL과 마찬가지로 프로그래머가 [리더
  매크로](#11-리더-매크로readtable)에 쓸 수 있도록 비워 두었다.
- **키워드**: `:name`처럼 콜론으로 시작하는 심볼(CL과 같다). 자기 평가된다. 바인딩을 찾지 않고 값은 자기 자신이며 정적
  타입은 `symbol`이다. 같은 이름의 키워드는 항상 같은 객체이다(`(eq :foo :FOO)`는 참. 다른 심볼처럼 소문자가 된다). 콜론
  자체가 이름의 일부이므로 `(symbol->string :foo)`는 `":foo"`이다(typelisp에는 패키지 체계가 없으므로 CL의 `symbol-name`과
  다르다). 홀로 있는 `:`나 `:a:b`처럼 콜론이 더 있는 것은 읽기 오류이다. `keywordp`로 판정한다. `::`로 시작하는 것은
  키워드가 아니라 절대 경로이다(아래).
  또한 `:dyn`은 타입 위치 전용의 예약 키워드이며, 다른 곳에 쓰면 오류이다([2장](#2-타입-표기) 참고).
- **리스트**: `(a b c)`. 점 쌍 `(a . b)`도 읽을 수 있다.
- **벡터**: `#(1 2 3)`(CL과 같다). 내용은 모두 리터럴이며 평가하지 않는다. `#(a b)`의 `a`는 변수가 아니라 심볼이다. 요소의 타입은 문맥에서
  정해지고(`(the Vector<i32> #(1 2))`), 문맥이 없으면 첫 요소의 타입이 된다(`#(1 2 3)`은 `Vector<int>`). 요소의 타입은 모두 같아야
  하며 `#(1 "a")`는 타입 오류다. 요소도 문맥도 없는 `#()`도 타입 오류다. 평가할 때마다 새 벡터가 만들어진다. S 식 데이터가 기대되는
  곳(`(the Option<Sexpr> #(1 x))`, `'#(..)`, `read`로 읽은 것)에서는 요소가 모두 데이터인 `Vector<Option<Sexpr>>`, 곧
  `Sexpr`의 `vector` 변형이 된다.
- **배열**: `#2A((1 2) (3 4))`(CL과 같다). `#`와 `A` 사이의 수가 차원 수이고, 내용 리스트의 중첩 중 처음 그 수만큼의 단이 각 차원이 된다.
  `#0A x`는 요소 하나를 가진 0차원 배열이다. 같은 단의 리스트 길이가 다르면 읽기 오류다. 타입은 벡터와 같은 방식으로 정해져 `Array<T>`가 된다(요소가 없으면
  `(the Array<f64> #2A(()))`처럼 문맥이 필요하다). S 식 데이터로서는 `Array<Option<Sexpr>>`, 곧 `Sexpr`의 `array` 변형이
  된다.
- **튜플**: `#{"foo" 123 45.6}`. 내용은 모두 리터럴이며 평가하지 않는다(`#{a b}`의 `a`는 기호). 각 요소의 타입은 위치마다 문맥으로
  정해지고(`(the #{i32 f64} #{1 2.0})`), 문맥이 없으면 각 요소 자신의 타입이 된다(`#{"foo" 123}`는 `#{string int}`). 요소는
  1~12개. 평가할 때마다 새 튜플이 만들어진다. 계산한 값으로 만들려면 `(tuple a b)`를 쓴다. S 식 데이터가 기대되는 곳에서는 요소가 모두 데이터인 튜플, 곧
  `Sexpr`의 `tuple` 변형이 된다.
- **빈 리스트 `()`**: 문맥에 따라 `Unit` 타입의 값이거나 `Option<Sexpr>`의 `none`이다. **`Sexpr`에는 빈 리스트 변형이
  없다.** `Sexpr`는 "비어 있지 않은 S 식"을 뜻하며, S 식 데이터의 타입은 `Option<Sexpr>`이다([4.3 match](#43-match--패턴-매칭)의
  "`Option<Sexpr>`의 패턴" 참고).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)`(quasiquote 안에서만 의미가 있다)
  - `,@x` → `(unquote-splicing x)`(전개할 때 리스트 요소로 펼쳐 넣는다)
- **경로 `::`**: `foo::bar`는 모듈, 타입, 멤버를 지나는 경로로 읽힌다(하나의 심볼 이름이 되지 않는다). `::foo`처럼 `::`로
  시작하면 루트부터의 절대 경로이다. 제네릭 인수 안의 `::`(`Vec<a::b>` 등)는 경로 구분자로 다루지 않는다.

## 2. 타입 표기

소스에서 타입은 보통의 심볼이나 리스트로 쓴다.

- **기본 타입**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string` `symbol`.
  `int`는 정수(CL의 integer. 63비트 즉시값과 다배정도 사이를 자동으로 오간다. [수](functions/numbers.md#3-임의-정밀도-정수-int))이고,
  고정 폭 6가지 타입은 폭과 부호 유무로 이름 지어져 있다(64비트 정수 타입은 없다.
  [수](functions/numbers.md#1-고정-폭-정수) 참고).
- **유리수 타입**: `ratio`(기약 분수인 유리수). CL처럼 힙에 할당되며 `int`/`f64` 등과의 암묵적 변환은 없다(`as`/`try-as`나
  변환 메서드로 명시적으로 변환한다. [수](functions/numbers.md#5-유리수-ratio) 참고).
- **C 경계의 원시 워드**: `ptr`(불투명 포인터), `c-long` / `c-ulong`. FFI 전용이며, 값으로 만들려면 `(unsafe ...)`가 필요하고
  나올 수 있는 곳도 제한된다([3.3 defffi](#ptr--c-long--c-ulong--원시-워드)). 64비트 정수를 원하는 곳에 쓰지 않는다. 산술이
  없다.
- **불투명한 가변 타입**: `random-state`(난수 생성기의 상태). `Vector<T>`/`HashTable<K,V>`/`Sexpr`에 넣을 수 없다
  (`Option<T>`/`Result<T,E>`에는 넣을 수 있다).
- **Unit 타입**: `()`
- **Never 타입**: `!`(`panic`/`unreachable`/`todo`/반환하지 않는 루프 같은 발산하는 식의 타입. 어떤 기대 타입에도 맞는다)
- **함수 타입**: `(fn (인수-타입...) 반환-타입)`. 가변 인수를 가진 함수의 타입은
  `(fn (인수-타입... &rest 요소-타입) 반환-타입)`이다.
- **제네릭 타입**: `Name<T1,T2,...>`. 예: `Option<i32>` `Result<i32,ParseIntError>`
  `HashTable<string,i32>` `Vector<T>`. 이름 바로 뒤의 `<`에 타입이 이어지면, 리더는 대응하는 `>`까지를 타입 인수로 읽는다. 타입 인수 안은
  공백이나 줄바꿈으로 구분해도 되고(`HashTable<string, int>`), unit 타입 `()`(`Result<(), FileError>`), `:dyn Trait`,
  튜플 타입도 쓸 수 있다. `<` 바로 뒤에 타입이 오지 않으면 그 `<`는 이름의 일부가 된다(`<=`나 `string<`). 타입 인수가 `>`로 닫히지 않거나 타입 모양이
  아니면 읽기 오류다(`(a<b c)`도 타입 인수 `b c`의 오류로 읽기 오류). `()`는 필드 타입이나 인수 타입으로도 쓸 수 있다.
- **제네릭 타입의 적용 형식**: `(Name T1 T2 ...)` — `Name<T1,T2,...>`와 같은 타입을 가리키는 리스트 표기. 예를 들어
  `(vector char)`는 `Vector<char>`와 같다. 이름 형식이 보통의 표기이며, 이 형식은 **타입 인수를 이름 안에 쓸 수 없을 때를 위해 있다**. 타입
  인수 자체는 타입 식이지만 `<..>` 안에는 이름, `()`, `:dyn`, 튜플 타입만 쓸 수 있고 함수 타입은 쓸 수 없다(`Vector<(fn (i32) i32)>`는
  읽기 오류. `deftype`으로 이름을 붙이면 `Vector<F>`로 쓸 수 있다). 트레이트의 연관 타입을 시그니처에 대입한 결과처럼, 구현이 타입을 보일 때 이 형식으로
  나오기도 한다.
- **튜플 타입**: `#{int string}`(값과 같은 모양). 요소는 1~12개이며 제네릭 타입의 인수로도 쓸 수 있다(`Vector<#{int string}>`).
  요소는 `t::0` `t::1`로 읽고 `(setf t::0 v)`로 바꾼다. `Eq`, `Ord`(첫 요소부터 차례로 비교), `Hash`, `print-object`는 모든
  요소의 타입이 그것을 구현하면 쓸 수 있다.
- **한정된 타입 이름**: `module::Type`처럼 `::`로 한정할 수 있다.
- **트레이트 객체 타입**: `:dyn Trait`(공백으로 구분된 두 단어가 하나의 타입). 구체적인 타입이 실행 중에 정해지는 값을
  나타내며, 트레이트 메서드 호출은 vtable을 거친다(동적 디스패치). 연관 타입을 가진 트레이트는 선언 순서대로 위치로
  고정한다(`:dyn Iter<i32>`는 `Item`을 `i32`로 고정한다). 제네릭 인수 안에도 쓸 수 있다: `Vector<:dyn Drawable>`
  `HashTable<string, :dyn Drawable>`. 구체 값은 기대 위치에서 자동으로 상자에 담기며, 명시적인 형식은 `(as :dyn Trait 식)`이다.
  `:dyn Sub`의 값은 그 상위 트레이트(전이적으로 상속하는 모든 것) 중 어느 `:dyn Super`가 요구되는 곳에도 그대로 넘길 수
  있다(업캐스트). 상속 관계가 없는 트레이트에는 넘길 수 없다. `:dyn`이 될 수 있는 트레이트의 조건은
  [3.9 deftrait / impl](#39-deftrait--impl--트레이트-기구)을 참고한다. 타입 위치 밖에 `:dyn`을 쓰면 오류이다.
- 내장 제네릭 타입: `Option<T>`(`Some(T)` / `None`), `Result<T,E>`(`Ok(T)` / `Err(E)`), `HashTable<K,V>`, `Vector<T>`, 그리고
  동시성의 `Task<T>` / `Thread<T>` / `Chan<T>`([12장](#12-동시성태스크)). S 식 데이터의 타입 `Sexpr`도 있다. 내장 구체 오류
  타입은 `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` / `FileError` / `NetError`이고, 표준 라이브러리에는
  구조체 `SimpleError` / `WrappedError`가 있다(`Error`는 타입이 아니라 트레이트이다. `:dyn Error`로 쓴다). 목록은
  [types.md](types.md)에 있다.
- **타입과 트레이트는 하나의 이름공간을 공유한다**(Rust와 같다). 하나의 모듈 안에서 타입(`defstruct`/`defenum`)과
  트레이트(`deftrait`)는 같은 이름을 가질 수 없다.

## 3. 최상위 정의

### 3.1 defun — 함수 정의

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- 인수 타입과 반환 타입은 필수이다.
- 제네릭 함수는 이름 뒤 꺾쇠괄호에 타입 매개변수를 쓴다: `(defun name<T1,T2...> (params) Ret body...)`(타입 위치의
  `Vector<T>`와 같은 꺾쇠괄호 구문).
- `defun`/`lambda`/`defmethod`는 끝에 `&rest (name Type)`을 쓰면 가변 인수를 받는다:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)`(본체에서 `xs`는 항상 S 식 리스트인 `Option<Sexpr>`로 묶인다. 호출
  지점의 각 실제 인수는 하나하나 `Type2`로 타입 검사된 뒤 `Sexpr`에 감싸진다).
  `defmacro`에도 자체 `&rest`가 있지만 항상 타입 없는 `Sexpr`라는 점이 다르다(`defun`/`lambda`는 요소 타입을 명시한다).
  함수 타입도 `(fn (T1... &rest Te) Ret)` 형태로 가변 인수 함수를 나타낼 수 있다.
- **`&optional` / `&key`**(`defun`과 `defmethod`용. `lambda`/`labels`는 아래의 이유로 대상이 아니며, `defmacro`는 아래에
  설명하는 다른 구현). 순서는 CL의 `필수 &optional &rest &key`이다. 각 매개변수는 `(name Type)` 또는
  `(name Type 기본값-식)`으로 쓴다.

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; 기본값 없음
    (match suffix ((some s) (append name s)) ((none) name)))         ; 본체에서는 Option<string>

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; 기본값 있음
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; 호출하는 쪽은 `:name 값`을 순서 없이 쓴다. 생략한 것은 기본값
  ```

  - **기본값 식이 없는 매개변수의 타입은 `Option<Type>`이다.** 생략하면 `none`, 넘기면 호출하는 쪽이 쓴 맨 값이 자동으로
    `some`에 감싸진다. CL이 supplied-p 변수("주어졌는가")로 하는 일이 여기서는 정적 타입 쪽에 나타난다.
  - 기본값 식이 있으면 타입은 선언한 `Type` 그대로이다. 생략하면 그 **검사된 식**이 호출 지점에 그대로 들어간다(호출할
    때마다 평가된다).
  - **`&key`는 하나의 인수 목록에서 `&optional`/`&rest`와 섞을 수 없다.** CL 자체에 있는 모호함(끝의 실제 인수를 위치의
    `&optional`이 받을지 레이블로 맞추는 `&key`가 받을지가 *값*에 따라 달라진다)을 그 조합을 금지해서 피한다.
    `&optional`과 `&rest`는 함께 쓸 수 있다.
  - 제네릭 함수에서도 쓸 수 있지만 **생략된 인수에만 나오는 타입 매개변수는 추론할 수 없어 오류이다**(맞춰 볼 값이 없다).
  - **`defmethod`도 같은 세 구역을 쓸 수 있다**(인스턴스 메서드와 정적 함수 모두). 받는 쪽 뒤에 `&optional`/`&rest`/`&key`를
    늘어놓는다.

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; 정적 함수
    (point::origin :y 7)
    ```

    제네릭 타입의 메서드에서도 쓸 수 있지만 **기본값 식을 가진 매개변수의 타입에 소유자의 타입 매개변수를 쓸 수 없다**
    (`defun`이 자기 타입 매개변수에 대해 가지는 것과 같은 제한. 생략했을 때 들어가는 것은 *검사된* 식이므로 그 타입을
    추상적인 변수로 남겨 둘 수 없다).
  - **트레이트 메서드에서는 쓸 수 없다.** `deftrait`에는 그 구문이 없으며, `impl` 쪽만 구역을 선언할 수 있다면 `:dyn`을
    받는 쪽으로 하는 호출(트레이트의 선언에서 인수를 채운다)과 구체 타입을 받는 쪽으로 하는 호출(`impl`의 선언에서
    채운다)이 서로 다른 것이 되어 버린다. vtable 슬롯의 인수 개수는 고정이다.
  - **`lambda` / `labels`에서는 쓸 수 없다**(`&rest`는 쓸 수 있다). 생략한 인수를 채우려면 호출하는 쪽이 **호출되는 쪽의
    검사된 기본값 식**을 읽어야 하는데, 그것은 이름으로 해석되는 시그니처에서만 얻을 수 있다. `lambda`는 값으로 넘겨지며
    그 값을 기술하는 것은 함수 타입 `(fn ...)`뿐이다. 거기에는 식을 둘 곳이 없고, 둔다면 "시그니처가 같고 기본값만 다른
    두 lambda"가 다른 타입이 되어 버린다. `&rest`는 타입의 문제에 머무르므로 함수 타입에 쓸 수 있다.
- **전방 참조는 `defsignature`로 선언한다**(아래). 선언하지 않은 이름은 정의보다 앞에서 호출할 수 없다. 최상위는 소스
  순서대로 형식 하나씩 검사되고 실행되기 때문이다.
- 트레이트 경계를 요구하려면 본체 바로 앞에 `where` 절을 쓴다:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (`(AssocName ConcreteType)`으로 연관 타입을 고정하는 것은 생략할 수 있다).
- **문서 문자열**: `where` 절(있다면) 바로 뒤, 본체 맨 앞의 문자열 리터럴은 문서 문자열이 된다(CL과 같다). 다만 그 뒤에
  본체 형식이 하나 이상 있을 때만이다. 홀로 있는 문자열은 반환값으로 남고 문서 문자열로 보지 않는다:
  `(defun f () string "doc" "value")`는 문서 문자열을 가지고 `"value"`를 반환하며, `(defun f () string "value")`는 문서
  문자열 없이 `"value"`를 반환한다. `(documentation name)`으로 꺼낼 수 있다
  ([문서 문자열](functions/system.md#7-문서-문자열--documentation)).

### 3.2 defsignature — 전방 선언

```lisp
(defsignature name (인수-타입...) 반환-타입)
(pub defsignature name (인수-타입...) 반환-타입)
```

자신보다 **나중에** 정의되는 `defun`을 호출하려면 먼저 이렇게 선언한다. 상호 재귀는 이 방법으로만 쓸 수 있다.

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

인수는 **타입만** 늘어놓는다. 본체가 없으므로 이름을 붙일 대상이 없다. `&rest`는 마지막에 `&rest 요소-타입`으로 쓸 수 있다.

선언은 **검사된다**.

- 뒤따르는 정의는 선언과 맞아야 한다(인수의 개수와 타입, 반환 타입, `&rest`, `pub` 여부). 맞지 않으면 정의에서 오류이다.
- 선언하고 정의하지 않으면 오류이다(파일 / 모듈을 다 읽었을 때 보고된다). REPL은 입력할 때마다 보고하지 않는다. 선언과
  정의를 다른 줄에 입력할 수 있어야 하기 때문이다.
- 정의 **뒤에** 둔 선언은 아무것도 할 수 없으므로 오류이다.

선언할 수 없는 것이 세 가지 있다.

- **제네릭 함수.** 타입마다의 사본을 만들려면 본체가 필요한데 선언에는 본체가 없다. 전방 호출은 해석되더라도 인스턴스화가
  실패하므로 선언 시점에 거절한다.
- **`&optional`/`&key`.** 그 시그니처에는 각 기본값의 **검사된** 식이 포함되며(인수를 생략하면 호출 지점에 들어간다) 선언에는
  그것을 둘 곳이 없다.
- **`defun` 이외의 것.** `defmacro`는 전개하려면 매크로 본체가 **이미 실행되어** 있어야 하며, 시그니처 등록으로 대신할 수
  없다. 타입(`defstruct`/`defenum`/`deftrait`)의 등록은 "타입을 등록하는 코드 자체가 필요로 하는 것"이라 시그니처처럼
  자기 완결적이지 않다. `defmethod`는 소유하는 타입에 등록되므로 타입을 따른다.

CL에서 대응하는 것은 `(declaim (ftype (function (i32) bool) even2))`이지만, 그것은 선언 체계 전체를 동반하며 **권고**에
지나지 않는다. 여기서는 정적 타입이므로 선언이 검사된다.

### 3.3 defffi — C 함수 선언(FFI)

```lisp
(defffi (이름 "c_symbol") (인수-타입...) 반환-타입)
(defffi (이름 "c_symbol") (인수-타입...) 반환-타입 :library "이름")
(defffi 이름 (인수-타입...) 반환-타입)              ; 이름 = C의 심볼 이름
(pub defffi ...)
```

C 함수를 호출할 수 있도록 선언한다. 형식은 `defsignature`와 같지만(이름, 인수 타입, 반환 타입, 본체 없음) 본체가 없는 것의
의미가 다르다. `defsignature`는 "나중에 스스로 정의한다"는 약속이고, `defffi`는 "본체는 이미 다른 누군가가 쓰고
컴파일했다"는 선언이다.

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

typelisp 쪽 이름과 C의 심볼 이름을 따로 쓸 수 있는 것은 typelisp의 식별자에는 보통 `-`가 들어가는데 C의 식별자에는 들어갈
수 없기 때문이다. C의 이름을 생략하면 이름이 그대로 C의 심볼 이름이 된다.

**호출에는 `(unsafe ...)`가 필요하다**(스칼라만 다루는 함수라도). 선언한 C 시그니처가 실제와 맞는지 컴파일러는 확인할 방법이
없고 선언을 믿을 수밖에 없다. `unsafe`는 그 책임을 떠맡는다는 표시이다. 의도된 쓰는 법은 한 번만 감싸 안전한 래퍼를 만드는
것이다.

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; 이후로는 unsafe가 필요 없다
```

쓸 수 있는 타입은 `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()`(void) `string` `ptr` `c-long` `c-ulong`,
그리고 타입 있는 포인터 `(ptr T)`이다([뒤에서](#def-c-struct와-타입-있는-포인터--c-구조체-배치)).

`string`은 `const char *`이다. typelisp의 문자열은 NUL로 끝나지 않고 그 자체에 NUL이 들어갈 수도 있으므로 **넘길 때 C
문자열로 복사되고** 호출이 끝나면 해제된다. 문자열 안에 NUL이 있으면 오류이다. C는 그 앞까지만 보므로 조용히 다른 문자열을
넘기게 되기 때문이다.

**반환할 때도 복사되며** 해제하지 않는다. C가 반환한 것은 C의 것이며, `getenv`처럼 정적인 표를 가리킬 수도 있다. 호출한
쪽이 해제해야 하는 메모리를 반환하는 함수(`strdup` 등)는 `ptr`로 받아 직접 해제한다.

결과가 인수 안을 가리키는 함수(`strchr`, `strstr`)도 올바르게 동작한다. 인수를 해제하기 전에 결과를 복사하기 때문이다.

`string`을 반환한다고 선언한 함수가 NULL을 반환하면 오류이다. `string`에는 "없었다"를 뜻하는 값이 없기 때문이다. NULL이
될 수 있다면 `ptr`로 받는다.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

`:library`를 쓰면 그 공유 라이브러리를 열고 그 안에서 심볼을 찾는다. 생략하면 **프로세스 자신**(이미 링크된 모든 것. libc
포함)에서 찾는다. `sqlite3` 같은 짧은 이름은 `libsqlite3.dylib` / `libsqlite3.so` 순서로 찾고, `/`가 들어 있으면 경로로
다룬다. 연 라이브러리는 닫지 않는다. 그 안의 함수를 가리키는 코드가 계속 돌기 때문에 올바른 수명은 프로세스의 수명뿐이다.

#### ptr / c-long / c-ulong — 원시 워드

`ptr`은 불투명 포인터(`void *`, `FILE *`, 또는 선언이 뜻한 무엇이든)이다. `c-long` / `c-ulong`은 C의 `long` /
`unsigned long`이다(`size_t`, `int64_t`, `intptr_t`도).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**`i64` / `u64`라고 부르지 않는 것은 의도된 것이다.** 이 언어에는 64비트 정수 타입이 없다. 태그 붙은 즉시값은 63비트뿐이기
때문이다([2장](#2-타입-표기)). `c-long`이라는 이름은 "이것은 이 언어의 정수가 아니라 C와의 경계를 넘는 워드"라고 말한다.

**산술은 없다.** `(+ x 1)`은 쓸 수 없다. 제공할 수 있지만 제공하지 않는 것은, 어디에도 저장할 수 없고 폭이 다른 모든 수와
다른 값에 계산이 돌지 않게 하기 위해서이다. 64비트 정수 타입을 뺀 것과 같은 이유이다. 있는 것은 **변환뿐이다**.

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; 반환된 것을 읽는다
(as int (unsafe (c-strlen s)))               ; 정확히 읽으려면 이것(int는 64비트를 잃지 않는다)
(try-as i32 (unsafe (c-strlen s)))           ; 들어가는지 묻는다
(as c-ulong n)                               ; 다른 정수에서 만든다
```

정수 **리터럴**은 기대 타입을 따르므로 넘기기만 할 때는 `as`가 필요 없다.

```lisp
(unsafe (c-malloc 16))                       ; 16은 c-ulong으로 읽힌다
```

범위를 벗어난 리터럴은 다른 폭처럼 거절된다(`(c-malloc -1)`은 `c-ulong`에 들어가지 않는다).

**나올 수 있는 곳이 제한된다.** 인수 타입, 반환 타입, 지역 변수뿐이다. 다음은 모두 오류이다.

```lisp
(defstruct handle (p ptr))          ; 구조체의 필드
(defenum maybe (none) (some ptr))   ; 열거형의 필드
(defvar (block ptr) ...)            ; 전역 변수
(defffi f ((vector ptr)) i32)       ; 타입 인수 안
```

이유는 하나이다. 모두 **슬롯이 담는 것에 태그를 붙이는** 곳이다. 태그를 붙이면 포인터의 최상위 비트가 사라진다. 64비트 정수
타입을 뺀 것과 같은 이유이므로 `unsafe` 안에서도 허용하지 않는다. 허가의 문제가 아니라 그 표현이 존재하지 않는 것이다.

같은 이유로 중첩된 함수에 **포착되는** 지역 변수가 될 수도 없다(포착된 바인딩은 셀에 들어가고, 셀은 담는 것에 태그를
붙인다). 이것은 컴파일 시에 알 수 있으며 `(compile f)`가 보고한다.

GC는 `ptr`를 추적하지 않는다. 힙 밖을 가리키므로 그것이 옳다.

선언할 수 없는 것이 네 가지 있다.

- **가변 인수**(`printf`). 가변 부분은 고정 인수와 다른 규칙으로 넘겨지므로(AArch64 Darwin에서는 스택) 고정된
  시그니처에서는 올바르게 호출할 수 없다. `&rest`는 거절된다.
- **구조체를 값으로 넘기거나 반환하기.** 같은 이유이다(플랫폼마다의 호출 규약에 따라 다르다). 쓸 수 있는 타입을 위의
  목록으로 한정했으므로 쓸 수 없다.
- **제네릭.** C에 대응하는 것이 없다.
- **내장과 같은 이름.** 컴파일된 호출은 그 이름을 내장 함수로 해석하므로 조용히 잘못되는 대신 거절한다.

#### 콜백 — C에서 다시 호출하게 하기

인수 타입에 함수 타입 `(fn (타입...) 반환-타입)`을 쓰면 그 인수는 C가 다시 호출하는 함수(콜백)가 된다.

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; 최상위 함수
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; 지역 함수
```

C의 함수 포인터는 코드의 주소일 뿐이며, C는 선언한 인수만 넘겨 호출한다. 포착한 변수를 넘길 곳이 없으므로 **자유 변수가
없는 함수만 넘길 수 있고**, 이것은 타입 검사 때 검사된다.

- 실제 인수에는 함수 이름이나 `lambda` 식을 **직접** 쓴다. 함수를 담은 변수는 넘길 수 없다. 그 안에 어느 함수가 있는지,
  따라서 자유 변수가 있는지는 실행할 때까지 알 수 없다.
- `lambda`가 바깥의 지역 변수를 참조하면 오류이다. 전역 변수와 최상위 함수는 참조해도 된다.
- 지역 함수(`labels`)는 그것이 호출하는 형제 함수까지 포함해 자유 변수가 없어야 한다. 형제 함수는 포착한 변수를 두는
  곳을 공유하므로 호출되는 형제가 포착한 것은 이 함수도 포착한 것이다.
- 제네릭 함수의 타입은 선언한 함수 타입에서 정해진다.
- 함수 타입에 쓸 수 있는 타입은 위의 목록과 같다. 다만 콜백의 반환 타입에 `string`은 쓸 수 없다(아무도 해제하지 않는
  메모리를 C에 건네게 된다). `string` 인수는 C가 넘긴 문자열을 typelisp의 문자열로 복사한다.

C 함수 호출은 `unsafe` 안에서만 쓸 수 있으므로 콜백도 `unsafe` 안에서만 넘길 수 있다.

**콜백을 호출할 수 있는 것은 typelisp가 호출한 C 함수가 실행되는 동안뿐이다.** 그 밖의 곳(typelisp를 실행하지 않는
스레드, 시그널 핸들러, `atexit`로 등록한 함수)에서 호출되면 이유를 출력하고 프로세스를 멈춘다.

**실패는 C를 넘어 전파되지 않는다.** 콜백 안의 `panic`이나 `throw`는 C의 프레임을 넘어 되감을 수 없으므로(정의되지 않은
동작이 된다) C에는 0을 반환하고, C 함수가 반환될 때 호출한 쪽에 다시 던진다. 실패에서 C 함수가 반환될 때까지 사이에 다시
호출되면 실행하지 않고 0을 반환한다.

콜백 안에서 기다려야 하는 연산(빈 채널의 `recv` 등)은 오류이다([12.6](#126-컴파일된-코드와-태스크)).

함수를 다시 정의하면 다음에 C에 넘길 때부터 새 정의가 호출된다.

AOT(`compile-file`)에서도 똑같이 동작한다. C가 호출하는 진입점은 실행 파일에 들어간다.

**값으로 넘길 수 없다.** `(map f xs)`의 `f`에 FFI 선언을 그대로 쓸 수 없다. 함수 값은 정의의 본체를 감싼 클로저인데 이
선언에는 감쌀 본체가 없다. `lambda`로 감싼다.

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)`도 거절된다. 보일 수 있는 것은 C의 기계어이며 이 컴파일러가 만든 것이 아니다. `(compile c-abs)`는
성공한다(이미 컴파일되어 있으므로 아무것도 하지 않는다).

**AOT(`compile-file`)에서도 동작한다.** C 함수 자체는 링커가 해석한다. `:library`를 쓴 선언이 있으면 그 라이브러리가
`-l`로 링크 명령줄에 추가되므로(중복은 하나로 합쳐진다) `compile-file`에 인수를 더할 필요가 없다. 소스를 읽는 것은
compile-file 자신이므로 선언에서 모을 수 있다.

빌드할 때도 심볼을 찾는다. 존재하지 않는 함수를 선언하면 링크 오류보다 먼저 그 이름을 밝힌 오류가 된다.

표준 라이브러리(prelude)는 `defffi`를 쓰지 않는다. 표준 라이브러리는 모든 실행 파일에 통째로 들어가므로, 거기에
`:library`가 붙은 선언이 있으면 FFI를 쓰지 않는 프로그램까지 그 라이브러리를 링크하게 된다.

#### def-c-struct와 타입 있는 포인터 — C 구조체 배치

```lisp
(unsafe
  (def-c-struct 이름 (필드 타입)...)
  ...)
(unsafe (pub def-c-struct ...))
```

C와 같은 배치의 구조체를 선언한다. 최상위의 `unsafe` 안에만 쓸 수 있다(그 `unsafe` 안에는 `def-c-struct`만 쓸 수 있다).
이름 바로 뒤에 문서 문자열을 둘 수 있다.

필드에 쓸 수 있는 타입은 `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32` `f64` `bool` `ptr`, 타입 있는 포인터
`(ptr T)`, 그리고 다른 `def-c-struct`(값으로 포함)이다. 배치(각 필드의 오프셋, 구조체의 크기와 정렬)는 C의 규칙으로
계산된다(LP64를 전제로 한다). 자기 자신을 가리키는 필드는 쓸 수 있지만 자기 자신을 포함할 수는 없다.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x는 0, y는 8, 크기 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

`def-c-struct`의 이름은 타입의 이름공간에 들어가지만(같은 모듈에 같은 이름의 `defstruct` 등을 둘 수 없다) **값의 타입은
아니다**. `(defun f ((p point)) ...)`라고 쓸 수 없으며, 타입 있는 포인터가 가리키는 대상으로만 나온다.

**타입 있는 포인터 `(ptr T)`**는 `T`를 가리키는 주소이다. `T`는 위의 필드에 쓸 수 있는 타입 중 하나이다. `ptr`처럼 원시
워드이며 나올 수 있는 곳의 규칙도 같다(인수, 반환 타입, 지역 변수뿐이며 `unsafe` 안에서만 값이 될 수 있다).

할당, 읽기, 쓰기는 다음 형식으로 쓴다. 모두 `unsafe` 안에서만 쓸 수 있다.

| 형식 | 의미 |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | `T`를 `n`개(생략하면 1개) 할당한다. 내용은 0으로 채워진다. `(ptr T)`를 반환한다 |
| `(c-ref p i)` | `p`에서 `i`번째 요소의 포인터. 할당한 범위를 벗어나면 오류 |
| `(c-deref p)` / `(setf (c-deref p) v)` | `p`가 가리키는 스칼라를 읽기 / 쓰기 |
| `p::field` / `(setf p::field v)` | 구조체의 필드를 읽기 / 쓰기. 포함된 구조체인 필드를 읽으면 그 주소(`(ptr 안쪽-타입)`)를 얻는다 |
| `(as ptr p)` | 타입을 잊고 `ptr`로 만든다(`qsort`의 `void *` 같은 데 넘기기 위해). 반대 방향의 변환은 없다 |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**할당한 메모리는 할당한 `unsafe`를 빠져나갈 때 해제된다.** 소유자는 같은 함수 안에서 렉시컬하게 가장 바깥의 `unsafe`이다.
정상 종료든 `panic`, `throw`, `return-from`으로 빠져나가든 해제된다. `lambda`와 `labels`의 함수는 다른 함수이므로 그 안의
`c-alloc`에는 그 안에 자신의 `unsafe`가 필요하다.

그래서 타입 있는 포인터는 그것을 할당한 `unsafe` 밖으로 가지고 나갈 수 없다. 다음은 모두 타입 검사 때의 오류이다.

- `unsafe` 식의 값으로 하기(그래서 함수에서 반환할 수도 없다)
- 클로저(`lambda`, `labels`)에서 포착하기
- `task` / `thread`에 넘기기
- `throw`로 던지기

`unsafe` 밖에서 값을 쓰고 싶으면 `unsafe` 안에서 `defstruct`나 수에 복사해 반환한다.

**C 쪽이 할당한 메모리는 다루지 않는다.** C에서 타입 있는 포인터로 들어오는 값(`defffi`의 반환값, 콜백의 인수, 포인터
타입 필드에서 읽은 값)은 살아 있는 `c-alloc` 할당 안의 그 타입의 값의 위치를 가리키는지 실행 시에 검사되며, 아니면
오류이다. NULL도 오류이다. C가 할당한 메모리나 NULL을 받고 싶으면 타입 없는 `ptr`로 받는다(내용은 읽을 수 없다).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

콜백의 인수가 검사에서 거절되면 콜백 안의 실패처럼 C 함수가 반환될 때 호출한 쪽에 전해진다.

### 3.4 defvar / defparameter / defconstant — 전역 변수

```lisp
(defvar (name Type) init-expr)        ; 아직 묶이지 않았을 때만 초기화한다
(defparameter (name Type) init-expr)  ; 매번 대입한다
(defconstant (name Type) init-expr)

; 문서 문자열 포함(CL의 defvar/defparameter/defconstant와 같은 순서: 값 뒤)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**`defvar`와 `defparameter`의 차이는 다시 읽을 때 나타난다**(CL과 같다). 그 전역 변수가 **이미 묶여 있으면 `defvar`는
초기화 식조차 평가하지 않으므로** 설정 파일을 고쳐 다시 읽어도 세션에서 바꾼 값은 그대로이다. `defparameter`는 매번
대입하므로 다시 읽으면 파일에 쓴 값으로 돌아간다.

타입 주석은 필수이다(초기화 식에서 추론하지 않는다). `defvar`는 바꿀 수 있고 `defconstant`는 바꿀 수 없다(`setf`는 오류).

### 3.5 defmethod — 메서드 정의

```lisp
; 인스턴스 메서드: (m obj args...)로 호출할 수 있다
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; 정적 / 연관 함수: (Type::name args...)로 호출할 수 있다
(defmethod name (Type (arg Type2) ...) RetType body...)
```

호출하는 쪽은 `obj`의 정적 타입으로 메서드를 해석한다(단일, 정적 디스패치). `defun`과 같은 위치에 같은 규칙으로 문서
문자열을 둘 수 있다(`where` 절 바로 뒤, 본체 맨 앞, 뒤에 본체 형식이 있을 때만). `impl` 안의 메서드도 같으며
`(documentation Type::method)`로 꺼낸다.

메서드 자신의 타입 매개변수는 `defun`과 같이 이름에 `<...>`로 쓴다. 받는 쪽 타입의 타입 매개변수(아래 예의 `T`)는 받는 쪽로부터 정해지고, 메서드 자신의 타입
매개변수(`U`)는 호출의 인수로부터 추론된다.

```lisp
(defstruct Box<T> (v T))

(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))

(fmap (Box::new 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- 메서드 자신의 타입 매개변수에는 받는 쪽 타입이 선언한 타입 매개변수(`(defstruct Box<T> ...)`의 `T`)와도, 받는 쪽에 쓴 이름과도 다른 이름을 붙인다.
- 받는 쪽 타입이 제네릭이면 받는 쪽에는 그 타입 매개변수를 모두 변수로 쓰거나(`Box<T>`) 모두 구체적인 타입으로 쓴다(`Box<int>`).
- `impl` 안의 메서드에는 타입 매개변수를 추가할 수 없다. 시그니처는 트레이트가 선언한 것을 따른다.

### 3.6 defstruct — 구조체(사용자 정의 타입)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; 제네릭(타입 매개변수는 꺾쇠괄호 안에)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- 각 필드는 `(name type)` 또는 `(pub name type)`이다(필드마다의 공개 여부. 구조체 자체의 `pub`과는 독립적이다). 끝에 식을
  하나 더 쓰면 그 슬롯의 **기본값**이 된다(`(x i32 0)`). 아래의 옵션 목록 참고.
- 다음이 자동으로 만들어진다.
  - 생성자 `Name::new`(필드 순서대로 인수)
  - 읽기 접근자 `(field-name instance)`, 그 표기 `instance::field-name`
  - 쓰기 접근자 `(set-field-name instance value)`, 그 표기 `(setf instance::field-name value)`
- 구조체 자체를 `pub`으로 하려면 `(pub defstruct ...)`처럼 앞에 `pub`을 붙인다.
- **타입은 이름이 나오기 전에 정의한다.** 필드의 타입으로 자기 자신을 쓸 수는 있지만(`(next Option<node>)`) 나중에 정의하는
  타입은 쓸 수 없다. 타입에는 `defsignature`에 해당하는 전방 선언이 없다. 아직 정의되지 않은 이름은 `defun`의 인수 타입이나
  `the`에서도 같은 `unknown type` 오류가 된다. 그래서 서로를 참조하는 두 타입은 쓸 수 없다.
- **타입 변수는 선언 위치에 쓴 것뿐이다.** `defun`/`defstruct`/`defenum`/`deftype`은 이름의 `<T>`, `defmethod`는 받는 쪽의
  타입(`(self box<T>)`, 정적 메서드는 `box<T>`)와 이름의 `<U>`, `impl`은 대상 타입과 `impl<T>`, `deftrait`는 `Self`와
  `(type Item)`의 연관 타입이다. 그 밖의 곳(인수, 반환값, 본체의 `the`/`lambda`)에서 처음 나오는 이름은 타입 변수가 되지 않고
  `unknown type`이 된다.
- **문서 문자열**: 이름 바로 뒤, 필드 앞의 문자열 리터럴은 문서 문자열이 된다(`(defstruct Name "doc" (field Type)...)`. CL의
  `defstruct`와 같은 위치). 필드는 항상 `(name Type ...)` 형태이며 맨 문자열일 수 없으므로 모호함이 없다.
  `(documentation Name)`으로 꺼낸다.

#### 옵션 목록

이름 위치에 리스트 `(Name option...)`을 쓰면 옵션을 지정한다(CL과 같은 위치).

```lisp
(defstruct (point (:constructor make-point)          ; 키워드 생성자
                  (:constructor at (x &optional y))  ; BOA 생성자
                  (:copier copy-point))
  (x i32 0)          ; 세 번째 요소는 그 슬롯의 기본값
  (y i32 0))

(point::make-point :y 7)   ; x는 0
(point::at 1)              ; y는 0
(point::at 1 2)
(copy-point p)             ; 얕은 복사(CL의 copier와 같다)
```

- **`:constructor`** — 만들어지는 것은 타입의 **정적 함수**(`point::make-point`)이며, 본체는 항상 `(point::new ...)`이다.
  `new`는 여전히 구조상 유일한 생성자이며, 여기서 만드는 것은 그것을 *호출하는 방법*이다. 여러 개를 선언할 수 있다.
  - `(:constructor name)` — 모든 슬롯을 `&key`로 받는다. **모든 슬롯에 기본값이 필요하다**(이 언어에는 CL의 "묶이지 않은
    슬롯"에 해당하는 것이 없다).
  - `(:constructor name (slot...))` — 나열한 슬롯을 위치 인수로 받는다(순서는 자유). 나열하지 않은 슬롯은 기본값으로
    채워지므로 **기본값이 필요하다**. `&optional`을 끼우면 그 뒤는 생략할 수 있다(마찬가지로 기본값이 필요하다).
- **`:copier`** — 슬롯 값이 같은 새 값을 반환하는 **인스턴스 메서드**를 만든다. CL의 copier처럼 얕은 복사이다.
- **`:include Parent`** — 부모의 슬롯 목록을 앞에 붙인다(기본값도 이어받는다. 부모는 다른 파일에 있어도 된다). **타입
  관계는 만들지 않는다.** 자식은 부모의 하위 타입이 아니고, 부모의 메서드는 자식에 적용되지 않으며, 둘을 잇는 실행 시
  검사도 없다. 이 언어에는 하위 타입이 없으며, 공통 인터페이스는 `deftrait`가 맡는다. 이어지는 것은 슬롯의 *목록*뿐이다.
- **슬롯의 기본값은 만들어진 생성자만 읽는다.** `:constructor`를 하나도 선언하지 않고 기본값을 쓰면 쓰일 수 없으므로
  오류이다.
- 넣지 않은 옵션과 그 이유:
  - **`:conc-name`** — CL에서는 하나의 평평한 함수 이름공간에서의 충돌을 피하려고 접근자에 접두사를 붙인다. 여기서는
    접근자가 받는 쪽 타입으로 디스패치되는 메서드이므로 충돌하지 않고, 접두사는 `instance::field`(슬롯 이름만 안다)를
    망가뜨린다.
  - **`:predicate`** — 실행 중에 "이 값은 `point`인가"에 답한다. 여기서 타입은 실행 시의 증거가 없는 컴파일 시의 분류이며,
    "point일 수도 있는 알 수 없는 타입의 값"이 있는 위치도 존재하지 않으므로(`Sexpr`에 대한 `match`는 닫혀 있고 `:dyn`은
    다운캐스트할 수 없다) 만들어진 술어는 항상 `true`를 반환할 수밖에 없다.
  - **`:type` / `:initial-offset` / `:named`** — 값의 표현을 리스트나 벡터로 바꾸는 지정이다. 표현은 컴파일러의 것이며
    언어에서 관찰할 수 없다.

### 3.7 defenum — 열거형(합 타입)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; 페이로드가 있는 변형(위치 필드)
  (Variant2)                  ; 페이로드가 없는 변형
  ...)

; 제네릭
(defenum Option<T>
  (Some T)
  (None))
```

- 각 변형의 형태는 `(VariantName FieldType...)`이다. 필드는 위치로만 지정한다(이름이 없다). 변형은 하나 이상 필요하고 이름은
  겹칠 수 없다.
- 값은 내장 `Option`/`Result`처럼 한정하거나 `use`를 거쳐 만든다: `(Name::Variant1 a b)`, 또는 `(use Name)` 뒤에
  `(Variant1 a b)`.
- `match` / `if-let`으로 분해할 수 있다. `match`는 망라성을 검사한다(모든 변형을 덮거나 `_`가 있어야 한다):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- 메서드와 연관 함수는 `defstruct`처럼 `defmethod`/`impl`로 나중에 더한다.
- 열거형 자체를 `pub`으로 하려면 `(pub defenum ...)`이라고 쓴다.
- **문서 문자열**: `defstruct`와 같은 위치와 규칙. 이름 바로 뒤, 변형 앞(`(defenum Name "doc" (Variant ...)...)`).
  `(documentation Name)`으로 꺼낸다.

### 3.8 deftype — 타입 별칭

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

CL의 `deftype`을 정적 타입 언어에서 의미가 있는 범위로 좁힌 것이다. **타입이 아니라 타입의 표기이다.**

- 이름 위치는 `defun`과 같고 제네릭 인수는 `Name<T,U>`로 쓴다. 쓰는 곳에서는 선언과 같은 개수의 타입 인수가 필요하다(많거나
  적으면 그 자리에서 오류).
- 전개는 **타입 파서 안에서** 일어난다. 그래서 아래쪽에서는 아무도 별칭의 존재를 모른다. 단형화의 키, 덤프, 컴파일 경로,
  그리고 **오류 메시지**가 모두 전개된 형태를 보인다. `meters`를 요구하는 함수에 대해 `(f "x")`가 실패하면 메시지에는
  `i32`가 나온다.
- **새로운 타입이 아니다.** `(deftype meters i32)`는 `meters`와 `i32`를 같은 타입으로 만드므로 섞어 써도 아무것도 잡히지
  않는다. 구별하려면 `defstruct`를 쓴다.
- **술어가 되지 않는다.** CL의 `(deftype small () '(integer 0 9))`는 `typep`이 실행 중에 판정하는 *값의 집합*을 나타내지만,
  여기서 타입은 실행 시의 증거가 없는 컴파일 시의 분류이므로 값을 제한하는 별칭은 제한할 대상이 없다.
- **자기 자신을 포함할 수 없다.** 별칭은 쓴 곳에서 전개되므로 재귀할 곳이 없다. 재귀적인 데이터 타입은
  `defstruct`/`defenum`으로 쓴다.
- 타입, 트레이트와 이름공간을 공유한다(같은 모듈 안에서 `defstruct`/`defenum`/`deftrait`와 같은 이름을 가질 수 없다).
  `(pub deftype ...)`으로 공개하고 `(use m::meters)`로 들여온다.
- **문서 문자열**: 이름 바로 뒤, 타입 앞(`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — 트레이트 기구

```lisp
(deftrait TraitName (SuperTrait...)      ; 상위 트레이트 목록은 필수. 없으면 ()
  (type AssocName)                       ; 연관 타입(여러 개 가능, 생략 가능)
  (method-name ((self Self) params...) RetType)          ; 본체 없음 = 구현 필수
  (method-name ((self Self) params...) RetType body...)) ; 본체 있음 = 기본 구현

(impl TraitName TargetType
  (where (Trait A)...)                   ; impl 전체에 걸리는 경계(생략 가능)
  (type AssocName ConcreteType)          ; 연관 타입을 구체화한다
  (method-name (recv params...) RetType body...))
```

`impl`을 거쳐 각 메서드는 `TargetType`의 보통의 `defmethod`로 등록된다. 제네릭 함수의 `where` 절에서 트레이트 경계로
참조한다([3.1 defun](#31-defun--함수-정의) 참고). 트레이트 이름은 `m::Trait`처럼 `::` 경로로 쓸 수도 있다.

**상위 트레이트 목록(필수)**: 항상 트레이트 이름 바로 뒤에 쓴다. 요소는 맨 트레이트 이름, 또는 그 트레이트에 연관 타입이
있으면 **모든 연관 타입을 고정한** `(Trait (Assoc Type))`이다.

```lisp
(deftrait Eq () ...)                       ; 상속 없음
(deftrait Ord (Eq) ...)                    ; Rust의 trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; 연관 타입을 고정한다
  (rewind ((self Self)) ()))
```

상속에는 세 가지 효과가 있다. (1) `impl Ord X`는 `impl Eq X`가 **먼저** 쓰여 있기를 요구한다(쓰는 순서에 관한 규칙. REPL과
하나씩 하는 `load`에서 결정적으로 판단할 수 있는 유일한 형태이며 Rust보다 엄격하다). (2) `(where (Ord T))`만으로 `Eq`의
메서드도 호출할 수 있다. (3) `:dyn Ord`에서 `Eq`의 메서드를 호출할 수 있고, `:dyn Ord`의 값은 `:dyn Eq`가 요구되는 곳에 그대로
넘길 수 있다(업캐스트). 하위 트레이트가 부모와 같은 이름의 메서드를 다시 선언하는 것과, 두 부모에서 같은 이름의 메서드를
상속하는 것은 모두 오류이다(vtable의 슬롯은 이름마다 하나). 다이아몬드 상속은 하나의 슬롯으로 합쳐진다.

**기본 구현**: 시그니처 뒤에 본체를 쓰면 `impl`이 그 메서드를 생략했을 때 쓰인다. 본체는 트레이트를 쓴 **모듈의
이름공간**에서 해석되므로 그 모듈의 공개되지 않은 함수도 호출할 수 있다. 본체가 있는 메서드에는 `where` 절과 문서 문자열도
쓸 수 있다. 본체의 타입 검사는 **선언한 시점에 한 번**, `Self`를 타입 변수로 둔 채(`Self: 그 트레이트`를 경계로) 이루어진다
(Rust와 같다). 어떤 `impl`도 생략하지 않는 기본 구현이라도, 어느 구현 타입에 대해서도 통과하지 못할 실수는 거기서 걸러진다.
`self`에 대해 그 트레이트 자신과 상속원의 메서드를 호출하는 것은 이 경계로 통과하며, 연관 타입은 자기 자신으로 고정되므로
`Item`을 반환하는 시그니처와 본체가 구체 타입을 모른 채 대조된다.

**블랭킷 구현**: 대상을 타입 변수로 하면 경계를 만족하는 모든 타입에 한꺼번에 구현한다.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; 본체 없음. 모두 기본 구현
```

**구체 타입이 실제로 쓰기 전까지는 코드를 만들지 않는다**(타입마다 한 번, 보통의 단형화와 같은 구조). 하나의 트레이트에
블랭킷 구현은 최대 하나이다. 같은 타입에 명시적인 `impl`이 있으면 그것이 우선한다. 본체의 타입 검사는 생성과 별개로, 선언한
시점에 **대상을 타입 변수로 둔 채** 한 번 이루어진다(Rust와 같다). 한 번도 쓰이지 않는 구현이라도, 선언한 경계 아래에서
어떤 대상에 대해서도 통과하지 못할 실수는 거기서 걸러진다. 경계가 허락하는 호출(`(where (Ord T))` 아래의 `(less self other)`
등)은 제네릭 `defun`의 본체처럼 통과한다.

**문서 문자열**: `deftrait`는 상위 트레이트 목록 바로 뒤, 항목 앞에 문자열 리터럴을 두면 트레이트 전체에 문서 문자열을 하나
가질 수 있다(`(deftrait Name () "doc" (type ...) (method ...)...)`). 본체가 없는 시그니처는 문서 문자열을 가질 수 없다. 끝의
문자열 자체가 기본 구현의 반환값이 되어 둘을 구별할 수 없기 때문이다.

표준 라이브러리가 제공하는 트레이트: **`Iter`**(`next` / 연관 타입 `Item`. `doiter`와 시퀀스 함수의 바탕), **`Eq`**(`equals`.
`not-equals`는 기본 구현), **`Ord`**(`Eq`를 상속. `less`만 구현 필수이고 `less-equal` / `greater` / `greater-equal`은 기본
구현), **`Error`**(`message` / `source`. 오류 타입을 한결같이 다루는 `:dyn Error`), **`print-object`**(타입별 출력 표현),
**`Pathish`**(경로명 지정자 = 문자열 또는 `pathname`), 그리고 스트림 계층 **`Stream`** → **`InputStream`** /
**`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**. 어느 타입이 어느 트레이트를 구현하는지는
[types.md](types.md)에, 각 트레이트의 메서드는 [표준 트레이트](functions/traits.md),
[오류 타입](functions/option-result.md#3-오류-타입과-error-트레이트),
[print-object](functions/printing.md#5-print-object타입별-출력-표현), [스트림](functions/streams-files.md)에 있다. 자신의
컬렉션 타입에 `Iter`를 `impl`하면 `doiter`(5장)와 `map` / `filter` / `sort` 등이 그대로 동작한다.

트레이트 호출은 기본적으로 **정적**이다(받는 쪽의 정적 타입으로 해석된다). 구체 타입이 실행 중에 정해지는 값을 다루려면
트레이트 객체 타입 `:dyn Trait`(2장)을 쓰면 vtable을 거친 동적 디스패치가 된다.

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; 호출 지점 하나에 구현마다의 답
```

`:dyn Trait`이 될 수 있는 것은 "모든 메서드가 `self` 받는 쪽을 가지고, 받는 쪽 이외에 `Self`를 쓰지 않으며, 메서드 자체가
제네릭도 가변 인수도 아닌" 트레이트뿐이다(상속한 메서드도 같은 조건을 만족해야 한다).

`:dyn` 상자에 넣을 수 있는 것은 값이 힙에 표현을 가진 타입뿐이다.

| 넣을 수 있는 것 | 넣을 수 없는 것 |
|---|---|
| `defstruct` / `defenum`의 타입(`Vector<T>`, `cons-cell<A,B>`, `Result<T,E>`, 표준 라이브러리의 구조체 포함), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | 고정 폭 정수(`i8`~`u32`), `f32`, `bool`, `char`, `symbol`, `()`, 함수 타입, 상자가 없는 `Option<T>`([Option의 실행 시 표현](functions/option-result.md#2-optiont의-실행-시-표현)) |

넣을 수 없는 타입의 값을 `:dyn` 위치에 두면 타입 오류이다. 그런 값을 `:dyn`으로 다루려면 `(defstruct flag (v bool))`처럼
구조체로 감싼다.

### 3.10 module / use — 이름공간

```lisp
(module path body...)      ; path는 foo나 foo::bar 같은 구간의 열
(in-module path)           ; 여기서 이 단위의 끝까지 path 안(module의 평평한 형식)
(use path...)              ; 함수, 타입, 모듈을 현재 이름공간에 별칭으로 들여온다
(import path...)           ; use와 같다(CL 호환 표기)
(shadowing-import path...) ; 맨 이름이 이미 쓰이고 있음을 알고도 가져오는 use
```

- `module`은 이름공간을 만든다. **타입은 이름공간이 아니다**(Rust처럼 타입은 연관 함수와 메서드만 가진다).
- 타입을 `use`하면 그 생성자와 공개된 정적 메서드도 맨 이름으로 쓸 수 있게 된다(예: `(use option)` 뒤에는
  `option::some`/`option::none`이라고 쓰지 않고 `some`/`none`을 호출할 수 있다).
- 맨 이름(한정하지 않은 식별자)의 해석 순서: 특수 형식 → 생성자 → 자유 함수(현재 이름공간 → 루트) → 인스턴스 메서드(첫 번째
  인수의 정적 타입으로 해석). 중간의 부모 모듈로 거슬러 올라가지 않는다.
- 한정 경로 `a::b`는 `a`를 위의 순서로 해석하고, 모듈이면 안으로 들어가며, 타입이면 마지막 구간을 연관 항목으로 해석한다.
- **`use`는 그 뒤의 형식에 효과가 있다.** 파일은 형식 하나씩 읽히고 의존 관계도 그 형식을 검사하기 직전에 해석되므로,
  `(use m)`보다 **위에** `m::f`를 쓰면 `unresolved path`가 된다. `use`는 파일 맨 앞에 둔다.
- **`use`는 여러 경로를 받을 수 있다**(`(use a::f b::g)`). `import`는 같은 동작의 CL 호환 표기이다.
- **맨 이름이 이미 쓰이고 있는 `use`는 보고된다.** 맨 이름의 해석은 별칭보다 먼저 그 모듈 자신의 정의를 보므로
  `(defun twice ...)` 뒤의 `(use m::twice)`는 **아무것도 하지 않는다**. 그것을 의도한다면 `shadowing-import`를 쓴다(그래도
  정의를 이길 수는 없다. 정의를 없앨 수단이 없기 때문이다. 이길 수 있는 것은 앞선 별칭뿐이다).
- **`in-module`은 `(module path body...)`의 평평한 형식이다.** `(in-module geometry)`라고 쓰면 거기서 그 단위(파일, 또는
  바깥 `module`의 본체)의 끝까지가 `geometry` 안이 된다. 파일 자신의 모듈 **안쪽**에 들어간다(`main.typl`에서는
  `main::geometry`). 둘을 이어 쓰면 차례로 중첩된다. CL의 `in-package`와는 다르며 이름도 일부러 구별했다. 이 체계에서는
  파일이 이미 모듈이므로 "고를" 대상이 없고, 형식이 할 수 있는 것은 중첩뿐이다.

### 3.11 파일과 모듈의 대응(여러 파일 프로젝트)

소스 루트에서 본 파일 경로가 모듈 경로이다. `<root>/geo/point.typl`의 내용은 암묵적으로 모듈 `geo::point`에 감싸진다
(디렉터리도 한 구간이다. Rust/Python 방식). 파일 안의 명시적인 `(module bar ...)`는 그 **안쪽**에 중첩된다
(`geo::point::bar`). 도출된 경로와 명시적인 선언은 충돌하지 않는다.

- **소스 루트**: 프로젝트 루트에 매니페스트 파일 `typelisp.toml`을 둔다(비어 있어도 된다. 선택적으로 `src = "src"` 한 줄로
  소스 디렉터리를 지정할 수 있다). 대상 파일의 디렉터리에서 위로 올라가며 찾는다. 매니페스트가 없으면 시작점 파일의
  디렉터리(REPL은 현재 디렉터리)가 루트이다.
- **필요할 때 읽기**: `(use geo::point)`가 아직 읽지 않은 모듈을 참조하면 대응하는 파일(`geo/point.typl`)을 자동으로 읽고
  타입 검사해 등록한다. `use a::b::c`는 가장 긴 접두사부터 `a/b/c.typl` → `a/b.typl` → `a.typl` 순서로 찾는다(`c`가 모듈
  안의 항목일 수 있기 때문이다). 다른 모듈에서 보이는 정의에는 `pub`이 필요하다([3.13 pub](#313-pub--공개)).
- **순환 참조는 오류**: `circular module dependency: a -> b -> a` 형태로 사슬을 보고한다.
- **실행**: `typl <file.typl>`로 파일을 실행한다(인수가 없으면 REPL). REPL의 `use`도 같은 규칙으로 파일을 해석한다.
- **cons 영역 용량**: `typl --heap-cells N`으로 cons 셀 영역의 **초기 용량**을 지정한다(기본 65536. `--heap-cells=N`도 되며
  파일 실행과 REPL 모두에 적용된다). 영역이 모자라면 **더해서 늘어난다**. 증가의 상한은 초기 용량의 256배이며, 그것을 넘는
  할당은 `heap exhausted`가 된다. 즉 초기 용량은 "처음에 이만큼 잡는다"는 뜻이고 상한은 "여기를 넘으면 누수로 본다"는
  뜻이다.

### 3.12 load — 평평한 로드

```lisp
(load "path")   ; 최상위에서만. path는 문자열 리터럴
```

- CL식 **평평한 로드**: 대상 파일의 형식을 **그대로 현재 이름공간에** 읽어 들인다(`use`처럼 모듈로 감싸지 않는다).
  최상위에서만 쓸 수 있다(함수 본체 안에서는 타입 오류).
- `path`는 읽어 들이는 쪽 파일의 디렉터리 기준이다(REPL에서는 프로세스의 cwd 기준). 확장자가 없으면 `.typl`을 붙인다.
- 읽어 들인 파일 자신의 `(load ...)`/`(use ...)`도 재귀적으로 처리된다.
- **형식 하나씩 읽고 그 자리에서 실행한다**(CL의 `load`와 같다). 형식 *k*는 *k+1*을 읽기 전에 실행을 마친다. 도중에 구문
  오류나 타입 오류가 있어도 그 앞의 형식은 이미 실행되었다. `use`로 읽는 모듈 파일은 이와 달리 하나의 단위로 검사되고,
  실행은 그것을 `use`한 쪽에 맡겨진다(CL의 `compile-file`에 해당한다).

### 3.13 pub — 공개

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub`을 붙일 수 있는 것은 위의 11가지뿐이다(`module`/`use`/`deftrait`/`impl`에는 붙일 수 없다). 정의 형식을 괄호로 감싸는
`(pub (defun ...))` 형태가 아니라 `pub` 바로 뒤에 정의 키워드를 쓴다. `pub` 하나는 정확히 하나의 정의를 공개한다(여러
정의를 한꺼번에 지정할 수 없다).

### 3.14 defmacro — 매크로 정의

```lisp
(defmacro name (필수... &optional opt... &rest rest-name &key key...) body...)
```

- 매개변수와 반환값은 모두 항상 `Sexpr`이므로 타입 주석은 쓰지 않는다.
- CL식의 비위생적 매크로(`gensym`으로 충돌을 피하는 것은 매크로 작성자의 책임).
- 람다 목록은 CL의 `필수 &optional &rest &key` 순서이다(각 표시는 최대 한 번, 이 순서로만).
  - `&optional` … 생략할 수 있는 인수. `name` 또는 `(name 기본값-식)`. 기본값 식은 전개할 때 평가되며(앞서 묶인 매개변수를
    참조할 수 있다) 생략하면 묶인다(기본값이 없으면 빈 리스트 `()`).
  - `&rest name` … 남은 위치 인수를 하나의 `Sexpr` 리스트로 모아 받는다.
  - `&key` … 키워드 인수. `name` 또는 `(name 기본값-식)`. 호출하는 쪽은 `:name 값`으로 넘긴다(순서 자유). 생략하면 기본값
    식(없으면 빈 리스트 `()`). 알 수 없는 키워드나 홀수 개의 `:key` 열은 오류이다.
- 예: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — 지역 매크로 바인딩

```lisp
(macrolet ((name (람다 목록) body...) ...) body...)   ; 렉시컬 범위의 매크로
(symbol-macrolet ((name 전개-형식) ...) body...)       ; 이름이 형식 하나를 나타낸다
```

둘 다 **식**의 특수 형식이며 실행 시에는 아무것도 남지 않는다(컴파일되는 것은 본체를 전개한 형식이다). 람다 목록은
`defmacro`와 같다. 자세한 규칙과 예는 [지역 매크로 바인딩](functions/system.md#9-지역-매크로-바인딩macrolet--symbol-macrolet)에
있다.

## 4. 바인딩과 조건 분기

```lisp
(let ((name val) ...) body...)      ; 병렬 바인딩
(let* ((name val) ...) body...)     ; 순차 바인딩(앞의 바인딩을 뒤의 초기화 식에서 쓸 수 있다)

(if cond then else)                 ; else는 필수(항상 세 요소)
(when cond body...)                 ; else가 없는 if(Unit 타입). defmacro
(unless cond body...)               ; when의 부정판. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; 키 목록: 어느 것과든 맞으면
  (else body...))                   ; expr은 한 번만 평가된다. key는 equal로 비교한다.
                                     ; key는 "리터럴"이며 평가되지 않는다(CL과 같다).
                                     ; 맨 심볼 a는 심볼 'a를 뜻한다.
                                     ; 'a라고 쓰면 오류(맨 a를 쓴다). defmacro
(ecase expr (key body...) ...)      ; 맞기를 요구하는 case. 아무것도 맞지 않으면 panic. defmacro
(ccase expr (key body...) ...)      ; CL의 ccase. 제공할 재시작이 없으므로 ecase와 같다. defmacro
(and expr...)                       ; 단락 평가. 인수가 0개면 true. defmacro
(or expr...)                        ; 단락 평가. 인수가 0개면 false. defmacro
(progn body...)                     ; 차례로 실행하고 마지막 값을 반환한다
(unsafe body...)                    ; progn과 같다. 더해서 FFI 호출과 원시 워드를
                                     ; 쓸 허가를 준다. 3.3 defffi 참고
(prog1 form more...)                ; 모두 평가하고 값은 form의 것. defmacro
(prog2 a b more...)                 ; 모두 평가하고 값은 b의 것. defmacro
(the Type expr)                     ; 타입 주석(실행 시의 효과 없음)
```

### 4.1 unsafe — 검사할 수 없는 전제를 떠맡기

```lisp
(unsafe body...)
```

`progn`과 같다. 본체를 차례로 평가하고 마지막 값을 반환한다. 범위를 만들지 않고 함수 경계도 아니다(`break` /
`return-from`은 그대로 바깥으로 지나간다). 다른 점은 그 안에서만 쓸 수 있는 것이 있다는 것이다.

현재 `unsafe`를 요구하는 것은 세 가지이다. [defffi](#33-defffi--c-함수-선언ffi)로 선언한 C 함수의 호출, 원시 워드(`ptr` /
`c-long` / `c-ulong` / `(ptr T)`)를 값으로 만드는 것, 그리고
[`def-c-struct`와 `c-alloc`](#def-c-struct와-타입-있는-포인터--c-구조체-배치)이다.

`c-alloc`으로 할당한 메모리는 같은 함수에서 가장 바깥의 `unsafe`를 빠져나갈 때 해제된다. 그 `unsafe`만은 `progn`과 달리
나갈 때 할 일, 즉 해제가 있다.

`unsafe`가 떠맡는 것은 컴파일러가 확인할 수 없는 다음 전제이다.

- **타입의 일치.** 선언한 C 시그니처가 실제와 맞는다는 것. 맞지 않으면 인수가 엉뚱한 레지스터에 들어가고 반환값이 엉뚱한
  폭으로 읽힌다.
- **메모리 안전.** C 쪽이 넘겨받은 것을 어떻게 다루는지.
- **프로세스 전역의 상태.** 환경 변수, 시그널 핸들러, `errno`. 예를 들어 FFI로 `setenv`를 호출하면 이 구현의
  `decode-universal-time`이 지역 시간을 구할 때의 전제가 깨진다.
- **스레드 안전.**

타입 검사에서 빠져나가는 출구가 아니다. `(unsafe (+ 1 "two"))`는 통과하지 않는다. 허가되는 것은 특정한 **연산**을 쓰는 것이지
엉터리를 쓰는 것이 아니다.

렉시컬하게 동작한다. `unsafe` 안에 쓴 `lambda`의 본체는 이 허가를 이어받는다(Rust의 `unsafe` 블록 안의 클로저와 같다). 그
값은 나중에 `unsafe` 밖에서 호출될 수 있지만, 거기에 쓴 것 자체를 책임을 떠맡은 것으로 본다.

### 4.2 destructuring-bind — 리스트를 모양대로 분해

```lisp
(destructuring-bind 람다-목록 form body...)
```

`form`이 만드는 리스트를 **모양대로** 분해해 묶는다. 람다 목록은 `defmacro`의 것이다(필수 → `&optional` → `&rest`/`&body` →
`&key`, 각각 기본값 식 포함). CL이 둘을 하나로 공유하는 것과 같은 이유로, 같은 것을 분해하는 두 형식이기 때문이다.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **묶이는 변수는 모두 `Option<Sexpr>`이다.** 구현의 제한이 아니라 묶이는 대상의 성질이다. S 식 리스트는 이 언어에서 유일한
  리스트이므로 요소에 줄 다른 타입이 없다. 스칼라가 필요한 곳에서 `match`로 넘어가는 것은 `defmacro`의 본체와 같다.
- **모양이 맞지 않으면 panic한다**(CL의 오류에 해당). 요소가 모자라거나, 너무 많거나, `&key`의 열이 홀수 개이거나, 알 수
  없는 키워드이면 그렇다. `sexpr-car`는 `()`에 `()`를 반환하는 관대한 함수이므로 검사를 쓰지 않으면 짧은 리스트가 조용히
  빈 열로 묶여 버린다.
- **중첩된 람다 목록은 지원하지 않는다.** `defmacro`도 받지 않으므로 규칙을 하나로 유지한다. `(a (b c))`는 하위 리스트를
  조용히 `b`에 묶지 않고, 그렇다는 오류가 된다.
- `&optional` / `&key`의 기본값 식은 **쓰일 때만 평가된다**(CL과 같다).
- CL의 `&allow-other-keys`에 해당하는 것은 없다(`defmacro`에도 없다).

### 4.3 match — 패턴 매칭

```lisp
(match expr
  (pattern body...)
  ...)
```

패턴의 종류:
- `_` — 와일드카드
- 변수 이름 — 바인딩 패턴(항상 맞는다). 다만 검사 대상의 타입에 그 이름의 변형이 있으면 **아래의 맨 변형 이름 패턴**으로
  해석된다
- 맨 변형 이름 — 인수를 받지 않는 변형과 맞는다(`(match c (red 1) (blue 2))`). 필드가 있는 변형을 맨 이름으로 쓰면 인수 개수
  오류가 되므로 `(circle r)`처럼 괄호로 쓴다
- **즉시값 리터럴**: 정수 / `true`/`false` / 문자 — 워드로 비교한다
- **값 리터럴**: 문자열 / 부동소수점 수 / 심볼(`'foo`) / 다배정도 정수 / ratio — 그 타입의 `Eq::equals`
  ([표준 트레이트](functions/traits.md#2-eq--ord비교))로 값을 비교한다. 문자열은 동일성이 아니라 내용을 비교한다
- `(= expr)` — 아무 식이나 평가해 `Eq::equals`로 비교한다. 리터럴 구문이 없는 타입(`defstruct` 인스턴스, 전역 변수, 계산
  결과)을 비교하는 유일한 방법이며, 사용자 정의 `Eq` 구현이 그대로 비교 규칙이 된다. `expr`은 그 갈래 위치에서 보이는
  모든 것(인수, 바깥 바인딩, 전역 변수)을 참조할 수 있다
- `(Ctor sub-pattern...)` — 생성자 패턴(`Some x` `None` `Cons a d` `Ok v` 등)
- `#{p0 p1 ...}` — 튜플 패턴. 요소마다 부분 패턴으로 대조한다. `Sexpr`에 대해서는 요소 수가 같은 `#{..}` 데이터에만 맞는다
- `(:or p1 p2 ...)` — or 패턴. 선택지 중 하나라도 맞으면 맞는다. 본체가 하나뿐이므로 모든 선택지가 같은 이름의 변수를 같은 타입으로 묶어야 한다. 생성자
  패턴 안에도 쓸 수 있다(`(some (:or (circle r) (rect r _)))`)

`Eq`를 구현하지 않은 타입을 값 리터럴 / `(= expr)`로 비교하면 타입 오류이다(조용히 맞지 않는 갈래를 남기는 대신 비교할 수
없다고 말하는 쪽을 택한다).

**가드**: 패턴 뒤에 `:when 조건`을 쓰면 패턴이 맞고 조건도 참일 때만 그 갈래를 고른다. 조건이 거짓이면 다음 갈래를 시도한다. 조건은 패턴이 묶은 변수를 읽을 수
있다.

```lisp
(defun classify ((n int)) string
  (match n
    (0 "zero")
    (k :when (< k 0) "negative")
    (k :when (evenp k) "even")
    (_ "odd")))
```

- 가드가 붙은 갈래는 망라성 검사에서 세지 않는다(Rust와 같다). 조건이 거짓일 수 있으므로, 그 갈래가 덮을 변형에는 가드 없는 갈래나 `_`가 따로 필요하다.
- or 패턴과 가드는 함께 쓸 수 있다. 가드는 어느 선택지로 맞았든 평가된다(`((:or 1 2 3) :when on "small")`).

**`Sexpr` 검사 대상에 대한 값 리터럴**: `sexpr`의 `Eq`는 `eq`(CL의 동일성)이므로 즉시값 — `'foo`(인턴됨) / 정수 / 문자 /
`true`/`false` — 은 그대로 쓸 수 있고 내용으로 맞는다.

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

즉시값이 아닌 리터럴(문자열 / 부동소수점 수 / 다배정도 정수 / ratio)은 `Sexpr`에 대해 **쓸 수 없다**. 그 `eq`는 객체의
동일성을 비교하므로 "타입은 통과하지만 결코 맞지 않는 갈래"가 되기 때문에, 변형 패턴을 가리키는 오류로 한다.
`(str "hi")`라고 쓰면 `string`으로 분해해 내용으로 비교한다. `(= expr)`는 명시적으로 `equals`를 요구하므로 이 제한을 받지
않는다.

**검사 대상이 ADT일 필요는 없다.** `string`/`symbol`/`i32`/`f64` 등을 바로 `match`할 수 있다(문자열 리터럴 패턴이 쓰이는
곳이 바로 거기이다). 다만 변형이 없는 타입은 열거로 망라할 수 없으므로 `_`(또는 와일드카드 역할의 바인딩 패턴)가 필요하다.

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; 변형이 없는 타입이므로 `_`가 필요하다
```

`Sexpr` 검사 대상에 대해서는 위의 내장 19개 변형 패턴에 더해 **다운캐스트 패턴**(사용자 정의 ADT의 인스턴스를 꺼낸다)을 쓸
수 있다. `(list p 42)`처럼 `Sexpr`로 암묵 변환된 `defstruct`/`defenum`(3장)의 인스턴스를 `match`로 되찾는 구문이다.

- `(TypeName sub-pattern...)` — **타입 이름**을 앞에 둔 필드 분해(구조체 전용. `defstruct`는 항상 변형이 하나이므로 변형
  이름이 아니라 타입 이름으로 쓴다). 예: `(defstruct point (x f64) (y f64))`에 대해 `(point x y)`.
- 맨 변형 이름 `(VariantName sub-pattern...)` — `defenum`의 변형을 꺼낸다. `(use EnumType)` 뒤에 보이는 맨 이름으로 해석된다
  (생성자를 호출할 때와 같은 가시성 규칙). 예: `(defenum color (red) (blue))`에 대해 `(use color)` 뒤에 `(red)` `(blue)`.
  보이는 여러 열거형의 변형 이름이 겹치면 모호성 오류가 되므로 한정 형식 `(EnumType::VariantName ...)`도 쓸 수 있다(`use`가
  필요 없다).
- `(the Type pattern)` — 타입 전체로 다운캐스트(통째로 묶기). 필드를 분해하지 않고 값을 그대로 `pattern`에 넘긴다. 가변
  구조체를 동일성을 유지한 채 꺼내는 유일한 방법이며, `Sexpr`에서 `Vector<T>`/`HashTable<K,V>`를 꺼내는 유일한 수단이기도
  하다(둘 다 필드 분해 형식이 없다). 예: `(the point p)` 뒤에 `(setf p::x 9)`는 리스트 안의 원래 인스턴스에도 반영된다.

**`Option<Sexpr>`의 패턴**: S 식 데이터의 타입은 `Sexpr`가 아니라 `Option<Sexpr>`이며, 빈 리스트는 `Sexpr`의 변형이 아니라
`Option`의 `none`이다. 그래서 `Option<Sexpr>`를 `match`할 때는 `Sexpr`의 19개 변형과 `none`을 **같은 갈래 목록에 평평하게**
쓸 수 있다(`Option`을 벗기는 바깥 `match`가 필요 없다).

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; 빈 리스트
    (_          9)))
```

망라성도 같은 평평한 전체 집합 — `Sexpr`의 19개 변형에 `none`을 더한 20개 — 에서 검사된다. `(none)`을 잊으면 `_`가 없는 한
오류이다. `(some x)`도 쓸 수 있으며 "비어 있지 않은 무언가"를 묶는다.

이 편의 표기는 **정확히** `Option<Sexpr>`에만 적용된다. `Option<Option<Sexpr>>`에서는 `(int n)`이 어느 층을 벗겼는지 정할 수
없으므로 평소처럼 `match`를 두 겹으로 쓴다.

**트레이트 객체(`:dyn Trait`, 2장)의 검사 대상**에도 같은 다운캐스트 패턴을 그대로 쓸 수 있다. `match`가 상자를 연 뒤 위의
`Sexpr` 패턴 구조에 넘기므로 추가 구문은 없다. 구현 타입의 집합은 열려 있으므로 망라되지 않으며 `_`가 필요하다.

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; 타입 이름을 앞에 둔 필드 분해
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**갈래 사이의 타입 추론**: 모든 갈래는 같은 타입이어야 한다(`panic` 같은 발산하는 갈래는 제외). 기대 타입이 없는 곳에 쓴
`match`에서는 갈래들이 모자란 타입 인수를 **서로** 채운다. `(result::ok v)`는 `T`만, `(result::err e)`는 `E`만 정하지만 둘이
나란히 있으면 `Result<T,E>`가 정해진다. 끝까지 어느 갈래도 정할 수 없는 타입 인수가 있으면 그 갈래 자체의 오류가 된다
(`cannot infer type argument ...`). `match` 밖에서는 정할 수 없는 타입 인수가 그 자리에서 오류이다.

다운캐스트 패턴을 쓰는 `match`의 망라성 검사는 `Sexpr` 자체의 변형을 덮는 것으로 세지 않는다(다운캐스트 패턴만 나열한
`match`는 `_`로 닫아야 한다). 제네릭 ADT(`defstruct point<T> ...` 등)는 다운캐스트 패턴의 타입 인수를 추론할 수 없으므로
필드 분해 형식(`(point ...)`)과 맨 변형 형식은 쓸 수 없고, `(the point<i32> p)`처럼 `the`로 명시한다.

**다운캐스트는 인스턴스화도 본다.** 명시한 타입 인수는 맞추는 데 쓰인다. `(the point<i32> p)`는 `point<i32>`의 값만
통과시키며, `point<string>`은 다음 갈래로 넘어간다. 값이 자신의 타입 인수를 포함한 타입을 기억하기 때문이다(`print-object`를
고르는 것과 같은 구조).

```lisp
(if-let (pattern val) then els)     ; val이 pattern과 맞으면 then(바인딩 포함), 아니면 els. defmacro
(while-let (pattern val) body...)   ; val(매번 다시 평가)이 pattern과 맞는 동안 반복한다. defmacro
```

## 5. 반복

```lisp
(loop body...)                      ; 무한 루프. break/return으로 빠져나간다
(while test body...)                ; test가 참인 동안 반복한다. defmacro
(until test body...)                ; test가 거짓인 동안 반복한다(while의 부정판). defmacro
(dotimes (var count-expr) body...)  ; count-expr을 한 번 평가하고 var를 0..count-1로 움직인다. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL식의 병렬 진행 반복. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; do의 순차판(let* 바인딩, 차례로 대입). defmacro
(doiter (var coll-expr) body...)    ; Iter 트레이트를 구현한 값을 순회한다. defmacro

(break)                             ; 가장 안쪽의 루프만 빠져나간다. 값은 항상 Unit
(return)                            ; 가장 안쪽의 루프만 빠져나간다
(return value)                      ; 값을 가지고 가장 안쪽의 루프를 빠져나간다
```

`break`/`return`은 모두 **가장 안쪽의 바깥 루프만** 빠져나간다(함수의 이른 반환이 아니며 `lambda`의 경계를 넘을 수 없다).
`loop`의 타입은 안에서 찾은 `break`/`return`의 값 타입을 합친 타입이다(한 번도 빠져나가지 않으면 `!`). 함수를 빠져나가려면
아래의 `return-from`을 쓴다.

### 5.1 `block` / `return-from` — 이름 붙은 탈출

```lisp
(block name body...)                ; 이름 붙은 탈출 대상. 값은 마지막 형식,
                                    ; 또는 return-from이 넘긴 값
(return-from name)                  ; Unit으로 그 block을 빠져나간다
(return-from name value)            ; 값을 가지고 빠져나간다
```

**`defun` / `defmethod` / `labels`의 각 함수는 자기 이름의 block을 암묵적으로 세운다**(CL과 같다). 그래서
`(return-from f v)`는 함수의 이른 반환이다.

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block`은 **렉시컬한** 탈출이며 이름은 **쓴 곳에서 해석된다**. 검사기는 `return-from`을 바깥의 `block`에 대응시키고 그 값의
타입을 블록의 탈출 타입에 합친다. 그래서:

- 대응하는 `block`이 없는 `return-from`은 **타입 오류**이다(실행 시 오류가 아니다).
- 값의 타입이 다른 탈출이나 본체의 타입과 맞지 않으면 **타입 오류**이다(`match` 갈래와 같은 규칙).
- 같은 이름의 `block`이 중첩되면 **안쪽이 이긴다**(CL의 가림 규칙).
- **함수의 경계를 넘을 수 없다.** `lambda` 안에서 바깥의 `block`으로 빠져나갈 수 없다(`lambda`는 block을 세우지 않는다. CL의
  암묵적 block은 *이름*을 요구하는데 이름 없는 함수에는 그것이 없다). 넘어야 하는 것은 `catch`/`throw`(8장. 이것은
  **동적**이다)를 쓴다.

`break`/`return`(5장)처럼 **정적인** 탈출이므로, 컴파일된 코드에서는 컴파일 시에 정해진 기본 블록으로의 분기이다. 사이에
`unwind-protect`가 있으면 그 `cleanup`이 실행된다(8장).

`return-from`을 한 번도 쓰지 않으면 암묵적 block에는 아무 비용도 없다.

### 5.2 확장 `loop`(CL의 LOOP)

**`loop`의 첫 번째 요소가 키워드이면** 절의 열로 읽는다. 그렇지 않으면 위의 단순 루프 그대로이며, 이미 쓴 `loop`의 의미는
바뀌지 않는다(CL 자체의 simple loop 규칙과 같다).

CL은 절 단어를 맨 심볼로 쓰지만(`(loop for i from 1 to 3 collect i)`) 여기서는 **모두 키워드**이다. 맨 `for`는 변수 참조가 될
뿐이며, 키워드인지가 단순 루프와의 경계이기도 하다. 예외는 변수와 값을 나누는 `=`로, 위치가 하나로 정해지므로 맨 것과
키워드(`:=`) 어느 쪽으로든 읽는다.

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**변수 절**(본체 절보다 앞에 쓴다. CL의 규칙이다. 뒤에 쓰면 "거기서부터만 반복한다"로 읽힐 수 있어 오류이다):

| 절 | 의미 |
|---|---|
| `:with v = e` | 한 번만 묶는다. 앞 절의 변수를 읽을 수 있다 |
| `:for v :in s` / `:for v :across s` | `Iter`의 요소를 차례로. 여기에는 CL의 리스트/벡터 구별이 없으므로 같은 절의 두 표기이다 |
| `:for v :on s` | 차례로 이어지는 **접미사**. CL은 공유하는 꼬리 cons를 넘기지만 `Iter`에는 공유할 꼬리가 없으므로 매번 새 `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | 세기. `:downfrom`/`:upfrom`도 된다 |
| `:for v = e [:then f]` | `e`에서 시작해 두 번째부터는 `f`(`:then`이 없으면 매번 `e`) |
| `:repeat n` | 그 횟수만큼 반복한다 |

`:for`가 여러 개이면 **병렬로** 나아가고, 어느 하나가 다하면 끝난다.

**본체 절**(매번 쓴 순서대로 실행된다):

| 절 | 의미 |
|---|---|
| `:do form...` | 부작용을 위해 |
| `:collect e [:into v]` | `Vector<T>`에 모은다 |
| `:append e [:into v]` | `Iter`의 내용을 잇는다 |
| `:sum e` / `:count e` | 합계 / 참이었던 횟수 |
| `:maximize e` / `:minimize e` | 최대 / 최소. **`Option<T>`**(CL이 빈 열에 nil을 반환하는 것과 같다. 임의의 `Ord` 타입에는 최소 원소가 없다) |
| `:always e` / `:never e` | 모두 만족하면 `true`, 하나라도 만족하지 않으면 그 즉시 `false` |
| `:thereis e` | `e`는 **`Option<T>`**. 첫 `some`을 반환하고, 없으면 `none`(CL의 "첫 nil이 아닌 값"에 해당하는 것이 이것. `bool`을 검사하려면 `:always`/`:never`) |
| `:while e` / `:until e` | 여기서 **정상 종료**(`:finally`가 실행되고 모은 것이 답) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | 절 하나를 조건부로 한다 |
| `:return e` | 그 값으로 즉시 빠져나간다(`:finally`는 실행되지 않는다. CL과 같다) |
| `:initially form...` / `:finally form...` | 루프 전 / 정상 종료 시 |

**`:named name`**(다른 모든 절보다 앞에, 하나만)은 루프 전체를 `(block name …)`으로 감싼다. `(return-from name e)`는 중첩된
루프 안에서도 단번에 빠져나가며, `:return`처럼 `:finally`는 실행되지 않는다. 이름을 붙이지 않으면 block은 세우지 않는다.
CL의 이름 없는 `loop`는 `block nil`을 세우지만, 여기에는 `nil`이 없고 `break`/`return`(5장)이 이미 "가장 안쪽의 루프를
빠져나가기"를 제공한다.

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

`:finally (return 0)`을 생략하면 **타입 오류**이다. `block`의 규칙이 작동할 뿐이다(5.1). 탈출의 타입 `int`와 루프가 다했을 때
남기는 `()`가 맞지 않는다.

**루프의 값**: 누적 절이 있으면 그 누적(여러 개면 첫 번째), `:always`/`:never`면 `true`, `:thereis`면 `none`, 아무것도 없으면
`()`이다. `:finally`의 마지막이 `(return e)`이면 그것이 값이다. CL의 `finally (return …)` 관용구이며, 누적하지 않는 루프가
자신의 답을 밝히는 유일한 방법이다.

**CL과의 차이 / 넣지 않은 것**:

- **절 단어는 키워드이다**(위).
- `:maximize`/`:minimize`/`:thereis`는 `Option<T>`를 반환한다(nil이 없기 때문이다).
- **`:return`만 쓰고 누적도 `:finally`도 없는 것은 오류이다.** CL은 다했을 때 nil을 반환하지만 여기에는 그것이 없으므로 루프는
  다했을 때의 값을 밝혀야 한다.
- `:and`로 병렬 절을 잇기, `:being`/해시 테이블 전용 반복, `:it`, `:nconc`는 넣지 않았다.
- `:collect`의 요소 타입은 누적되는 식의 타입에서 정해진다. 함수 타입처럼 **타입 이름으로 쓸 수 없는 타입**을 모으려 하면
  그렇다는 오류가 된다.

## 6. 함수 값과 호출

```lisp
(lambda (params) RetType body...)   ; 일급 함수 값(클로저)을 만든다
(labels ((name (params) RetType body...) ...) body...)   ; 상호 재귀할 수 있는 지역 함수 정의
(apply f arg1 ... argN rest-list)   ; rest-list를 펼쳐 f(&rest를 가진 가변 인수 함수)를 호출한다
```

이름 붙은 함수도 그대로 값으로 넘길 수 있다(고차 함수의 인수 등으로).

## 7. 기타 특수 형식

```lisp
(setq var value ...)                ; CL의 변수 대입. (setf var value)를 늘어놓은 것뿐. defmacro
(psetq var value ...)               ; 병렬 대입. 모든 값을 먼저 평가한 뒤 대입한다. defmacro
(psetf place value ...)             ; psetq를 place로 일반화한 것(같은 전개). defmacro
(setf place value)                  ; place에 대입한다. place는 변수 이름 / var::field /
                                     ; (accessor recv key...) 형태의 호출. recv의 정적
                                     ; 타입에 set-{accessor}라는 인스턴스 메서드가 있으면
                                     ; 성립한다(Vector<T>와 HashTable<K,V>의 get은 예외로
                                     ; set이 대응하고, 그 밖에는 set-접근자이름).
                                     ; 값은 대입한 값이다(CL과 같다). 그래서
                                     ; (if c (setf x 1) ())에서 then과 else의 타입이 맞지 않는다
(incf place)  (incf place delta)    ; place += delta(생략하면 delta=1). 결과는 setf와 같다
(decf place)  (decf place delta)    ; place -= delta(생략하면 delta=1)
(rotatef place1 place2 ... placeN)  ; N개의 place를 순환 이동한다(새 place1=옛 place2, ...,
                                     ; 새 placeN=옛 place1). 각 place의 하위 식은 한 번만 평가된다
(shiftf place1 ... placeN newvalue) ; place2..N의 값을 왼쪽으로 옮기고 placeN에 newvalue를 넣는다.
                                     ; 반환값은 옛 place1의 값
(list e1 e2 ... en)                 ; (cons e1 (cons e2 (... ())))로 전개된다. 인수가 0개면 ().
                                     ; 각 요소는 암묵적으로 Sexpr로 변환된다(CL의 cons처럼 어떤
                                     ; 값이든 가질 수 있다). 스칼라(int/i32/f64/ratio/char/bool/string/
                                     ; symbol)는 대응하는 Sexpr 변형에 감싸지고, defstruct/defenum/
                                     ; Vector<T>/HashTable<K,V> 등은 그대로 들어간다(변환
                                     ; 비용 없음). &rest/format의 인수도 같다.
(source-file)                       ; 이 형식을 읽은 파일 이름(string). 검사 시에
                                     ; 상수로 정해진다. CL의 *load-pathname*에 해당하지만 변수가 아니다.
                                     ; 모듈의 본체는 검사 뒤에 실행되므로 "지금 읽는 중"은
                                     ; 믿을 수 없지만, 검사 시에는 항상 알려져 있다.
                                     ; 파일이 아닌 소스는 리더가 부르는 이름(<stdin>/<input>)
(quote datum)                       ; 'datum과 같다. 평가하지 않고 Sexpr 데이터로 반환한다
(quasiquote template)               ; `template과 같다. ,/,@로 식을 템플릿에 넣는다
(documentation name)                ; name(맨 이름 또는 Type::method)의 문서 문자열을 Option<string>으로 반환한다
(panic message)                     ; message: string. 복구할 수 없는 오류로 비정상 종료한다. 타입은 !
(unreachable)                       ; (panic "unreachable")로 전개된다. defmacro
(todo)                              ; (panic "todo")로 전개된다. defmacro
(as Type expr)                      ; 수/문자의 타입 변환. 실패할 수 있는 변환은 실패하면 panic
(try-as Type expr)                  ; as와 같지만 결과를 Option<Type>으로 반환한다(실패하면 None)
(print control args...)             ; 서식을 전개해 표준 출력에 쓴다(줄바꿈 없음)
(println control args...)           ; 같다(끝에 줄바꿈)
(format dest control args...)       ; CL의 format. 전개한 string을 반환한다
(pprint x)                          ; 프리티 프린트한다. CL처럼 먼저 줄바꿈을 쓴다
(pprint-fill x)                     ; 채우기 레이아웃
(pprint-linear x)                   ; 모두 한 줄 또는 한 줄에 한 요소
(pprint-tabular x [colinc])         ; 표 레이아웃(기본 16열)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; 논리 블록을 직접 만든다
```

`print`/`println`/`format`/`pprint` 계열은 특수 형식이므로 가변 인수(`pprint` 계열은 대상 하나)는 각자의 타입으로 `Sexpr`에
감싸져 넘겨진다. `(println "~a" my-struct)`가 그대로 동작하는 것은 이 때문이다. 서식 지시자와 프리티 프린터의 자세한
내용은 [서식 지시자](functions/format.md)와 [출력](functions/printing.md#4-프리티-프린터)에 있다.

`as`/`try-as`가 다루는 것은 수와 문자의 카탈로그(`int`, 고정 폭 정수 타입, `f32`/`f64`/`ratio`/`char` 사이)뿐이다. 같은
타입은 변환하지 않는다. **정수 폭 사이(`int` 포함)와 `f32`↔`f64`는 진짜 변환이다.** `as`는 자르거나 반올림하고, `try-as`는
그 폭(정밀도)에 들어가는지 답한다. `(as int x)`는 고정 폭에서의 정확한 확대이고 `(as i32 n)`은 `int`에서의 절단이다. 정수 →
`char`는 범위를 벗어나 실패할 수 있으므로 `as`는 panic하고 `try-as`는 `None`이 된다. 그 밖(확대와
`float->int`/`ratio->int`의 절단)은 항상 성공한다. `float->int`/`ratio->int`/`char->int`는 `int`에 내려앉으며, 더 좁은 폭을
요구하면 그 뒤에 `int->W`를 호출한다. 대응하는 변환 메서드([수](functions/numbers.md)의 `int->char`/`int->int`/`int->W` 등)로
전개되는 편의 표기이다.

`documentation`은 `quote`/`compile`처럼 `name`을 평가하지 않고 평가되지 않은 맨 심볼 / `::` 경로로 읽는 특수 형식이다. CL의
`(documentation 'name 'function)`과 달리 타입 인수를 받지 않는다. `name`을 변수 → 함수 → 타입 → 트레이트 → 매크로 순서로
해석하고(맨 식별자를 식으로 평가할 때와 같은 우선순위) 찾은 정의의 문서 문자열을 반환한다(`(documentation Type::method)`는
메서드 전용). 해석 자체가 실패하면(그 이름의 정의가 없으면) 검사 시 오류이고, 정의는 있지만 문서 문자열이 없으면
`Option::none`이다. 모두 검사 시에 상수로 정해지며 실행 시의 조회는 일어나지 않는다. 모듈로 한정한 자유 이름(`mod::name`.
`Type::method` 제외)은 지원하지 않는다.

## 8. 비지역 탈출(catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; body를 실행한다. body가 닿는 범위 어디에서든
                                    ; (throw 'tag v)가 일어나면 그 v가 값이 된다
(throw 'tag value)                  ; 동적으로 가장 가까운 바깥의 (catch 'tag ...)로 빠져나간다
(unwind-protect protected cleanup)  ; protected를 어떻게 빠져나가든 cleanup을 실행한다
```

`break`/`return`(5장)과 달리 이것은 **동적인** 탈출이다. `throw`는 자신을 감싸는 `catch`를 렉시컬하게 찾지 않고, 함수 호출이
몇 겹이든 같은 태그의 `catch`에 닿는다.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; 찾지 못하면 평소대로 마지막 값
```

- **태그는 리터럴 심볼만**(`'done`). CL과 달리 평가되지 않는다.
- **태그는 타입을 나른다.** `'tag`를 처음 쓸 때 타입이 정해지고, 이후 같은 심볼의 `throw`/`catch`는 모두 그것과 대조된다.
  다른 타입으로 쓰면 타입 오류이다.
- `throw`의 타입은 `!`(발산)이다. `(catch 'tag expr)`의 타입은 `expr`의 타입과 태그의 타입을 합친 타입이다.
- `unwind-protect`의 값은 `protected`의 값이다. `cleanup`의 값은 버려진다. `protected`를 어떻게 빠져나가든 `cleanup`은
  실행된다. 정상 종료, `throw`, panic에 더해 `break`/`return`/`return-from`으로 빠져나갈 때도 실행된다. `cleanup` 자신의
  비지역 탈출은 진행 중인 탈출을 이긴다.
- 중첩된 `unwind-protect`는 안쪽부터 차례로 실행된다. `protected` **안의** 루프를 빠져나가는 `break`는 `protected`를
  빠져나간 것이 아니므로 그 `cleanup`은 실행되지 않는다.

CL의 컨디션(`define-condition`/`handler-bind`/`invoke-restart`)은 채택하지 않았다. 정적 타입과 맞지 않으므로 복구할 수 있는
실패는 `Result`로 나타낸다(9장).

## 9. 오류 처리 방침

- 복구할 수 있는 실패는 `Result<T,E>` + `match`. 복구할 수 없는 실패(버그, 불변식의 붕괴)는 `panic`.
- `?`/try에 해당하는 구문은 없다. 분기는 `match`로 명시적으로 쓴다.
- 함수와 특수 형식의 이름에 `!`(파괴적 연산)나 `?`(술어)를 접미사로 쓰지 않는다. 술어는 `-p`/`p` 접미사(`zerop` `consp` 등)나
  앞에 `is-`(`is-some` `is-ok` 등)를 붙여 이름 짓는다.

## 10. 컴파일

```lisp
(compile name)                      ; 이미 정의된 defun/메서드를 네이티브 코드로 JIT 컴파일한다
(compile-file src-path out-path)    ; 소스 파일을 네이티브 실행 파일로 AOT 컴파일한다(마지막 `(main)`은 건너뛴다)
(dump path)                         ; 현재 환경(타입 정보 + 컴파일된 본체)을 하나의 파일에 쓴다
(disassemble name)                  ; 그 정의가 무엇이 되었는지 출력한다(기본은 호스트의 기계어, 두 번째 인수가 true면 LLVM IR)
```

`compile`은 특수 형식이며 `name`은 평가되지 않고 평가되지 않은 맨 심볼 / `::` 경로로 읽힌다(문자열은 타입 오류). 제네릭
함수는 대상이 될 수 없다. 타입마다의 사본은 쓰이는 곳마다 만들어지므로 단일한 컴파일된 본체가 존재하지 않는다.
**해석할 수 없는 이름은 검사 시 오류이며** 실행 시로 미뤄지지 않는다(타입은 있지만 그 메서드가 없음 / 타입도 함수도 없음 /
정의되지 않은 맨 이름에 대해 각각 다른 메시지가 있다). 여기서의 가시성은 다른 참조와 똑같이 다루어지며, "있지만 여기서
보이지 않는다"도 "해석할 수 없다"처럼 검사 시에 실패한다.

호출되는 쪽도 전이적으로 컴파일되므로 **(간접적으로라도) 컴파일할 수 없는 것을 호출하는 함수는 컴파일할 수 없다.**
프로세스가 죽지는 않으며, 그렇다는 오류로 거절된다. 내장 함수는 모두 컴파일할 수 있으므로 이렇게 거절되는 것은 다음의
인터프리터 전용 연산을 호출하는 함수뿐이다.

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

인터프리터 전용인 것은 `compile`/`compile-file`/`dump`와 `trace`/`untrace`/`step`/`disassemble`이다
([구현 도구](functions/system.md#5-구현-도구clhs-252)). 컴파일할 수 없다기보다 컴파일하는 쪽의 연산이다(`dump`가 써 내는
것은 인터프리터의 환경 자체이며 AOT 실행 파일에는 그 환경이 없다. `trace`가 지켜보는 것과 `step`이 멈추는 곳은 실행 중인
인터프리터의 호출 경로이며, `disassemble`은 컴파일러 자체를 쓴다). `room`/`dribble`/`ed`는 여기에 들지 않으며 보통으로
컴파일할 수 있다.

컴파일할 **수 있는** 것: 스트림과 파일 입출력, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, 초월 함수, 비트 연산,
`catch`/`throw`/`unwind-protect`, `eq`/`eql`/`equal`/`equalp` 네 가지 모두(그래서 `case`도 모든 타입에 대해 컴파일된다),
`print`/`println`/`format`/`pprint`와 `pprint-logical-block`을 포함한 출력 기능 전부, `read`, 그리고 `eval`. 표준
라이브러리는 컴파일된 상태로 함께 제공된다.

AOT 실행 파일에는 프로그램이 쓰는 기능만 들어간다. 출력하지 않는 프로그램에는 서식 엔진이, `read`를 호출하지 않는
프로그램에는 리더가, `eval`을 호출하지 않는 프로그램에는 검사기와 인터프리터가 들어가지 않는다.

명령줄에서는 `typl -c src-path [-o out-path]`(`-c`는 `--compile`로도 쓸 수 있다)가 `compile-file`과 같은 일을 한다. `-o`를
생략하면 출력은 `src-path`에서 확장자 `.typl`을 뗀 이름이 된다. 실행 파일에 링크하는 정적 라이브러리
`libtypelisp_front.a`는 기본적으로, 릴리스 빌드의 `typl`이면 그 안에 가진 것을 처음 링크할 때
`$TYPELISP_HOME/lib/<빌드 ID>/`(`TYPELISP_HOME`이 없으면 `~/.typelisp/lib/<빌드 ID>/`)에 써 내어 쓰고, 디버그 빌드이면
`typl`을 빌드한 곳의 것을 쓴다. `typl --remove-lib`는 그 `typl`이 써 낸 라이브러리를 지운다. `--others`를 붙이면 다른 빌드
ID의 것을, `--all`을 붙이면 모든 빌드 ID의 것을 지운다. `typl --lib-dir DIR`를 지정하면 `DIR`의 라이브러리를 쓰며(`-c`와
`compile-file` 모두에 유효), 거기에 없으면 시작할 때 오류가 된다.

### 10.1 덤프

```lisp
(dump "session.typld")     ; 써 낸다
```
```sh
typl --image session.typld prog.typl   # 거기서 시작한다
typl --image session.typld             # REPL도 마찬가지
```

덤프는 타입 정보와 컴파일된 본체를 하나의 파일에 담은 것이다. `(dump path)`가 써 내는 것은 현재 세션이 읽어 들인 것(표준
라이브러리, 또는 `--image`로 넘긴 덤프)에 **세션 자신이 정의한 것**을 더한 것이다. 그래서 출력은 자기 완결적이며, `typl
--image`로 같은 환경을 띄울 수 있다. 세션에서 `(compile f)`한 것은 컴파일된 형태로 써 낸다.

저장되는 것은 **역사가 아니라 정의이다**.

- 세션의 최상위 식(`(println ...)` 등)은 포함되지 않는다. 읽을 때 다시 실행되면 곤란하기 때문이다.
- 전역 변수는 덤프할 때의 값이 아니라 **초기화 식을 다시 실행한 값**으로 복원된다. SBCL의 `save-lisp-and-die`(힙을 그대로
  써 낸다)와 일부러 다르게 한 것이며, 이 선택으로 열린 스트림, 클로저의 함수 포인터, 외부 메모리 같은 "저장할 수 없는 값"
  이라는 문제군 전체가 사라진다.
- `save-lisp-and-die`와 달리 **프로세스는 끝나지 않는다.** 써 내는 것이 이미지를 망가뜨리지 않기 때문이다.

덤프에는 그것을 쓴 구현의 표준 라이브러리와 컴파일러의 버전이 기록된다. 버전이 다른 `typl`로 읽으면 오류가 되며, 조용히
받아들이는 일은 없다.

### 10.2 AOT 실행 파일 안의 `eval`

`eval`은 "현재의 전역 환경"에 대해 타입 검사한 뒤 평가한다([해석과 평가](functions/system.md#6-해석과-평가)). 그 환경 —
검사기가 참조하는 시그니처, 타입, 매크로의 표와 인터프리터가 실행할 수 있는 본체 — 은 **기계어 안에 없다.** 컴파일된
함수는 어떤 주소에 놓인 심볼일 뿐이며, 인수의 타입도 이름으로 본체를 찾는 표도 가지지 않는다.

그래서 `compile-file`은 `eval`을 호출하는 프로그램에 대해서만 **컴파일 시에 그 환경을 조립해 실행 파일에 써 넣는다.**
형식은 덤프와 같으며 표준 라이브러리 부분과 프로그램 자신의 부분을 포함한다. 시작할 때는 복원만 하며, 소스를 다시 읽거나
타입 검사를 다시 하지 않는다. `eval`을 호출하지 않는 프로그램에는 아무것도 더하지 않는다.

결과:

- **시작이 오래 걸리고 실행 파일이 커진다.** 검사기와 인터프리터의 코드, 환경의 스냅숏이 들어가기 때문이다. 힙도 조금 크게
  잡는다.
- **eval의 형식은 인터프리트된다.** eval하는 형식이 프로그램 자신의 함수를 호출하더라도 실행되는 것은 스냅숏이 가진
  인터프리트용 본체이다. 결과는 같고 속도만 다르다.

전역 변수의 저장소는 컴파일된 코드와 **공유된다**(같은 슬롯). `defvar`의 초기화 식은 컴파일된 초기화가 한 번 실행하고
복원은 그것을 건너뛴다. 부작용이 있는 초기화 식이 두 번 실행되지 않도록 하기 위해서이다.

`compile-file`은 표준 라이브러리도 읽으므로(그 본체를 실행 파일에 넣는다) `abs`/`gcd` 같은 표준 라이브러리 함수와
`(impl print-object ...)`, `(defmethod print-object ...)`도 AOT에서 쓸 수 있다.

`compile-file`은 `use`(그리고 `import`/`shadowing-import`)도 받는다. 시작점 파일의 `(use m)`은 `typl file.typl`과 같은 규칙으로
파일을 찾고, 찾은 의존 파일도 컴파일되어 실행 파일에 링크된다. `main.typl`이 `(use http)`로 `http.typl`을 읽는 구성도 그대로
AOT 컴파일할 수 있다. 시작점 파일 자신의 정의도 `typl file.typl`처럼 파일 이름의 모듈에 들어간다(`p.typl`의 `point`는
`p::point`). 그래서 값의 표시(`#<p::point x: 1 y: 2>`)는 어느 쪽으로 실행하든 같다.

## 11. 리더 매크로(readtable)

리더가 **어떤 문자를 만났을 때 무엇을 할지**를 프로그램에서 바꿀 수 있다(CLHS 23.1).

```lisp
(set-macro-character c f)             ; f가 문자 c를 읽는다
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f가 두 글자 열 d s를 읽는다
(get-dispatch-macro-character d s)    ; Option<f>
```

`f`의 타입은 `(fn (string-input-stream char) Option<Sexpr>)`이다. 첫 번째 인수는 **아직 읽지 않은 텍스트를 내용으로 하는
스트림**, 두 번째 인수는 **발동한 문자**(디스패치이면 두 번째 문자)이다. 반환값이 그 자리에서 읽은 데이터가 된다. 스트림이
`:dyn PeekInput`이 아니라 구체 타입인 것은 리더가 넘기는 것이 항상 이 한 종류이기 때문이다. `read-sexpr` / `read-char` /
`peek-char` / `unread-char` / `read-delimited-list`는 모두 `(where (PeekInput S))`이므로 구체 타입에 그대로 모두 쓸 수 있다.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => (not (equal 1 2))로 읽혀 true
```

리더는 **내장 구문보다 먼저 매크로 문자를 보므로** `(`와 `'`도 빼앗을 수 있다. `#`의 하위 문자의 등록은 내장
`#b`/`#x`/`#.`보다 우선한다. `#` 이외의 문자도 `set-dispatch-macro-character`에 넘기면 그 자리에서 디스패치 문자가 된다.
CL의 `make-dispatch-macro-character`에 해당하는 것은 **없다.** 등록이 이미 그 역할을 하므로 별도의 단계로 두어도 할 일이
없다.

**언제 효과가 있는지**는 `#.`(1장)과 같이 읽기 경로에 따라 다르다.

- REPL과 `(load ...)`는 형식 하나씩 실행하므로 **앞의 형식에서 정의한 함수**를 그대로 등록할 수 있다.
- 모듈 파일은 하나의 단위로 검사되고 나중에 실행되므로 `set-macro-character` / `set-dispatch-macro-character`는 **호출만
  즉시 실행된다**(CL의 `(eval-when (:compile-toplevel) ...)`의 역할). 즉시 실행되므로 **넘기는 함수는 그 시점에 이미 있어야
  한다.** 같은 파일의 `defun`은 아직 실행되지 않았으므로 `lambda`로 쓰거나 표준 라이브러리나 이미 실행된 것을 쓴다. 대상은
  최상위의 호출뿐이며 `progn`이나 `let` 안은 보지 않는다.

내장 `read` / `read-from-string`도 readtable을 참조한다(CL과 같다).

**없는 것**: `*readtable*`와 `copy-readtable`, 그리고 `readtable-case`. 앞의 둘은 readtable이 **값이 아니기** 때문이다. 값이
되려면 "리더에 건넬 수 있는 것"이어야 하는데, 소스를 읽는 리더는 프로그램 밖에 있어 건넬 곳이 없다. `readtable-case`는 1장이
이 언어의 리더는 항상 소문자로 만든다(CL의 `:downcase`)고 정했기 때문이다.


## 12. 동시성(태스크)

**태스크는 경량 스레드이며**(Go로 말하면 `go` 문이 시작하는 것) 협조적으로 실행된다(선점이 없다). 전환은 커널을 거치지
않고 실행 상태는 기계 스택이 아니라 힙에 있으므로 태스크는 많이 만들어도 비용이 적다.

**태스크는 여러 OS 스레드에서 동시에 실행된다**(멀티코어 병렬). 스레드 수는 환경 변수 `TYPELISP_THREADS`로 정한다(`main`을
실행하는 스레드를 포함한 총수. 기본값은 기계의 병렬도). `typl`에서는 **컴파일된 태스크만** 다른 스레드에서 실행되고,
인터프리트되는 태스크는 인터프리터의 스레드에서 실행된다(12.7). 공유 데이터는 `Mutex<T>`나 `Chan<T>`를 거친다. 거치지 않는
동시의 읽기와 쓰기는 Go처럼 정의되지 않는다(12.7).

어휘 중 **특수 형식은 `task` / `thread` / `select` 셋뿐이며**, 나머지는 보통의 함수, 메서드, 매크로이다
([태스크와 채널](functions/concurrency.md)).

### 12.1 `task` — 태스크 시작

```lisp
(task (f arg...))                   ; Task<T>를 반환한다. T는 f의 반환 타입
```

**호출 형식만 받는다.** `f`와 각 `arg`는 `task`를 쓴 곳에서 쓴 순서대로 평가되고, 새 태스크에서 일어나는 것은 **호출**뿐이다.
Go의 `go f(x)`와 같은 규칙이며, thunk가 아니라 호출 형식을 받는 이유이기도 하다. thunk는 인수를 평가하지 않은 채 포착해
버린다.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i는 매번 그 자리에서 평가된다. 포착의 함정이 없다

(task ((lambda () ()                ; 임의의 본체를 실행하려면 lambda를 호출한다
         (println "start")
         (send ch 1))))
```

특수 형식(`if` / `let` / `progn` …)은 `task` 바로 아래에 쓸 수 없다.

**함수로 만들 수 없는 이유**: `(spawn (lambda () T body...))`라고 쓰면 `T`를 써야 한다. `lambda`에는 반환 타입 주석이
필수이고, 매크로는 `(f a b)`의 반환 타입을 모른다. 그것을 아는 것은 검사기뿐이다.

### 12.2 `thread` — 전용 OS 스레드에서 태스크 시작

```lisp
(thread (f arg...))                 ; Thread<T>를 반환한다. T는 f의 반환 타입
(join th)                           ; 완료를 기다려 그 값을 반환한다(몇 번이든)
```

형식과 평가 규칙은 `task`와 같다(호출 형식만 받고, `f`와 `arg`는 쓴 곳에서 평가된다). 다른 것은 실행되는 곳이다. **그
태스크 전용의 OS 스레드를 하나 시작해 그 위에서만 실행한다.** 다른 태스크와 다중화되지 않으므로 안에서 블로킹하는 C
함수(`defffi`)를 호출해도 멈추는 것은 그 스레드뿐이고 다른 태스크는 진행된다. 안에서는 `task`, `send`, `recv` 등을 그대로
쓸 수 있다.

- `Thread<T>`는 `Task<T>`의 짝이다. `join`은 `wait`처럼 **호출한 태스크**를 멈추며 값은 캐시된다. 태스크가 끝나면 스레드도
  끝난다.
- panic의 규칙은 `task`와 같다(프로세스 전체가 멈춘다). `main`이 반환하면 프로세스가 끝난다.
- 함수로 쓰려면 `(Thread::spawn (lambda () T body...))`(Rust의 `std::thread::spawn`)를 쓴다. 이름 붙은 함수를
  넘겨도 된다.
- **전용 스레드에서 실행되는 것은 컴파일된 코드뿐이다.** `typl`이 인터프리트 중에 `(thread (f ...))`나 `Thread::spawn`을
  평가하면 그 자리에서 실행할 함수(와 그것이 호출하는 것)를 컴파일한 뒤 실행한다. 컴파일할 수 없는 것 — 바깥의 지역 변수를
  참조하는 `lambda`, 구조체의 생성 등 — 은 스레드를 시작하기 전에 `(panic ...)`과 똑같이 다루어지는 panic이 된다. 지역 변수를
  참조하는 `lambda`도 컴파일된 함수 안에서 만들면 넘길 수 있다.

### 12.3 `select` — 여러 채널 연산을 동시에 기다리기

```lisp
(select
  ((v (recv ch1)) body...)          ; 받기 갈래. v는 Option<T>에 묶인다
  ((send ch2 x) body...)            ; 보내기 갈래
  (else body...))                   ; 생략할 수 있다. **쓴다면 마지막에**
```

- **`else`가 있으면 블로킹하지 않는다**(Go의 `default`). 없으면 어느 갈래가 가능해질 때까지 기다린다.
- **동시에 여럿이 가능하면 무작위로 하나를 고른다**(쓴 순서대로라면 뒤의 갈래가 굶는다).
- 받기 갈래의 `v`는 **`Option<T>`**이다. 닫힌 채널은 갈래를 건너뛸 이유가 아니라 "답"이므로 갈래 안에서 `match`한다.
- 타입은 **모든 갈래 본체의 타입을 합친 타입**이다(`match` 갈래와 같은 규칙).
- 갈래가 0개인 `(select)`는 타입 오류이다(Go의 `select{}` = 영원히 블로킹은 채택하지 않는다). `else`만 있는 `select`도
  마찬가지이다. 본체를 그대로 쓰는 것과 같기 때문이다.

**어느 갈래가 선택되든 채널 식과 보낼 값은 왼쪽에서 오른쪽으로 각각 한 번 평가된다**(`case`가 키에 대해 가진 규율과 같다).

```lisp
(select                             ; 타임아웃 있는 받기
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after`([시간이 지나면 전달되는 채널](functions/concurrency.md#5-after--시간이-지나면-전달되는-채널))는 "`sec`초 뒤에 값을
하나 전달하는 채널"이며 Go의 `time.After`에 해당한다.

### 12.4 다른 기능과의 관계

| 기능 | 태스크와의 관계 |
|---|---|
| `catch` / `throw` | **태스크 경계를 넘지 않는다.** 태스크의 본체를 빠져나가려는 `throw`는 panic |
| `unwind-protect` | 태스크가 자연스럽게 끝나면 cleanup이 실행된다. **주 태스크가 끝나서 프로세스가 끝날 때는 실행되지 않는다** |
| `block` / `return-from` | 렉시컬하므로 `lambda` 경계를 넘지 않는다 |
| `panic` | Go처럼 프로세스 전체가 멈춘다. `wait`는 panic을 값으로 관찰하지 않는다 |
| `dlet` | **태스크마다의 바인딩이 아니다.** 여전히 "전역 변수를 빌려 쓰고 돌려주는" 것이므로 태스크끼리 서로 간섭한다 |
| 표준 출력 | 모든 태스크가 공유한다. 하나의 `println`의 출력이 줄 중간에서 다른 출력과 섞이는 일은 없다 |
| `compile` / `eval` | 제한 없음. 태스크 안의 `(compile f)`도 통과한다 |

### 12.5 전환이 일어나는 곳

협조적 스케줄링이므로 **쓴 곳에서만 전환된다**: `(yield)`, `(sleep ...)`, `(wait ...)`, **기다려야 하는 채널 연산**
(`send`/`recv`/`select`), 그리고 **기다려야 하는 소켓 연산**(`accept` / `tcp-connect`(이름 해석 포함) / 소켓의 읽기와 쓰기 /
`recv-from`, [네트워크](functions/network.md)). 소켓은 모두 논블로킹이며, 준비되지 않았으면 그 태스크만 멈추고 OS가 준비되었다고
답하면 재개한다. Go의 netpoller와 같은 모양이다. 실행할 수 있는 태스크가 없을 때만 구현이 가장 가까운 `sleep` 기한까지
OS를 기다린다.

그 자리에서 답할 수 있는 채널 연산 — 버퍼에 빈자리가 있는 `send`, 값이 있는 `recv`, `(len ch)`/`(cap ch)`/`(close ch)`/
`(Chan::new n)` — 은 **차례를 소비하지 않는다.** 읽기로 뜻밖에 끼어들림을 당하지 않는다는 뜻이며, CL식의 "0초 동안 양보"인
`(sleep 0.0)`과 구별된다.

**선점은 없다.** 아무것도 호출하지 않는 촘촘한 루프는 다른 태스크를 굶긴다. 다만 컴파일된 루프는 주기적으로 스케줄러에
제어를 넘기므로 컴파일된 촘촘한 루프는 다른 태스크를 굶기지 않는다.

### 12.6 컴파일된 코드와 태스크

컴파일된 코드도 태스크를 멈출 수 있다. `compile-file`로 만든 실행 파일도 마찬가지로 `main`이 스케줄러의 주 태스크로
실행되며, `task`, `sleep`, `wait`, 채널, 소켓 대기가 모두 `typl`과 같은 의미로 동작한다. `main`이 반환하면 프로세스가 끝나고
나머지 태스크는 중단된다(Go와 같다). 스케줄러 때문에 인터프리터를 실행 파일에 넣는 일은 없다.

예외는 "C의 FFI 콜백 안"뿐이며, 거기서는 **기다려야 하는** 연산이 오류이다(조용히 교착 상태가 되는 것보다 친절하다).
`defffi`로 넘긴 함수가 C에서 호출되는 동안에는 C의 스택이 위에 쌓여 있어 태스크를 멈췄다가 나중에 재개할 수단이 없다.

다음 곳도 태스크 도중에 호출되는 함수이지만 멈출 수 없다: `print-object` 메서드, `format`의 `~/name/`, 리더 매크로, `eval`
안, AOT 실행 파일의 `defvar` 초기화 식. 여기서는 **기다리지 않고 답이 나오는 연산은 통과하고**(버퍼에 값이 있는
`(recv ch)`, 이미 데이터를 받은 소켓의 `read-line`, `(task ...)`, `(yield)` 등) **정말로 기다려야 하는 연산은 오류이다**(그
자리에서 프로세스를 멈추는 것이 아니라 `` `recv` cannot block: ... `` 같은, `(panic ...)`과 똑같이 다루어지는 panic).

### 12.7 Go와의 차이

- **`typl`에서 다른 스레드로 나가는 것은 컴파일된 태스크뿐이다.** 인터프리터의 상태는 스레드 사이에서 공유할 수 없으므로
  인터프리트되는 `task`의 태스크는 인터프리터의 스레드에서 실행된다. 컴파일된 태스크도 인터프리트되는 함수 값을 호출하거나,
  아무도 컴파일하지 않은 `:dyn` 메서드를 호출하거나, `eval`/`macroexpand`/`read`를 호출하는 시점에 **인터프리터의 스레드로
  옮겨 가서 그 뒤로 거기에 머문다**(돌아가지 않는다). 긴 처리 도중에 인터프리트되는 코드를 한 번이라도 건드리면 나머지는
  인터프리터의 스레드에서 실행된다.
- **`typl`의 워커는 최상위 평가 한 번 동안만 산다.** REPL이 입력을 기다리는 동안과 최상위 형식 사이에서는 다른 스레드가
  태스크를 진행하지 않는다(남은 태스크는 다음 평가에서 이어서 실행된다). 평가가 끝날 때 각 스레드가 지금의 한 단계를 마치기를
  기다리므로, `thread` 안에서 계속 블로킹하는 C 함수(`defffi`)가 있으면 그것이 반환될 때까지 평가가 끝나지 않는다.
- **워커에서의 출력**: 인터프리트되는 `print-object` / `~/name/` 메서드는 다른 스레드에서 실행할 수 없으므로 그런 값을 다른
  스레드에서 출력하면 `(panic ...)`과 똑같이 다루어지는 panic이 된다(`(compile T::print-object)`, 또는 주 태스크에서 출력한다).
- **데이터 경합은 정의되지 않는다**(Go와 같은 입장). `Mutex<T>`・`Chan<T>`를 거치지 않고 여러 태스크가 같은 값을 바꾼 결과는
  보장되지 않는다.
- **`task`는 값을 반환한다.** Go의 `go` 문과 달리 `Task<T>`를 반환하며 `(wait t)`로 결과를 얻을 수 있다.
- **nil 채널이 없다.** Go의 팬인 관용구(닫힌 채널을 `nil`로 해서 `select`의 갈래에서 빼기)를 쓸 수 없으므로 입력마다 태스크를
  시작해 `WaitGroup`으로 합류한다([WaitGroup](functions/concurrency.md#4-waitgroup--n개의-완료-기다리기)). Go에서도 권장되는
  쓰는 법이지만, **Go에서 온 사람이 가장 먼저 부딪치는 차이**이다.
